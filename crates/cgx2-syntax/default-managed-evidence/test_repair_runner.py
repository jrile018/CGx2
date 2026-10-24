"""Adversarial tests for evidence acceptance, not the production implementation."""
import copy
import json
import os
from pathlib import Path
import signal
import sys
import tempfile
import unittest

import repair_runner as runner


class AcceptanceTests(unittest.TestCase):
    def record(self, exit=1, reason='exit', signal=None, stderr=''):
        return dict(exit=exit, reason=reason, signal=signal, stderr=stderr, stdout='')

    def rejection(self):
        expected = runner.rejection_cases()[0][2]
        row = {**expected[0], 'severity': 'error'}
        return self.record(stderr=json.dumps(row)), expected

    def test_correct_diagnostic_is_accepted(self):
        record, expected = self.rejection()
        self.assertTrue(runner.rejection_matches(record, expected))

    def test_wrong_diagnostic_exit_one_is_rejected(self):
        record, expected = self.rejection()
        row = json.loads(record['stderr']); row['code'] = 'CGT001'
        record['stderr'] = json.dumps(row)
        self.assertFalse(runner.rejection_matches(record, expected))

    def test_wrong_span_or_message_is_rejected(self):
        for field, value in [('span', dict(start=0, end=71)), ('message', 'unrelated failure')]:
            record, expected = self.rejection()
            row = json.loads(record['stderr']); row[field] = value
            record['stderr'] = json.dumps(row)
            self.assertFalse(runner.rejection_matches(record, expected))

    def test_utf8_byte_end_is_required(self):
        _, _, expected = runner.rejection_cases()[1]
        row = copy.deepcopy(expected[0])
        row['span']['end'] -= 4  # character count instead of UTF-8 bytes
        self.assertFalse(runner.rejection_matches(self.record(stderr=json.dumps(row)), expected))

    def test_unexpected_exit_and_signal_cannot_reject(self):
        record, expected = self.rejection()
        for exit, reason, sig in [(0, 'exit', None), (2, 'exit', None), (-15, 'signal', 15), (1, 'timeout', None)]:
            record.update(exit=exit, reason=reason, signal=sig)
            self.assertFalse(runner.rejection_matches(record, expected))

    def test_extra_diagnostic_cannot_hide_unrelated_failure(self):
        record, expected = self.rejection()
        record['stderr'] += '\n' + record['stderr']
        self.assertFalse(runner.rejection_matches(record, expected))

    def test_artifact_replacement_and_deletion_are_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / 'sentinel'
            p.write_bytes(runner.SENTINEL)
            self.assertTrue(runner.preserved(p))
            p.write_bytes(b'replacement\n')
            self.assertFalse(runner.preserved(p))
            p.unlink()
            self.assertFalse(runner.preserved(p))

    def test_injected_error_exit_is_never_recovered_success(self):
        obs = dict(recovered=1, guard_entry_failures=0)
        record = self.record(stderr='CGX2_MANAGED_EVIDENCE ' + json.dumps(obs) + '\nCGR090: failure at 2..4')
        self.assertEqual(runner.classify(record), 'uncaught-runtime')
        record.update(exit=-signal.SIGABRT, reason='signal', signal=signal.SIGABRT)
        self.assertEqual(runner.classify(record), 'unexpected-exit-or-signal')

    def test_session_and_guard_entry_are_distinct(self):
        record = self.record(stderr='CGR090: failure at 0..72')
        self.assertEqual(runner.classify(record), 'uncaught-session-entry')
        record['stderr'] += '\nCGX2_MANAGED_EVIDENCE ' + json.dumps(dict(recovered=0, guard_entry_failures=1))
        self.assertEqual(runner.classify(record), 'uncaught-guard-entry')

    def test_duplicate_observation_is_invalid(self):
        record = self.record(stderr='CGX2_MANAGED_EVIDENCE {}\nCGX2_MANAGED_EVIDENCE {}')
        with self.assertRaises(ValueError):
            runner.accounting(record)


class SupervisorTests(unittest.TestCase):
    def run_child(self, source, cap=1024, timeout=2):
        return runner.supervise([sys.executable, '-c', source], cwd=Path.cwd(),
                                env=os.environ.copy(), limits=runner.limits, cap=cap, timeout=timeout)

    def test_unexpected_exit_is_retained(self):
        result = self.run_child('import sys; sys.exit(7)')
        self.assertEqual((result['reason'], result['exit']), ('exit', 7))
        self.assertFalse(runner.normal_exit(result))

    def test_signal_is_retained(self):
        result = self.run_child('import os,signal; os.kill(os.getpid(), signal.SIGTERM)')
        self.assertEqual((result['reason'], result['signal']), ('signal', signal.SIGTERM))
        self.assertFalse(runner.normal_exit(result, -signal.SIGTERM))

    def test_live_stdout_and_stderr_caps(self):
        for stream in ('stdout', 'stderr'):
            result = self.run_child(f'import os; os.write({1 if stream == "stdout" else 2}, b"x" * 1000000)')
            self.assertEqual(result['reason'], 'output_overflow')
            self.assertEqual(result['overflow_stream'], stream)
            self.assertLessEqual(len(result[stream]), 1024)

    def test_wall_timeout_is_retained(self):
        result = self.run_child('import time; time.sleep(10)', timeout=0.1)
        self.assertEqual(result['reason'], 'timeout')
        self.assertFalse(runner.normal_exit(result))


if __name__ == '__main__':
    unittest.main()
