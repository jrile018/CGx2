#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import unittest
spec=importlib.util.spec_from_file_location("ordinary_memory_runner",Path(__file__).with_name("run.py"))
runner=importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
class EvidenceTests(unittest.TestCase):
    def test_signal_timeout_and_unrelated_error_never_satisfy_negative(self):
        for reason,exit,signal in [("signal",-9,9),("timeout",1,None),("overflow",1,None),("exit",0,None)]:
            self.assertFalse(runner.normal({"reason":reason,"exit":exit,"signal":signal},1))
    def test_exact_span_code_and_error_exit_are_all_required(self):
        record={"reason":"exit","exit":1,"signal":None,"stdout":'{"diagnostics":[{"code":"CGT095","span":{"start":3,"end":5}}]}'}
        self.assertTrue(runner.rejection(record,"CGT095",3,5))
        self.assertFalse(runner.rejection(record,"CGT090",3,5))
        self.assertFalse(runner.rejection(record,"CGT095",3,6))
    def test_unavailable_metrics_are_distinct_from_zero(self):
        a={"stderr":'CGX2_MEMORY_EVIDENCE {"session_entered":false,"metrics":null}\n'}
        self.assertEqual(runner.observed(a),{"session_entered":False,"metrics":None})
        self.assertNotEqual(runner.observed(a),{"charged_bytes":0})
    def test_observation_requires_exactly_one_marker(self):
        self.assertIsNone(runner.observed({"stderr":""}))
        self.assertIsNone(runner.observed({"stderr":"CGX2_MEMORY_EVIDENCE {}\nCGX2_MEMORY_EVIDENCE {}\n"}))
    def test_json_diagnostics_on_stderr_are_checked(self):
        record={"reason":"exit","exit":1,"signal":None,"stdout":"","stderr":'{"code":"CGT095","span":{"start":3,"end":5}}'}
        self.assertTrue(runner.rejection(record,"CGT095",3,5))
    def test_malformed_memory_marker_cannot_pass(self):
        self.assertIsNone(runner.observed({"stderr":"CGX2_MEMORY_EVIDENCE garbage\n"}))
    def test_historical_negative_must_be_the_exact_known_failure(self):
        row={"label":"shadow-builtin:reject","reason":"exit","exit":0,"signal":None,"passed":False}
        report={"passed":523,"failed":1,"commands":[row]}
        self.assertTrue(runner.historical_shadow_failure(report))
        row["label"]="unrelated exception"
        self.assertFalse(runner.historical_shadow_failure(report))
    def test_only_pause_observations_are_normalized(self):
        self.assertEqual(runner.stable({"pause":{"total_ns":2},"charged_bytes":42}),{"charged_bytes":42})
        self.assertNotEqual(runner.stable({"charged_bytes":42}),runner.stable({"charged_bytes":41}))
if __name__=="__main__":unittest.main()
