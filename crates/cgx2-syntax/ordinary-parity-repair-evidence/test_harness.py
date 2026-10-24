#!/usr/bin/env python3
"""Evidence must reject supervision failures and survive partial runs."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import common
from common import Journal, normal, observed, rejection, stable
from run import roots_ok, primary, byte_span


class EvidenceTests(unittest.TestCase):
    def test_non_exit_never_satisfies_expected_error(self):
        for reason, exit, signal in [('timeout',1,None),('signal',-9,9),('overflow',1,None),('pending',1,None),('exception',None,None)]:
            self.assertFalse(normal({'reason':reason,'exit':exit,'signal':signal},1))

    def test_wrong_exit_never_satisfies_error(self):
        self.assertFalse(normal({'reason':'exit','exit':0,'signal':None},1))

    def test_exact_code_span_file(self):
        row={'reason':'exit','exit':1,'signal':None,
             'stdout':json.dumps({'diagnostics':[{'code':'CGT095','span':{'start':3,'end':5},'file':'lib.cg'}]})}
        self.assertTrue(rejection(row,'CGT095',{'start':3,'end':5},'lib.cg'))
        self.assertFalse(rejection(row,'CGT090',{'start':3,'end':5},'lib.cg'))
        self.assertFalse(rejection(row,'CGT095',{'start':3,'end':6},'lib.cg'))
        self.assertFalse(rejection(row,'CGT095',{'start':3,'end':5},'main.cg'))

    def test_duplicate_diagnostics_do_not_pass(self):
        diag={'code':'CGT095','span':{'start':3,'end':5}}
        row={'reason':'exit','exit':1,'signal':None,'stdout':json.dumps([diag,diag])}
        self.assertFalse(rejection(row,'CGT095',diag['span']))

    def test_unrelated_error_never_matches(self):
        row={'reason':'exit','exit':1,'signal':None,'stdout':'not JSON','stderr':'CGR006: division by zero'}
        self.assertFalse(rejection(row,'CGT095',{'start':3,'end':5}))

    def test_pre_session_unavailable_is_distinct_from_zero(self):
        row={'stderr':'CGX2_MEMORY_EVIDENCE {"session_entered":false,"metrics":null}\n'}
        self.assertEqual(observed(row),{'session_entered':False,'metrics':None})
        self.assertFalse(roots_ok(observed(row)))
        self.assertNotEqual(observed(row),{'charged_bytes':0})

    def test_missing_and_duplicate_observations_are_unavailable(self):
        self.assertIsNone(observed({'stderr':''}))
        self.assertIsNone(observed({'stderr':'CGX2_MEMORY_EVIDENCE {}\nCGX2_MEMORY_EVIDENCE {}\n'}))
        self.assertIsNone(observed({'stderr':'CGX2_MEMORY_EVIDENCE {broken}\n'}))

    def test_om07_cannot_hide_behind_zero_final_census(self):
        value={'baseline_roots':0,'final_roots':0,'cleanup_failures':0,'roots_registered':25,'roots_released':26}
        self.assertFalse(roots_ok(value))

    def test_cleanup_failure_cannot_hide_behind_balanced_counts(self):
        self.assertFalse(roots_ok({'baseline_roots':0,'final_roots':0,'cleanup_failures':1,'roots_registered':2,'roots_released':2}))

    def test_stable_preserves_charges_and_failures(self):
        a={'pause':{'total_ns':1},'charged_bytes':42,'allocation_failures':1,'collection_count':2}
        self.assertEqual(stable(a),{'charged_bytes':42,'allocation_failures':1,'collection_count':2})
        self.assertNotEqual(stable(a),stable(dict(a,charged_bytes=41)))
        self.assertNotEqual(stable(a),stable(dict(a,collection_count=3)))

    def test_utf8_span_is_bytes(self):
        with tempfile.TemporaryDirectory(dir=common.ROOT/'target/tmp-ordinary-evidence-parity') as work:
            path=Path(work)/'utf8.cg';path.write_text('# café 🔧\nxs\n')
            self.assertEqual(byte_span(path,'xs'),{'start':13,'end':15})

    def test_stable_only_excludes_region_session_identity(self):
        self.assertEqual(stable({'region_id':{'session':1,'index':2}}),stable({'region_id':{'session':9,'index':2}}))
        self.assertNotEqual(stable({'region_id':{'session':1,'index':2}}),stable({'region_id':{'session':1,'index':3}}))
        self.assertEqual(stable({'session_id':7}),{'session_id':7})

    def test_mixed_evidence_and_error_json_lines(self):
        self.assertEqual(primary({'stderr':'CGX2_MEMORY_EVIDENCE {}\n{"code":"CGR015","span":{"start":13,"end":18}}\n'}),
                         ('CGR015',{'start':13,'end':18}))

    def test_emitted_primary_preserves_span(self):
        self.assertEqual(primary({'stderr':'CGR006: division by zero 13..18\n'}),('CGR006',{'start':13,'end':18}))

    def test_probe_primary_preserves_span(self):
        self.assertEqual(primary({'stderr':'PRIMARY CGR090 Some(Span { start: 13, end: 18 }) "allocation"\nCLEANUP []\n'}),
                         ('CGR090',{'start':13,'end':18}))

    def test_duplicate_primary_does_not_pass(self):
        self.assertIsNone(primary({'stderr':'CGR006: division by zero 13..18\nCGR006: division by zero 13..18\n'}))

    def test_native_primary_preserves_span(self):
        self.assertEqual(primary({'stderr':'CGR006: /x/utf8.cg:13..18: division by zero\n'}),
                         ('CGR006',{'start':13,'end':18}))

    def test_json_primary_preserves_span(self):
        self.assertEqual(primary({'stdout':'[{"code":"CGR015","span":{"start":13,"end":18}}]'}),
                         ('CGR015',{'start':13,'end':18}))

    def test_partial_journal_survives_spawn_quota_exception(self):
        with tempfile.TemporaryDirectory(dir=common.ROOT/'target/tmp-ordinary-evidence-parity') as work:
            out=Path(work)/'attempt'
            j=Journal(out)
            j.check('prior-proof',True)
            with patch.object(common,'supervise',side_effect=OSError('standalone quota sentinel')):
                with self.assertRaises(OSError):
                    j.run('standalone-build',['rustc','program.rs'])
            saved=json.loads((out/'report.json').read_text())
            self.assertEqual(saved['checks'][0],{'name':'prior-proof','passed':True})
            self.assertEqual(saved['commands'][0]['reason'],'exception')
            self.assertIn('standalone quota sentinel',saved['commands'][0]['traceback'])
            self.assertFalse(saved['complete'])

    def test_command_is_persisted_before_spawn(self):
        with tempfile.TemporaryDirectory(dir=common.ROOT/'target/tmp-ordinary-evidence-parity') as work:
            out=Path(work)/'attempt';j=Journal(out)
            def crash(*args,**kwargs):
                saved=json.loads((out/'report.json').read_text())
                self.assertEqual(saved['commands'][0]['reason'],'pending')
                raise RuntimeError('dispatch sentinel')
            with patch.object(common,'supervise',side_effect=crash):
                with self.assertRaises(RuntimeError):j.run('dispatch',['fixture'])

    def test_attempt_directory_cannot_overwrite_prior_evidence(self):
        with tempfile.TemporaryDirectory(dir=common.ROOT/'target/tmp-ordinary-evidence-parity') as work:
            out=Path(work)/'attempt';Journal(out)
            with self.assertRaises(FileExistsError):Journal(out)


if __name__=='__main__':
    unittest.main()
