"""Local preparer tests only; no real dateutil import/install or acceptance claim."""
import hashlib
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import prepare_dateutil_lock as prepare


class PreparerTests(unittest.TestCase):
    def test_stdlib_snapshot_dereferences_files_and_omits_caches(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'installed'
            source.mkdir()
            (source / 'module.py').write_bytes(b'abc')
            (source / 'alias.py').symlink_to(source / 'module.py')
            (source / '__pycache__').mkdir()
            (source / '__pycache__' / 'module.pyc').write_bytes(b'ignored')
            snapshot = prepare.copy_stdlib(source, root / 'snapshot')
            self.assertFalse((snapshot / 'alias.py').is_symlink())
            self.assertFalse((snapshot / '__pycache__').exists())
            expected = hashlib.sha256()
            for name in ['alias.py', 'module.py']:
                expected.update((name + '\0' + hashlib.sha256(b'abc').hexdigest() + '\n').encode())
            self.assertEqual(prepare.tree_hash(snapshot), expected.hexdigest())
            with self.assertRaises(ValueError):
                prepare.copy_stdlib(source, snapshot)
            (snapshot / 'unlocked.pyc').write_bytes(b'bad')
            with self.assertRaises(ValueError):
                prepare.tree_hash(snapshot)

    def test_distribution_inventory_and_exact_version_are_local_only(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'demo').mkdir()
            (root / 'demo' / '__init__.py').write_text("raise AssertionError('must not import')\n")
            (root / 'demo' / 'value.py').write_text('value = 1\n')
            (root / 'demo-1.dist-info').mkdir()
            (root / 'demo-1.dist-info' / 'METADATA').write_text('Name: demo\nVersion: 1\n')

            class LocalDistribution:
                version = '1'
                files = [Path('demo/__init__.py'), Path('demo/value.py'),
                         Path('demo-1.dist-info/METADATA')]

                def locate_file(self, entry):
                    return root / entry

            with patch.object(prepare.metadata, 'distribution', return_value=LocalDistribution()):
                result = prepare.distribution('demo', '1', [])
                self.assertEqual([s['module'] for s in result['sources']], ['demo', 'demo.value'])
                self.assertTrue(result['sources'][0]['is_package'])
                with self.assertRaises(ValueError):
                    prepare.distribution('demo', 'wrong', [])


if __name__ == '__main__':
    unittest.main()
