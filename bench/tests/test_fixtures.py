import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import report
import runner


ROOT = Path(__file__).resolve().parents[1]


class FixtureTests(unittest.TestCase):
    def test_public_smoke_is_harness_only(self):
        result = runner.run_manifest(ROOT / "fixtures" / "smoke.json")
        self.assertTrue(result["verified_completion"])
        self.assertEqual(result["language"], "harness")
        self.assertIsNone(result["total_tokens"])

    def test_sample_report_has_success_failure_and_missing_tokens(self):
        data = report.summarize(report.load_results(ROOT / "fixtures" / "sample-results"))
        self.assertFalse(data["cgx2_language_result_measured"])
        self.assertIsNone(data["release_conclusion"])
        self.assertTrue(any(x["verified_successes"] < x["runs"] for x in data["conditions"]))
        self.assertTrue(any(x["missing_total_token_traces"] for x in data["conditions"]))

    def test_syntax_samples_are_distinct_and_marked_provisional(self):
        sample_dir = ROOT / "fixtures" / "syntax"
        indent = (sample_dir / "indent.cg").read_text()
        brace = (sample_dir / "brace.cg").read_text()
        self.assertIn("candidate", indent.lower())
        self.assertIn("candidate", brace.lower())
        self.assertNotEqual(indent, brace)


if __name__ == "__main__":
    unittest.main()
