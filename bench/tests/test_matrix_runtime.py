import importlib.util
import os
from pathlib import Path
import sys
import unittest


SPEC = importlib.util.spec_from_file_location(
    'matrix_runtime', Path(__file__).resolve().parents[1] / 'matrix_runtime.py')
runtime = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runtime)


class MatrixRuntimeTests(unittest.TestCase):
    def test_tiny_reference_and_mismatch_fail_closed(self):
        expected = runtime.reference(2, 3, 2)
        self.assertEqual(expected, [58, 64, 139, 154])
        correct = {'stdout': '[[58,64],[139,154]]'}
        self.assertEqual(len(runtime.checked_output(correct, expected, 'source')[0]), 64)
        with self.assertRaises(runtime.Invalid):
            runtime.checked_output({'stdout': '[[58,64],[139,155]]'}, expected, 'source')
        with self.assertRaises(runtime.Invalid):
            runtime.checked_output({'stdout': '{"version":"2.14.0","output":[58,64,139,154]}'},
                                   expected, 'cpp')

    def test_timeout_is_reported(self):
        run = runtime.run_timed([sys.executable, '-c', 'import time; time.sleep(2)'],
                                0.1, dict(__import__('os').environ))
        self.assertTrue(run['timeout'])
        self.assertNotEqual(run['exit_code'], 0)

    def test_insufficient_samples(self):
        self.assertEqual(runtime.interval([1, 2, 3])['status'], 'insufficient_samples')
        self.assertIsNone(runtime.interval(list(range(5)))['p95_ns'])
        self.assertEqual(runtime.interval(list(range(20)))['p95_ns'], 18)
        self.assertEqual(runtime.interval(list(range(21)))['p95_ns'], 19)

    def test_warm_shapes_and_output_provenance(self):
        self.assertEqual(runtime.COLD_CASES, ((2, 3, 2), (16, 16, 16)))
        self.assertEqual(runtime.WARM_CASES, ((128, 128, 128), (512, 512, 512)))
        for m, k, n in runtime.WARM_CASES:
            self.assertLessEqual(max(m * k, k * n, m * n), runtime.MAX_MATRIX_ELEMENTS)
        expected = runtime.reference(2, 3, 2)
        canonical, raw, _ = runtime.checked_output(
            {'stdout': '[[58,64],[139,154]]'}, expected, 'source')
        other, other_raw, _ = runtime.checked_output(
            {'stdout': '{"version":"2.11.0","output":[58.0,64.0,139.0,154.0]}'},
            expected, 'cpp')
        self.assertEqual(canonical, other)
        self.assertNotEqual(raw, other_raw)
        with self.assertRaises(runtime.Invalid):
            runtime.checked_output({'stdout': '[NaN,64,139,154]'}, expected, 'source')

    def test_invalid_warm_samples_and_route_hashes_fail_closed(self):
        sample = {'wall_ns': 10, 'cpu_ns': 10, 'matmul_wall_ns': 11,
                  'peak_rss_kib': 100}
        with self.assertRaises(runtime.Invalid):
            runtime.checked_samples({'samples': [sample]}, 1, 'facade', (128, 128, 128))
        report = {'exclusions': []}
        case = {'shape': [128, 128, 128]}
        with self.assertRaises(runtime.Invalid):
            runtime.same_outputs(report, case, {'cpp': 'a', 'facade': 'b'}, 'validation')
        self.assertEqual(report['exclusions'][0]['reason'], 'route_hash_mismatch')

    def test_timeout_run_is_recorded_as_exclusion(self):
        run = runtime.run_timed([sys.executable, '-c', 'import time; time.sleep(2)'],
                                0.1, dict(os.environ))
        report = {'runs': [], 'exclusions': []}
        with self.assertRaises(runtime.Invalid):
            runtime.record_run(report, run, (128, 128, 128), 'facade', 'warm')
        self.assertEqual(report['exclusions'][0]['reason'], 'timeout')
        self.assertTrue(report['runs'][0]['timeout'])


if __name__ == '__main__':
    unittest.main()
