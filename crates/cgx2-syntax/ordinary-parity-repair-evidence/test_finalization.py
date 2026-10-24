#!/usr/bin/env python3
"""Adversarial tests of the production validators, dispatch, and finalization."""
import copy
import hashlib
import json
from pathlib import Path
import signal
import sys
import tempfile
import unittest
from unittest.mock import patch

import common
from common import Journal, normal
import provenance
from validation import command_errors, inventory_errors, sentinel_status


def row(stderr='CGE006: main.cg:0..0: unsupported\n', exit=1):
    return dict(reason='exit', exit=exit, signal=None, stdout='', stderr=stderr)


class FinalizationTests(unittest.TestCase):
    def test_retained_audit_uses_the_actual_command_expectation(self):
        from finalization_audit import sentinel_code
        self.assertEqual(sentinel_code('cross-region-emit-python'), 'CGT095')
        self.assertEqual(sentinel_code('dependency-emit-python'), 'CGT095')
        self.assertEqual(sentinel_code('package-value-python'), 'CGE006')
        self.assertEqual(sentinel_code('package-error-python'), 'CGE006')

    def test_whole_diagnostic_result(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / 'sentinel'; path.write_bytes(b'prior\n')
            valid = row()
            self.assertEqual(sentinel_status(valid, path, b'prior\n', 'CGE006'), (True, True))
            for bad in (row(valid['stderr'] * 2), row('CGXC005: failure\n' + valid['stderr']),
                        row('CGXC005: unrelated mentions CGE006\n'), row('CGE006'), row('CGE006: '),
                        row('\n' + valid['stderr']), row(valid['stderr'] + 'extra output\n'),
                        dict(valid, stderr=None), dict(valid, stdout='unexpected'),
                        dict(valid, stdout=False), dict(valid, exit=True), dict(valid, exit=0),
                        {k: v for k, v in valid.items() if k != 'stderr'}, None):
                with self.subTest(bad=bad):
                    self.assertEqual(sentinel_status(bad, path, b'prior\n', 'CGE006'), (False, True))

    def test_sentinel_preservation_is_independent(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / 'sentinel'; path.write_bytes(b'changed')
            self.assertEqual(sentinel_status(row(), path, b'prior', 'CGE006'), (True, False))
            self.assertEqual(sentinel_status(row('CGXC005: CGE006'), path, b'prior', 'CGE006'), (False, False))

    def test_real_dispatch_rejects_duplicate_mixed_prose(self):
        with tempfile.TemporaryDirectory() as root:
            j = Journal(Path(root) / 'attempt'); path = j.output / 'sentinel'; path.write_bytes(b'prior')
            for name, text in [('duplicate', 'CGE006: unsupported\n' * 2),
                               ('mixed', 'CGXC005: failure\nCGE006: unsupported\n'),
                               ('prose', 'CGXC005: merely mentions CGE006\n')]:
                actual = j.run(name, [sys.executable, '-c', f'import sys;sys.stderr.write({text!r});sys.exit(1)'])
                self.assertEqual(sentinel_status(actual, path, b'prior', 'CGE006'), (False, True))
                j.check(name, not sentinel_status(actual, path, b'prior', 'CGE006')[0])
            self.assertEqual(j.finish(), 0)

    def test_actual_valid_success_and_expected_rejection_finish(self):
        with tempfile.TemporaryDirectory() as root:
            j = Journal(Path(root) / 'attempt')
            good = j.run('success', [sys.executable, '-c', 'print(42)'])
            rejected = j.run('expected-rejection', [sys.executable, '-c', 'raise SystemExit(1)'])
            j.required_commands = {'success', 'expected-rejection'}
            j.check('observations', normal(good) and normal(rejected, 1))
            self.assertEqual(j.finish(), 0); self.assertTrue(j.report['complete'])

    def test_actual_product_failure_is_red_with_complete_coverage(self):
        with tempfile.TemporaryDirectory() as root:
            j = Journal(Path(root) / 'attempt')
            actual = j.run('product', [sys.executable, '-c', 'raise SystemExit(1)'])
            j.check('expected-success', normal(actual))
            self.assertEqual(j.finish(), 1); self.assertTrue(j.report['complete'])
            self.assertEqual(j.report['commands'][0]['exit'], 1)

    def test_actual_timeout_signal_overflow_are_durable_incomplete(self):
        original = common.supervise
        def bounded(*args, **kwargs):
            kwargs.update(timeout=0.1, cap=128)
            return original(*args, **kwargs)
        for name, code in [('timeout', 'import time;time.sleep(3)'),
                           ('signal', 'import os,signal;os.kill(os.getpid(),signal.SIGTERM)'),
                           ('overflow', 'print("x"*4096)')]:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as root:
                j = Journal(Path(root) / 'attempt'); j.required_commands = {name}
                with patch.object(common, 'supervise', side_effect=bounded):
                    actual = j.run(name, [sys.executable, '-c', code])
                j.check('unrelated-valid-proof', True)
                self.assertEqual(j.finish(), 1); self.assertFalse(j.report['complete'])
                saved = json.loads((j.output / 'report.json').read_text())
                self.assertEqual(saved['commands'][0]['reason'], actual['reason'])
                self.assertTrue(saved['coverage_errors'])

    def test_invocation_exception_and_spawn_error_finish(self):
        for exception in (False, True):
            with tempfile.TemporaryDirectory() as root:
                j = Journal(Path(root) / 'attempt')
                if exception:
                    with patch.object(common, 'supervise', side_effect=OSError('fixture exception')):
                        with self.assertRaises(OSError): j.run('broken', ['fixture'])
                else:
                    j.run('broken', [str(j.output / 'absent-executable')])
                self.assertEqual(j.finish(), 1); self.assertFalse(j.report['complete'])

    def test_setup_exception_before_spawn_is_persisted(self):
        for empty in (False, True):
            with tempfile.TemporaryDirectory() as root:
                j = Journal(Path(root) / 'attempt')
                if empty:
                    with self.assertRaises((IndexError, ValueError)): j.run('empty', [])
                else:
                    with patch.object(common, 'digest', side_effect=OSError('executable hash unavailable')):
                        with self.assertRaises(OSError): j.run('hash-failed', [sys.executable])
                self.assertEqual(j.report['commands'][0]['reason'], 'exception')
                self.assertEqual(j.finish(), 1); self.assertFalse(j.report['complete'])

    def test_invalid_termination_structure(self):
        for change in ({'exit': True}, {'exit': -1}, {'exit': 256}, {'exit': None}, {'signal': 9},
                       {'reason': 'timeout'}, {'reason': 'output_overflow'}, {'overflow_stream': 'stderr'},
                       {'reason': 'pipe_cleanup_timeout'}, {'reason': 'pending'}, {'error': 'spawn'},
                       {'traceback': 'exception'}):
            self.assertTrue(command_errors(dict(row(), **change)))
        for key in ('exit', 'reason', 'signal'):
            self.assertTrue(command_errors({k: v for k, v in row().items() if k != key}))

    def test_missing_malformed_and_hash_mismatched_streams(self):
        with tempfile.TemporaryDirectory() as root:
            j = Journal(Path(root) / 'attempt')
            actual = j.run('valid', [sys.executable, '-c', 'print(42)'])
            self.assertFalse(command_errors(actual))
            for stream in ('stdout', 'stderr'):
                for key in (stream, stream + '_bytes', stream + '_sha256'):
                    bad = dict(actual); del bad[key]
                    self.assertTrue(command_errors(bad))
                for value in (None, False, [], {}):
                    self.assertTrue(command_errors(dict(actual, **{stream: value})))
                self.assertTrue(command_errors(dict(actual, **{stream + '_bytes': True})))
                self.assertTrue(command_errors(dict(actual, **{stream + '_sha256': '0' * 64})))

    def test_real_dispatch_cannot_be_replaced_with_minimal_or_missing_row(self):
        for strip in ('fields', 'row', 'executable'):
            with self.subTest(strip=strip), tempfile.TemporaryDirectory() as root:
                j = Journal(Path(root) / 'attempt')
                actual = j.run('real-command', [sys.executable, '-c', 'print(42)'])
                if strip == 'fields':
                    j.report['commands'] = [{k: actual[k] for k in ('name', 'reason', 'exit', 'signal')}]
                if strip == 'row': j.report['commands'] = []
                if strip == 'executable': del actual['executable_input']
                j.required_commands = set()  # Later dynamic inventories cannot erase dispatches.
                self.assertEqual(j.finish(), 1); self.assertFalse(j.report['complete'])

    def test_lossy_diagnostic_stream_fails(self):
        with tempfile.TemporaryDirectory() as root:
            j = Journal(Path(root) / 'attempt'); sentinel = j.output / 'sentinel'; sentinel.write_bytes(b'prior')
            actual = j.run('invalid-utf8', [sys.executable, '-c',
                'import sys;sys.stderr.buffer.write(b"CGE006: bad\\xff\\n");sys.exit(1)'])
            self.assertEqual(sentinel_status(actual, sentinel, b'prior', 'CGE006'), (False, True))

    def test_invalid_invocation_and_report_rows(self):
        for bad in (None, [], ['oops'], [{'name': []}], [{'name': 'ok', 'passed': 1}]):
            self.assertTrue(inventory_errors({'checks': bad}, set(), {'ok'}))
        for argv in ([], 'rustc', [None], ['']):
            self.assertTrue(command_errors(dict(row(), argv=argv, cwd='/')))

    def test_missing_failed_and_boolean_toolchain_observations(self):
        for value in (None, 1, True):
            with tempfile.TemporaryDirectory() as root:
                j = Journal(Path(root) / 'attempt')
                j.required_commands = {'rustc-identity', 'rustc-sysroot'}
                j.required_success_commands = set(j.required_commands)
                if value is not None:
                    j.report['commands'] = [dict(row(exit=value), name=n) for n in j.required_commands]
                j.check('unrelated-proof', True)
                self.assertEqual(j.finish(), 1); self.assertFalse(j.report['complete'])

    def test_failed_toolchain_helper_retains_prerequisite_disposition(self):
        with tempfile.TemporaryDirectory() as root:
            j = Journal(Path(root) / 'attempt')
            def fail(name, *args, **kwargs):
                actual = dict(row(exit=1), name=name); j.report['commands'].append(actual); return actual
            with patch.object(j, 'run', side_effect=fail):
                with self.assertRaises(ValueError): j.toolchain()
            self.assertEqual(j.finish(), 1); self.assertFalse(j.report['complete'])
            self.assertTrue(j.report['prerequisite_failures'])
            self.assertEqual({r['name'] for r in j.report['checks'] if r['passed'] is False},
                             {'rustc-identity', 'rustc-sysroot', 'required-evidence-coverage'})

    def test_missing_and_corrupt_checkout_identity(self):
        for change in (None, {}, {'head': '0' * 40}):
            with tempfile.TemporaryDirectory() as root:
                j = Journal(Path(root) / 'attempt'); j.report['source_identity'] = change
                self.assertEqual(j.finish(), 1); self.assertFalse(j.report['complete'])

    def test_candidate_commit_source_harness_and_reference_mismatch(self):
        from integrity import compiler_paths
        from common import ROOT
        current = provenance.checkout_identity()
        with tempfile.TemporaryDirectory() as root:
            reference = Path(root) / 'record'; reference.write_text(json.dumps({'source_commit': current['head']}))
            source = Path(root) / 'source.json'; source.write_text(json.dumps(current['compiler']))
            candidate = {'schema_version': 1, 'observing_harness': current,
                         'build': {'source_commit': current['head'],
                                   'original_invocation': {'source_commit': current['head']},
                                   'compiler': current['compiler'],
                                   'compiler_snapshot_sha256': current['compiler_snapshot_sha256'],
                                   'build_invocation_path': str(reference), 'compiler_source_manifest': str(source),
                                   'original_references': {str(reference): common.digest(reference), str(source): common.digest(source)}}}
            self.assertFalse(provenance.candidate_errors(candidate))
            for change in ('commit', 'both-commit-labels', 'compiler', 'harness', 'reference'):
                bad = copy.deepcopy(candidate)
                if change == 'commit': bad['build']['source_commit'] = '0' * 40
                if change == 'both-commit-labels':
                    bad['build']['source_commit'] = bad['build']['original_invocation']['source_commit'] = '0' * 40
                if change == 'compiler': bad['build']['compiler']['Cargo.toml'] = '0' * 64
                if change == 'harness': bad['observing_harness']['harness']['common.py'] = '0' * 64
                if change == 'reference': bad['build']['original_references'][str(reference)] = '0' * 64
                self.assertTrue(provenance.candidate_errors(bad))
            self.assertTrue(provenance.candidate_errors({}))

    def test_recorded_build_rejects_missing_or_mismatched_identity_first(self):
        with tempfile.TemporaryDirectory() as root:
            invocation = Path(root) / 'invocation.json'
            for value in ({}, {'source_commit': '1' * 40, 'dirty': '', 'exit': 0},
                          {'source_commit': '0' * 40, 'dirty': 'modified compiler', 'exit': 0},
                          {'source_commit': '0' * 40, 'dirty': '', 'exit': True}):
                invocation.write_text(json.dumps(value))
                with self.assertRaises(ValueError):
                    provenance.recorded_build(root, invocation, '0' * 40)


if __name__ == '__main__':
    unittest.main()
