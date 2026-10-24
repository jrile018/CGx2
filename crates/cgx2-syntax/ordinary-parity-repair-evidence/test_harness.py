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
import integrity
from validation import (ROOT_FIELDS, direct_inventory, validate_direct, session_unavailable,
                        matrix_inventory, auto_inventory, package_inventory, sentinel_status)


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
        with tempfile.TemporaryDirectory(dir=common.os.environ.get('TMPDIR', common.ROOT/'target/tmp-ordinary-harness-hardening')) as work:
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
        with tempfile.TemporaryDirectory(dir=common.os.environ.get('TMPDIR', common.ROOT/'target/tmp-ordinary-harness-hardening')) as work:
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
        with tempfile.TemporaryDirectory(dir=common.os.environ.get('TMPDIR', common.ROOT/'target/tmp-ordinary-harness-hardening')) as work:
            out=Path(work)/'attempt';j=Journal(out)
            def crash(*args,**kwargs):
                saved=json.loads((out/'report.json').read_text())
                self.assertEqual(saved['commands'][0]['reason'],'pending')
                raise RuntimeError('dispatch sentinel')
            with patch.object(common,'supervise',side_effect=crash):
                with self.assertRaises(RuntimeError):j.run('dispatch',['fixture'])

    def test_attempt_directory_cannot_overwrite_prior_evidence(self):
        with tempfile.TemporaryDirectory(dir=common.os.environ.get('TMPDIR', common.ROOT/'target/tmp-ordinary-harness-hardening')) as work:
            out=Path(work)/'attempt';Journal(out)
            with self.assertRaises(FileExistsError):Journal(out)


