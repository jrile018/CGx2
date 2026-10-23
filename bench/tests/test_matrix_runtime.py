import importlib.util
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


if __name__ == '__main__':
    unittest.main()
