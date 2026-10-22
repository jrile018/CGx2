import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import report


def result(task, language, variant, *, model="m1", seed=1, success=True,
           tokens=80, output=20, elapsed=2.0):
    return {
        "schema_version": 1, "task_id": task, "tier": "small", "language": language,
        "variant": variant, "model_id": model, "seed": seed,
        "verified_completion": success, "failure_reason": None if success else "unexpected_exit",
        "wall_seconds": elapsed, "charged_wall_seconds": 10.0 if not success else elapsed,
        "timeout_seconds": 10.0, "budget_total_tokens": 100,
        "total_tokens": tokens, "output_tokens": output,
    }


class ReportTests(unittest.TestCase):
    def test_only_matching_task_model_seed_enter_pair(self):
        rows = [result("a", "python", "standard"), result("a", "rust", "standard"),
                result("b", "python", "standard"), result("a", "rust", "standard", seed=2),
                result("a", "rust", "standard", model="m2")]
        data = report.summarize(rows)
        pair = data["comparisons"][0]
        self.assertEqual(pair["matched_pairs"], 1)
        self.assertEqual(pair["unmatched_left"], 1)
        self.assertEqual(pair["unmatched_right"], 2)

    def test_missing_tokens_stay_missing(self):
        rows = [result("a", "python", "standard", tokens=None, output=None),
                result("a", "rust", "standard")]
        data = report.summarize(rows)
        python = next(x for x in data["conditions"] if x["language"] == "python")
        self.assertIsNone(python["median_total_tokens"])
        self.assertIsNone(data["comparisons"][0]["total_token_ratio"]["estimate"])

    def test_failure_charges_full_time_and_token_budget(self):
        rows = [result("a", "python", "standard", success=False, elapsed=0.1, tokens=4),
                result("a", "rust", "standard")]
        data = report.summarize(rows)
        python = next(x for x in data["conditions"] if x["language"] == "python")
        self.assertEqual(python["all_task_charged_seconds"], 10.0)
        self.assertEqual(python["all_task_charged_tokens"], 100)
        self.assertEqual(python["verified_successes"], 0)

    def test_output_and_total_tokens_are_separate(self):
        rows = [result("a", "python", "standard", tokens=90, output=10),
                result("a", "rust", "standard", tokens=45, output=9)]
        data = report.summarize(rows)
        pair = data["comparisons"][0]
        self.assertNotEqual(pair["output_token_ratio"]["estimate"], pair["total_token_ratio"]["estimate"])

    def test_syntax_candidate_labels_and_small_interval(self):
        rows = [result("a", "cgx2", "indent"), result("a", "cgx2", "brace")]
        data = report.summarize(rows)
        labels = {(x["language"], x["variant"]) for x in data["conditions"]}
        self.assertEqual(labels, {("cgx2", "indent"), ("cgx2", "brace")})
        self.assertEqual(data["comparisons"][0]["completion_difference"]["interval_status"], "insufficient_data")
        self.assertIsNone(data["release_conclusion"])

    def test_thirty_pairs_give_deterministic_bootstrap_intervals(self):
        rows = []
        for i in range(30):
            rows.extend([result(f"task{i}", "python", "standard", elapsed=2.0),
                         result(f"task{i}", "rust", "standard", elapsed=1.0)])
        a = report.summarize(rows)
        b = report.summarize(rows)
        self.assertEqual(a, b)
        self.assertEqual(a["comparisons"][0]["time_ratio"]["interval_status"], "computed")
        self.assertEqual(a["comparisons"][0]["time_ratio"]["confidence_interval_95"], [0.5, 0.5])

    def test_directory_cli_report_is_json_and_rejects_duplicate_case(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name, row in (("one", result("a", "python", "standard")),
                              ("two", result("a", "python", "standard"))):
                (root / f"{name}.json").write_text(json.dumps(row))
            with self.assertRaises(ValueError):
                report.load_results(root)

    def test_invalid_budget_cannot_reduce_failure_cost(self):
        bad = result("a", "python", "standard", success=False)
        bad["budget_total_tokens"] = -10
        with self.assertRaises(ValueError):
            report.summarize([bad])


if __name__ == "__main__":
    unittest.main()
