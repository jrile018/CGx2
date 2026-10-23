"""Harness tests use synthetic executables; these are not CGx2 acceptance evidence."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('docs_check', Path(__file__).with_name('check.py'))
checker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checker)


class CheckerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.inputs = self.root / 'fixtures'
        self.inputs.mkdir()
        (self.inputs / 'demo.cg').write_text('42\n')
        self.work = self.root / 'work'
        self.work.mkdir()
        self.tool = self.root / 'compiler'
        self.tool.write_text('#!/usr/bin/env python3\nimport sys\nprint("help" if "--help" in sys.argv else "42")\n')
        self.tool.chmod(0o700)
        self.case = {'id':'demo', 'requirements':['R01'], 'scope':'source', 'disposition':'implemented',
                     'steps':[{'tool':'compiler','args':['demo.cg'],'exit':0,
                               'stdout':{'equals':'42\n'},'stderr':{'equals':''}}]}
        self.suite = {'schema_version':1, 'baseline':'0'*40, 'fixture_root':'fixtures','cases':[self.case]}
        self.manifest = self.root / 'suite.json'
        self.sequence = 0

    def invoke(self, *, pin=None, compiler=True, report=None, extra=()):
        self.manifest.write_text(json.dumps(self.suite))
        self.sequence += 1
        output = report or self.root / f'report-{self.sequence}.json'
        args = ['--suite',str(self.manifest),'--report',str(output),'--work-root',str(self.work)]
        if compiler:
            args += ['--compiler',str(self.tool),'--sha256', pin or checker.digest(self.tool)]
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            status = checker.main([*args,*extra])
        return status, json.loads(output.read_text()) if output.exists() and output.is_file() else None

    def test_positive_and_deterministic_report(self):
        status, one = self.invoke()
        status2, two = self.invoke()
        self.assertEqual((status,status2),(0,0))
        self.assertEqual(one,two)
        self.assertEqual(one['counts']['passed'],1)
        self.assertFalse(one['acceptance_complete'])
        self.assertEqual(one['results'][0]['steps'][0]['command'],['$TOOL:compiler','demo.cg'])

    def test_negative_diagnostics_and_utf8_span(self):
        self.tool.write_text('#!/usr/bin/env python3\nimport sys,json\nif "--help" in sys.argv: print("help"); sys.exit(0)\nprint(json.dumps(dict(code="CGT001",file="demo.cg",span=dict(start=2,end=4))),file=sys.stderr)\nsys.exit(1)\n')
        step = self.case['steps'][0]
        step.update(exit=1,stdout={'equals':''},stderr={'contains':['CGT001']},
                    diagnostics=[dict(code='CGT001',file='demo.cg',span=dict(start=2,end=4))])
        self.case['disposition']='expected-unsupported'
        self.assertEqual(self.invoke()[0],0)
        step['diagnostics'][0]['span']['end']=5
        self.assertEqual(self.invoke()[0],1)

    def test_wrong_exit_stdout_and_stderr_fail(self):
        for stream in ('stdout','stderr'):
            with self.subTest(stream=stream):
                self.case['steps'][0][stream]={'equals':'wrong'}
                self.assertEqual(self.invoke()[0],1)
                self.case['steps'][0][stream]={'equals':'42\n' if stream=='stdout' else ''}
        self.case['steps'][0]['exit']=1
        self.assertEqual(self.invoke()[0],1)

    def test_missing_compiler_blocked_and_never_passed(self):
        status, report=self.invoke(compiler=False)
        self.assertEqual(status,2)
        self.assertEqual(report['counts']['blocked'],1)
        self.assertEqual(report['counts']['passed'],0)

    def test_missing_executable_blocked(self):
        self.tool.unlink()
        status, report=self.invoke(pin='0'*64)
        self.assertEqual(status,2)
        self.assertEqual(report['tools']['compiler']['status'],'blocked')

    def test_tampered_compiler_not_executed(self):
        marker=self.root/'executed'
        pin=checker.digest(self.tool)
        self.tool.write_text('#!/usr/bin/env python3\nfrom pathlib import Path\nPath('+repr(str(marker))+').touch()\n')
        status, report=self.invoke(pin=pin)
        self.assertEqual(status,1)
        self.assertEqual(report['tools']['compiler']['reason'],'identity_mismatch')
        self.assertFalse(marker.exists())

    def test_absent_named_prerequisite_blocked(self):
        self.case['steps'][0]['args'].append('{tool:rustc}')
        self.assertEqual(self.invoke()[0],2)

    def test_future_and_external_never_pass(self):
        self.suite['cases']=[dict(self.case,id='reserved',disposition='future',steps=[],reason='S2',handoff='owner'),
                             dict(self.case,id='runtime',disposition='external',steps=[],reason='pins absent',handoff='owner')]
        status, report=self.invoke()
        self.assertEqual(status,2)
        self.assertEqual(report['counts'],dict(passed=0,failed=0,blocked=1,future=1))
        self.assertEqual(self.invoke(extra=['--runnable-only'])[0],0)

    def test_malformed_expectations(self):
        mutations=[lambda s:s.update(schema_version=2),lambda s:s.update(extra=True),
                   lambda s:s.update(cases=[]),lambda s:s.update(fixture_root='../fixtures'),
                   lambda s:s['cases'].append(s['cases'][0]),
                   lambda s:s['cases'][0]['steps'][0].update(exit=True),
                   lambda s:s['cases'][0]['steps'][0].update(stdout={'contains':[]}),
                   lambda s:s['cases'][0]['steps'][0].update(stdout={'equals':'','extra':''}),
                   lambda s:s['cases'][0]['steps'][0].update(args=['{unknown}']),
                   lambda s:s['cases'][0].update(disposition='expected-unsupported'),
                   lambda s:s['cases'][0].update(disposition='future')]
        original=json.dumps(self.suite)
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                self.suite=json.loads(original)
                mutation(self.suite)
                self.assertEqual(self.invoke()[0],2)

    def test_duplicate_keys_nan_and_malformed_json(self):
        for text in ('{"cases":[],"cases":[]}','{"v":NaN}','{'):
            self.manifest.write_text(text)
            with self.assertRaises(checker.Invalid):checker.load_json(self.manifest)

    def test_symlink_fixture_rejected(self):
        (self.inputs/'escape.cg').symlink_to(self.tool)
        self.assertEqual(self.invoke()[0],2)

    def test_report_never_overwrites(self):
        out=self.root/'existing.json'
        out.write_text('existing')
        self.manifest.write_text(json.dumps(self.suite))
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(checker.main(['--suite',str(self.manifest),'--report',str(out)]),2)
        self.assertEqual(out.read_text(),'existing')

    def test_symlink_and_traversal_report_paths(self):
        link=self.root/'link'
        link.symlink_to(self.work,target_is_directory=True)
        self.assertEqual(self.invoke(report=link/'out.json')[0],2)
        self.assertEqual(self.invoke(report=self.work/'..'/'out.json')[0],2)
        self.assertFalse((self.root/'out.json').exists())

    def test_report_cannot_replace_fixture(self):
        self.assertEqual(self.invoke(report=self.inputs/'new.json')[0],2)
        self.assertFalse((self.inputs/'new.json').exists())

    def test_output_limit_and_timeout_are_failure(self):
        for body in ('print("x"*1100000)','import time; time.sleep(2)'):
            self.tool.write_text('#!/usr/bin/env python3\nimport sys\nif "--help" in sys.argv: print("help"); sys.exit(0)\n'+body+'\n')
            status, report=self.invoke(extra=['--timeout','0.25'])
            self.assertEqual(status,1)
            self.assertIn(report['results'][0]['steps'][0]['problem'],('timeout','output_limit'))

    def test_pin_and_tool_argument_validation(self):
        for extra in (['--pin','unused='+'0'*64],['--pin','compiler='+'0'*64],['--tool','artifact=/tmp/no'],['--timeout','61']):
            self.assertEqual(self.invoke(extra=extra)[0],2)
        self.assertEqual(self.invoke(pin='XYZ')[0],2)

    def test_json_expectation_is_type_strict(self):
        self.assertTrue(checker.matches('{"schema":1,"extra":2}',{'json':{'schema':1}}))
        self.assertFalse(checker.matches('{"schema":true}',{'json':{'schema':1}}))


class InventoryTests(unittest.TestCase):
    def test_all_requirements_and_cases_linked(self):
        suite,_=checker.load_suite(checker.DEFAULT_SUITE)
        inventory=checker.load_json(checker.DEFAULT_SUITE.with_name('inventory.json'))
        self.assertEqual({r['id'] for r in inventory['requirements']},{f'R{i:02}' for i in range(1,16)})
        cases={c['id']:c for c in suite['cases']}
        for row in inventory['requirements']:
            self.assertEqual(row['release_gate'],'open')
            self.assertTrue(row['cases'])
            for case in row['cases']:
                self.assertIn(row['id'],cases[case]['requirements'])
        commands=inventory['commands']
        self.assertEqual({c['id'] for c in commands},{'check','build','run','script','test','verify','emit-rust','emit-python','fmt','api','notebook-open','notebook-run','bind','memory-report','memory-failure-report','security','performance'})
        for row in commands:
            self.assertTrue(row['route'])
            self.assertTrue(row['limitation'])
            for case in row['cases']:self.assertIn(case,cases)
        linked={c for r in inventory['requirements'] for c in r['cases']}
        self.assertEqual(linked,set(cases))


if __name__=='__main__':unittest.main()
