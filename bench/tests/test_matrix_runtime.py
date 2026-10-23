import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
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
            runtime.same_outputs(report, case, {'cpp': [1.], 'facade': [2.]}, 'validation')
        self.assertEqual(report['exclusions'][0]['reason'], 'route_tolerance_mismatch')

    def test_f32_rounding_source_and_caps(self):
        a, b = runtime.inputs(16, 16, 16, 'f32')
        self.assertNotEqual(b[0], runtime.inputs(16, 16, 16)[1][0])
        self.assertEqual(a[0], -3 / 8)
        self.assertIn('torch_cpu.cgx2_torch_matmul_f32', runtime.source(16, 16, 16, 'f32'))
        self.assertIn(repr(b[0]), runtime.source(16, 16, 16, 'f32'))
        runtime.source(32, 32, 32, 'f32')
        with self.assertRaises(runtime.Invalid):
            runtime.source(128, 128, 128, 'f32')

    def test_fixed_f32_tolerance_shape_dtype_and_nonfinite(self):
        expected = [1., 0.]
        def output(values, **changes):
            payload = dict(version=runtime.VERSION, dtype='f32', shape=[1, 2], output=values)
            payload.update(changes)
            return {'stdout': json.dumps(payload)}
        # Hashes can differ despite numerical agreement, including across a rounding boundary.
        first = runtime.checked_output(output([1., 0.]), expected, 'cpp', 'f32', (1, 2, 2))
        second = runtime.checked_output(output([1.000009, 0.000009]), expected,
                                        'facade', 'f32', (1, 2, 2))
        self.assertNotEqual(first[0], second[0])
        self.assertNotEqual(first[1], second[1])
        runtime.same_outputs({'exclusions': []}, {'shape': [1, 2, 2]},
                             {'cpp': [1., 0.], 'facade': [1.000009, 0.000009]},
                             'validation', 'f32')
        for run in (output([1.00003, 0.]), output([1., 0.000011]),
                    output([float('nan'), 0.]), output([float('inf'), 0.]),
                    output([1., 0.], dtype='f64'), output([1., 0.], shape=[2, 1]),
                    {'stdout': '[[1],[0]]'}, output([1.])):
            with self.subTest(run=run), self.assertRaises(runtime.Invalid):
                runtime.checked_output(run, expected, 'cpp', 'f32', (1, 2, 2))
        # Both routes can pass the scalar reference yet disagree with each other.
        with self.assertRaises(runtime.Invalid):
            runtime.same_outputs({'exclusions': []}, {'shape': [1, 1, 1]},
                                 {'cpp': [0.000009], 'facade': [-0.000009]},
                                 'validation', 'f32')

    def test_external_profile_never_inferred_from_filename(self):
        with tempfile.TemporaryDirectory() as tmp:
            binary = Path(tmp) / 'release/cgx2'
            binary.parent.mkdir()
            binary.write_bytes(b'not proof of optimization')
            unknown = runtime.artifact_provenance(binary, [])
            self.assertEqual(unknown['profile_status'], 'unverified-profile')
            self.assertIsNone(unknown['cargo_profile'])
            profile = {'opt_level': '3', 'debug_assertions': False}
            verified = runtime.artifact_provenance(binary, [
                {'executable': str(binary), 'profile': profile}])
            self.assertEqual(verified['cargo_profile'], profile)
            self.assertEqual(verified['sha256'], unknown['sha256'])

    def test_package_dtype_layout_mismatch(self):
        with tempfile.TemporaryDirectory() as tmp:
            package = Path(tmp)
            (package / 'cgx2.toml').write_text('')
            facade = package / 'libcgx2_libtorch_facade.so'
            facade.write_bytes(b'fixture only')
            (package / 'cgx2-libtorch-closure.lock.json').write_text('{}')
            (package / 'cgx2-libtorch-evidence.json').write_text(json.dumps({
                'torch_version': runtime.VERSION, 'facade_sha256': runtime.digest(facade)}))
            lock = package / 'cgx2-bind.lock.json'
            lock.write_text(json.dumps({'bindings': [{'id': 'torch_cpu',
                'version': runtime.VERSION, 'layout': 'tensor-f64-cpu-matrix-v2'}]}))
            runtime.package_checks(package)
            with self.assertRaisesRegex(runtime.Invalid, 'tensor-f32-cpu-matrix-v3'):
                runtime.package_checks(package, 'f32')

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
