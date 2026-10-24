"""Additional explicit prerequisite/sentinel scenarios; only disposable fixtures mutate."""
import json
from pathlib import Path
import sys
import tempfile
import unittest

LANE=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(LANE))
from common import Journal
from validation import matrix_inventory, sentinel_status


class ExplicitScenarios(unittest.TestCase):
    def test_failed_build_with_absent_execution_stays_incomplete(self):
        with tempfile.TemporaryDirectory() as root:
            j=Journal(Path(root)/'attempt')
            j.required_commands,j.required_checks=matrix_inventory(2)
            j.report['commands']=[dict(name=n,reason='exit',exit=0,signal=None) for n in sorted(j.required_commands)]
            j.report['checks']=[dict(name=n,passed=True) for n in sorted(j.required_checks)]
            next(r for r in j.report['commands'] if r['name']=='normal-debug-build')['exit']=1
            next(r for r in j.report['checks'] if r['name']=='normal-debug-build')['passed']=False
            j.report['commands']=[r for r in j.report['commands'] if r['name']!='normal-debug-run']
            absent={'normal-debug-result','normal-debug-balanced-roots','normal-debug-manual-zero-collections','normal-debug-deterministic-parity'}
            j.report['checks']=[r for r in j.report['checks'] if r['name'] not in absent]
            self.assertEqual(j.finish(),1)
            self.assertFalse(j.report['complete'])
            saved=json.loads((j.output/'report.json').read_text())
            self.assertTrue(any('normal-debug-run' in e for e in saved['coverage_errors']))
            self.assertEqual(next(r for r in saved['commands'] if r['name']=='normal-debug-build')['exit'],1)
            self.assertFalse(next(r for r in saved['checks'] if r['name']=='normal-debug-build')['passed'])

    def test_replaced_sentinel_fails_with_correct_diagnostic(self):
        with tempfile.TemporaryDirectory() as root:
            path=Path(root)/'sentinel'
            path.write_bytes(b'prior artifact\n')
            replacement=Path(root)/'replacement'
            replacement.write_bytes(b'newly emitted artifact\n')
            replacement.replace(path)
            row=dict(reason='exit',exit=1,signal=None,stderr='CGT095: expected rejection')
            self.assertEqual(sentinel_status(row,path,b'prior artifact\n','CGT095'),(True,False))


if __name__=='__main__':
    unittest.main()
