import copy
import gzip
import json
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import native_worker_baselines as b


class NativeWorkerTests(unittest.TestCase):
    def setUp(self):
        (b.ROOT / 'bench/target').mkdir(exist_ok=True)
        self.tmp = tempfile.TemporaryDirectory(dir=b.ROOT / 'bench/target')
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)

    def execute(self, code, timeout=2):
        return b.run([sys.executable, '-I', '-S', '-c', code], dict(os.environ), self.root, timeout)

    def test_wrong_output_and_exact_diagnostics_gate(self):
        row = self.execute('print(8)')
        self.assertFalse(b.assess(row, '7\n'))
        self.assertTrue(b.assess(row, '8\n'))
        self.assertFalse(b.assess(row, '8'))
        row = self.execute("import sys; sys.stderr.write('CGR020'); sys.exit(1)")
        self.assertTrue(b.assess(row, '', 1, 'CGR020'))
        self.assertFalse(b.assess(row, '', 1, 'CGR021'))
        row['stdout'] = 'unexpected'
        self.assertFalse(b.assess(row, '', 1, 'CGR020'))

    def test_missing_binary_and_interpreter_are_retained(self):
        for name in ('missing-cgx2', 'missing-python'):
            row = b.run([self.root / name], dict(os.environ), self.root)
            self.assertEqual(row['status'], 'spawn-error')
            self.assertFalse(b.assess(row))
            self.assertIn(name, row['command'][0])
            self.assertIsNone(row['peak_rss_kib'])

    def test_timeout_retains_partial_output_and_kills_group(self):
        pidfile = self.root / 'child'
        row = self.execute("import subprocess,sys,time; "
                           "p=subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)']); "
                           f"open({str(pidfile)!r},'w').write(str(p.pid)); "
                           "print('started',flush=True); time.sleep(30)", .1)
        self.assertEqual(row['status'], 'timeout')
        self.assertEqual(row['stdout'], 'started\n')
        self.assertFalse(b.assess(row, 'started\n'))
        # SIGKILL delivery is asynchronous: communicate() reaps the direct child,
        # but a descendant can briefly remain running before becoming a zombie.
        # Require cleanup within a fixed deadline; never accept a surviving child.
        proc = Path('/proc') / pidfile.read_text() / 'stat'
        deadline = time.monotonic() + 2
        while True:
            try:
                state = proc.read_text().split()[2]
            except FileNotFoundError:
                break
            if state == 'Z':
                break
            remaining = deadline - time.monotonic()
            self.assertGreater(remaining, 0, f'descendant survived SIGKILL in state {state}')
            time.sleep(min(.01, remaining))

    def test_nonzero_exit_and_rusage(self):
        row = self.execute('raise SystemExit(7)')
        self.assertEqual(row['exit_status'], 7)
        self.assertFalse(b.assess(row))
        self.assertIsNotNone(row['user_cpu_s'])
        self.assertGreater(row['peak_rss_kib'], 0)
        self.assertIn('7', row['usage_raw'])

    def test_external_binary_profile_and_tampered_provenance(self):
        binary = self.root / 'binary'
        binary.write_text('placeholder')
        self.assertEqual(b.provenance(binary)['profile'], 'unverified-profile')
        manifest = self.root / 'manifest.json'
        prefix = ['cargo', '+1.98.1', 'build', '--offline', '--locked', '--release',
                  '-p', 'cgx2c', '-p', 'cgx2-cli']
        value = dict(base=b.BASE, build_prefix=prefix, rustc_version='pinned',
                     cargo_lock_sha256=b.digest(b.ROOT / 'Cargo.lock'),
                     binaries={'cgx2': b.artifact(binary)})
        manifest.write_text(json.dumps(value))
        with patch.object(b, 'source_state'), patch.object(b, 'capture', return_value='pinned'):
            self.assertEqual(b.provenance(binary, manifest, 'cgx2')['profile'], 'verified-release-build-record')
            for key, changed in [('base', 'wrong'), ('build_prefix', prefix + ['--features=python']),
                                 ('rustc_version', 'other'), ('cargo_lock_sha256', 'wrong')]:
                bad = dict(value, **{key: changed})
                manifest.write_text(json.dumps(bad))
                with self.assertRaisesRegex(ValueError, 'provenance'):
                    b.provenance(binary, manifest, 'cgx2')
            manifest.write_text(json.dumps(value))
            binary.write_text('replaced')
            with self.assertRaisesRegex(ValueError, 'provenance'):
                b.provenance(binary, manifest, 'cgx2')

    def test_sample_gates(self):
        self.assertEqual(b.distribution([]), {'n': 0})
        self.assertIsNone(b.distribution(list(range(19)))['p95'])
        self.assertEqual(b.distribution(list(range(20)))['p95'], 18)
        rows = self.rows(29)
        self.assertNotIn('bootstrap_95', b.summarize(rows)['scalar_aot']['paired_wall_ratio'])
        rows += self.rows(1, start=29)
        interval = b.summarize(rows)['scalar_aot']['paired_wall_ratio']['bootstrap_95']
        self.assertEqual(interval, [2, 2])

    def rows(self, count, start=0):
        return [{'case': 'scalar_aot', 'pair': i, 'warmup': False,
                 'runs': {route: dict(valid=True, wall_s=wall, user_cpu_s=0.,
                                      system_cpu_s=0., peak_rss_kib=2)
                          for route, wall in [('cgx2', 2.), ('baseline', 1.)]}}
                for i in range(start, start + count)]

    def test_randomized_pair_accounting_and_inputs(self):
        planned = b.schedule()
        self.assertEqual(planned, b.schedule())
        for case in ('scalar_aot', 'python_child'):
            rows = [r for r in planned if r['case'] == case and not r['warmup']]
            self.assertEqual(len(rows), 30)
            self.assertEqual([r['pair'] for r in rows], list(range(30)))
            self.assertEqual({tuple(r['order']) for r in rows}, {('cgx2', 'baseline'), ('baseline', 'cgx2')})
            if case == 'scalar_aot':
                self.assertEqual([sum(len(r['args']) == n for r in rows) for n in range(3)], [10, 10, 10])

    def test_failed_pairs_and_warmups_are_never_replaced(self):
        rows = self.rows(30)
        rows[3]['runs']['baseline']['valid'] = False
        warmup = copy.deepcopy(rows[0])
        warmup['warmup'] = True
        rows.append(warmup)
        original = copy.deepcopy(rows)
        summary = b.summarize(rows)['scalar_aot']
        self.assertEqual(summary['valid_pairs'], 29)
        self.assertEqual(summary['attempted_pairs'], 30)
        self.assertEqual(summary['excluded_pair_ids'], [3])
        self.assertEqual(summary['cgx2']['wall_s']['n'], 30)
        self.assertEqual(summary['baseline']['wall_s']['n'], 29)
        self.assertEqual(rows, original)

    def test_disassembly_archive_is_lossless_and_deterministic(self):
        assembly = {'binary': '00 <main>:\n  01: jmp 00\n'}
        first = self.root / 'a.gz'
        second = self.root / 'b.gz'
        record = b.archive_assembly(assembly, first)
        b.archive_assembly(assembly, second)
        self.assertEqual(json.loads(gzip.decompress(first.read_bytes())), assembly)
        self.assertEqual(first.read_bytes(), second.read_bytes())
        self.assertEqual(record['sha256'], b.digest(first))
        with self.assertRaises(FileExistsError):
            b.archive_assembly({'replaced': True}, first)

    def test_existing_evidence_cannot_be_overwritten(self):
        out = self.root / 'failed.json'
        out.write_text('{"valid":false}')
        with self.assertRaises(SystemExit), patch('sys.stderr'):
            b.main(['--out', str(out)])
        self.assertEqual(out.read_text(), '{"valid":false}')


if __name__ == '__main__':
    unittest.main()