class HardeningTests(unittest.TestCase):
    def roots(self):
        return dict(baseline_roots=0, final_roots=0, roots_registered=3, roots_released=3, cleanup_failures=0)

    def test_all_missing_root_fields(self):
        for key in ROOT_FIELDS:
            with self.subTest(key=key):
                value=self.roots();del value[key]
                self.assertFalse(roots_ok(value))

    def test_all_malformed_root_fields(self):
        for key in ROOT_FIELDS:
            for invalid in (None, False, True, '0', 0.0, -1):
                with self.subTest(key=key, invalid=invalid):
                    value=self.roots();value[key]=invalid
                    self.assertFalse(roots_ok(value))

    def test_valid_integer_root_control_and_census(self):
        self.assertTrue(roots_ok(self.roots()))
        self.assertFalse(roots_ok(dict(self.roots(),roots_released=4)))
        self.assertFalse(roots_ok(dict(self.roots(),cleanup_failures=1)))
        self.assertFalse(roots_ok(dict(self.roots(),baseline_roots=1,final_roots=1)))

    def test_session_unavailable_requires_exact_false_null(self):
        self.assertTrue(session_unavailable({'session_entered':False,'metrics':None}))
        for value in (None, {}, {'session_entered':0,'metrics':None},
                      {'session_entered':True,'metrics':None}, {'session_entered':False},
                      {'session_entered':False,'metrics':{}}, dict(self.roots(),session_entered=False,metrics=None)):
            self.assertFalse(session_unavailable(value))

    def direct_rows(self):
        rows=[dict(name=n,passed=True,details='fixture') for n in sorted(direct_inventory(2))]
        for row in rows:
            if row['name']=='sweep-bound-at-most-64':row['details']='2'
            if row['name']=='reserve-18446744073709551615':row['details']='(2, None, Ok(()), fixture)'
        return rows

    def direct(self, rows):
        return validate_direct('\n'.join(json.dumps(r) for r in rows))

    def test_direct_complete_control(self):
        rows, errors, bound=self.direct(self.direct_rows())
        self.assertFalse(errors);self.assertEqual(bound,2)
        self.assertEqual({r['name'] for r in rows},direct_inventory(2))

    def test_direct_empty_missing_malformed(self):
        for text in ('', '\n', '{broken}', '{}', 'null', '[]'):
            self.assertTrue(validate_direct(text)[1])
        for index in range(len(self.direct_rows())):
            rows=self.direct_rows();rows.pop(index)
            self.assertTrue(self.direct(rows)[1])

    def test_direct_duplicate_and_substituted_names(self):
        rows=self.direct_rows();rows.append(dict(rows[0]))
        self.assertTrue(self.direct(rows)[1])
        rows=self.direct_rows();rows[0]['name']='substituted'
        self.assertTrue(self.direct(rows)[1])

    def test_direct_strict_boolean_and_structure(self):
        for key, invalid in [('passed',1),('passed',None),('passed','true'),('details',{}),('name',1)]:
            rows=self.direct_rows();rows[0][key]=invalid
            self.assertTrue(self.direct(rows)[1])
        rows=self.direct_rows();del rows[0]['details']
        self.assertTrue(self.direct(rows)[1])

    def test_direct_missing_reserve_and_unaffected_control(self):
        for name in ('reserve-1','reserve-2','reserve-3'):
            rows=[r for r in self.direct_rows() if r['name']!=name]
            self.assertTrue(self.direct(rows)[1])

    def test_direct_bound_is_baseline_derived_and_bounded(self):
        for value in ('0','65','true','2.0','-1','3'):
            rows=self.direct_rows()
            next(r for r in rows if r['name']=='sweep-bound-at-most-64')['details']=value
            self.assertTrue(self.direct(rows)[1])

    def journal(self, root, inventory):
        j=Journal(Path(root)/'attempt')
        j.required_commands, j.required_checks=inventory
        j.report['commands']=[dict(name=n,reason='exit',exit=0,signal=None) for n in sorted(j.required_commands)]
        j.report['checks']=[dict(name=n,passed=True) for n in sorted(j.required_checks)]
        return j

    def test_finalization_valid_matrix_package_auto(self):
        for inventory in (matrix_inventory(2),auto_inventory(),package_inventory()):
            with tempfile.TemporaryDirectory() as root:
                j=self.journal(root,inventory)
                self.assertEqual(j.finish(),0)
                self.assertTrue(j.report['complete'])

    def test_missing_each_required_target_is_incomplete(self):
        inventory=matrix_inventory(2)
        names=('normal-interpreter','normal-debug-run','normal-release-run','normal-standalone-emit',
               'normal-standalone-build','normal-standalone-run','source-reserve-17','source-reserve-64',
               'source-fault-3-debug-run','source-fault-3-release-run','source-fault-3-standalone-run')
        for name in names:
            with self.subTest(name=name),tempfile.TemporaryDirectory() as root:
                j=self.journal(root,inventory)
                j.report['commands']=[r for r in j.report['commands'] if r['name']!=name]
                self.assertEqual(j.finish(),1);self.assertFalse(j.report['complete'])

    def test_missing_cleanup_result_parity_rows_is_incomplete(self):
        for name in ('normal-interpreter-result','normal-debug-balanced-roots',
                     'normal-release-deterministic-parity','normal-standalone-result'):
            with tempfile.TemporaryDirectory() as root:
                j=self.journal(root,matrix_inventory(2))
                j.report['checks']=[r for r in j.report['checks'] if r['name']!=name]
                self.assertEqual(j.finish(),1);self.assertFalse(j.report['complete'])

    def test_each_duplicate_target_is_incomplete(self):
        for name in ('normal-interpreter','normal-debug-run','normal-release-run','normal-standalone-emit',
                     'normal-standalone-build','normal-standalone-run','source-reserve-64'):
            with self.subTest(name=name),tempfile.TemporaryDirectory() as root:
                j=self.journal(root,matrix_inventory(2))
                j.report['commands'].append(dict(next(r for r in j.report['commands'] if r['name']==name)))
                self.assertEqual(j.finish(),1);self.assertFalse(j.report['complete'])

    def test_package_and_auto_cannot_disappear(self):
        for inventory in (auto_inventory(),package_inventory()):
            for kind in ('commands','checks'):
                with tempfile.TemporaryDirectory() as root:
                    j=self.journal(root,inventory);j.report[kind]=[]
                    self.assertEqual(j.finish(),1);self.assertFalse(j.report['complete'])

    def test_duplicate_commands_and_checks_rejected_at_finalization(self):
        for kind in ('commands','checks'):
            with tempfile.TemporaryDirectory() as root:
                j=self.journal(root,auto_inventory());j.report[kind].append(dict(j.report[kind][0]))
                self.assertEqual(j.finish(),1);self.assertFalse(j.report['complete'])

    def test_duplicate_dispatch_and_check_fail_before_overwrite(self):
        with tempfile.TemporaryDirectory() as root:
            j=Journal(Path(root)/'attempt');j.check('one',True)
            with self.assertRaises(ValueError):j.check('one',False)
            j.report['commands'].append(dict(name='one',reason='exit'))
            with self.assertRaises(ValueError):j.run('one',['fixture'])
            self.assertEqual(len(j.report['commands']),1)

    def test_product_failure_remains_red_but_coverage_complete(self):
        with tempfile.TemporaryDirectory() as root:
            j=self.journal(root,auto_inventory());j.report['checks'][0]['passed']=False
            self.assertEqual(j.finish(),1);self.assertTrue(j.report['complete'])
            self.assertEqual(j.report['failed'],1)

    def test_malformed_report_boolean_is_durable_and_incomplete(self):
        for invalid in (None, 1, 'true', {}):
            with tempfile.TemporaryDirectory() as root:
                j=self.journal(root,auto_inventory());j.report['checks'][0]['passed']=invalid
                self.assertEqual(j.finish(),1);self.assertFalse(j.report['complete'])
                saved=json.loads((j.output/'report.json').read_text())
                self.assertTrue(saved['coverage_errors'])

    def test_skip_and_invalid_sweep_bounds_are_nonaccepting(self):
        for bound in (None,0,64,True):
            with tempfile.TemporaryDirectory() as root:
                j=self.journal(root,matrix_inventory(bound))
                j.report['checks']=[r for r in j.report['checks'] if r['name']!='valid-source-reserve-bound']
                self.assertEqual(j.finish(),1);self.assertFalse(j.report['complete'])
        with tempfile.TemporaryDirectory() as root:
            j=self.journal(root,matrix_inventory(2));j.report['incomplete_reason']='required compiled sweep skipped'
            self.assertEqual(j.finish(),1);self.assertFalse(j.report['complete'])

    def frozen_fixture(self, root):
        root=Path(root);binary=root/'frozen';(binary/'deps').mkdir(parents=True)
        (binary/'cgx2').write_bytes(b'consumer');(binary/'libcgx2_runtime.rlib').write_bytes(b'canonical')
        (binary/'deps/dependency.rlib').write_bytes(b'dependency')
        source=root/'compiler.rs';source.write_bytes(b'compiler')
        tool=root/'rustc';tool.write_bytes(b'toolchain')
        return binary,source,tool

    def test_frozen_control_and_mutations(self):
        for relative in ('cgx2','libcgx2_runtime.rlib','deps/dependency.rlib'):
            for action in ('mutate','delete','replace'):
                with self.subTest(relative=relative,action=action),tempfile.TemporaryDirectory() as root:
                    binary,source,tool=self.frozen_fixture(root)
                    with patch.object(integrity,'compiler_paths',return_value={source}),patch.object(integrity,'harness_paths',return_value=set()):
                        expected=integrity.freeze_inputs(binary,{tool})
                        self.assertFalse(integrity.verify_inputs(binary,expected)[1])
                        path=binary/relative
                        if action=='mutate':path.write_bytes(b'changed')
                        else:
                            path.unlink()
                            if action=='replace':path.write_bytes(b'replacement')
                        self.assertTrue(integrity.verify_inputs(binary,expected)[1])

    def test_added_frozen_artifact_dependency_and_source(self):
        for relative in ('extra','deps/extra'):
            with tempfile.TemporaryDirectory() as root:
                binary,source,tool=self.frozen_fixture(root)
                with patch.object(integrity,'compiler_paths',return_value={source}),patch.object(integrity,'harness_paths',return_value=set()):
                    expected=integrity.freeze_inputs(binary,{tool});(binary/relative).write_bytes(b'new')
                    self.assertTrue(integrity.verify_inputs(binary,expected)[1])
        with tempfile.TemporaryDirectory() as root:
            binary,source,tool=self.frozen_fixture(root)
            with patch.object(integrity,'compiler_paths',return_value={source}),patch.object(integrity,'harness_paths',return_value=set()):
                expected=integrity.freeze_inputs(binary,{tool});source.write_bytes(b'changed compiler')
                self.assertTrue(integrity.verify_inputs(binary,expected)[1])

    def test_same_byte_replacement_is_detected(self):
        for relative in ('cgx2','deps/dependency.rlib'):
            with tempfile.TemporaryDirectory() as root:
                binary,source,tool=self.frozen_fixture(root)
                with patch.object(integrity,'compiler_paths',return_value={source}),patch.object(integrity,'harness_paths',return_value=set()):
                    expected=integrity.freeze_inputs(binary,{tool});path=binary/relative
                    replacement=Path(root)/'replacement';replacement.write_bytes(path.read_bytes());replacement.replace(path)
                    self.assertTrue(integrity.verify_inputs(binary,expected)[1])

    def test_changed_symlink_target_and_target_bytes(self):
        for swap in (False,True):
            with tempfile.TemporaryDirectory() as root:
                binary,source,tool=self.frozen_fixture(root)
                link=binary/'deps/link.rlib';link.symlink_to(source)
                with patch.object(integrity,'compiler_paths',return_value=set()),patch.object(integrity,'harness_paths',return_value=set()):
                    expected=integrity.freeze_inputs(binary,{tool})
                    if swap:
                        other=Path(root)/'other';other.write_bytes(source.read_bytes());link.unlink();link.symlink_to(other)
                    else:source.write_bytes(b'changed target bytes')
                    self.assertTrue(integrity.verify_inputs(binary,expected)[1])

    def test_integrity_audit_failure_reaches_nonzero_finalization(self):
        with tempfile.TemporaryDirectory() as root:
            binary,source,tool=self.frozen_fixture(root)
            with patch.object(integrity,'compiler_paths',return_value={source}),patch.object(integrity,'harness_paths',return_value=set()):
                expected=integrity.freeze_inputs(binary,{tool})
                (Path(root)/'inputs.json').write_text(json.dumps(expected));(binary/'deps/new').write_bytes(b'new')
                j=Journal(Path(root)/'attempt');self.assertFalse(integrity.audit(j,binary,'after'))
                self.assertEqual(j.finish(),1);self.assertFalse(j.report['complete'])

    def test_sentinel_expected_rejection_and_exact_preservation(self):
        with tempfile.TemporaryDirectory() as root:
            path=Path(root)/'sentinel';path.write_bytes(b'prior artifact\n')
            row=dict(reason='exit',exit=1,signal=None,stderr='CGT095: expected rejection')
            self.assertEqual(sentinel_status(row,path,b'prior artifact\n','CGT095'),(True,True))
            for change in ({'stderr':'unrelated'}, {'exit':0}, {'reason':'timeout'}, {'signal':9}, {'exit':True}):
                self.assertEqual(sentinel_status(dict(row,**change),path,b'prior artifact\n','CGT095'),(False,True))
            path.write_bytes(b'changed')
            self.assertEqual(sentinel_status(row,path,b'prior artifact\n','CGT095'),(True,False))
            path.unlink()
            self.assertEqual(sentinel_status(row,path,b'prior artifact\n','CGT095'),(True,False))

    def test_missing_observation_never_satisfies_cleanup(self):
        for stderr in ('', 'CGX2_MEMORY_EVIDENCE null', 'CGX2_MEMORY_EVIDENCE []', 'CGX2_MEMORY_EVIDENCE {}'):
            self.assertFalse(roots_ok(observed({'stderr':stderr})))


if __name__=='__main__':
    unittest.main()
