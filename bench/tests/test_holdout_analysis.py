"""Synthetic statistics checks only; these observations are not model evidence."""

import math
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import holdout_analysis as analysis


def row(language, outcome="pass", *, tokens=100, seconds=10, family="family", seed=0):
    return {"language": language, "hidden_test_result": outcome,
            "prompt_tokens": tokens // 2, "completion_tokens": tokens - tokens // 2,
            "tool_tokens": 0, "wall_seconds": seconds,
            "token_budget": 1000, "time_budget_seconds": 100,
            "model_family": family, "model_version": "v1", "seed": seed}


def pair(task_id, tier, family, seed, cgx, baseline):
    return {"task_id": task_id, "tier": tier, "family": family,
            "baseline_language": "python", "cgx": cgx, "baseline": baseline}


class TaskClusterStatisticsTests(unittest.TestCase):
    def test_cluster_interval_exposes_iid_row_false_certainty(self):
        pairs = []
        for task in range(30):
            for seed in range(20):
                positive = task < 17
                pairs.append(pair(f"task-{task}", "small" if task % 2 else "large",
                                  "family", seed,
                                  row("cgx2", "pass" if positive else "fail", seed=seed),
                                  row("python", "fail" if positive else "pass", seed=seed)))
        metric = analysis._measure(pairs, "completion_difference", ["family"], ["large", "small"])
        self.assertEqual(metric["count"], 30)
        self.assertEqual(metric["distinct_tasks"], 30)
        self.assertEqual(metric["seed_variation"]["median_within_cluster_range"], 0)
        self.assertLess(metric["confidence_interval_95"][0], -0.05)
        # An IID-row normal interval on the same 600 rows excludes the margin.
        iid_lower = metric["estimate"] - 1.96 * math.sqrt(
            (1 - metric["estimate"] ** 2) / len(pairs))
        self.assertGreater(iid_lower, -0.05)

    def test_ties_near_margin_and_seed_variation(self):
        tied = [pair(f"task-{task}", "small" if task % 2 else "large", "family", seed,
                     row("cgx2", seed=seed), row("python", seed=seed))
                for task in range(30) for seed in (1, 2)]
        tie_metric = analysis._measure(tied, "completion_difference", ["family"], ["large", "small"])
        self.assertEqual(tie_metric["confidence_interval_95"], [0, 0])
        self.assertEqual(analysis._gate(tie_metric, completion=True), "met")
        for item in tied:
            if item["task_id"] == "task-0" and item["cgx"]["seed"] == 2:
                item["cgx"]["hidden_test_result"] = "fail"
        near = analysis._measure(tied, "completion_difference", ["family"], ["large", "small"])
        self.assertEqual(near["seed_variation"]["clusters_with_multiple_observations"], 30)
        self.assertEqual(near["seed_variation"]["median_within_cluster_range"], 0)
        self.assertEqual(analysis._gate(near, completion=True), "uncertain")

    def test_family_gate_prevents_pooled_win_hiding_one_family(self):
        tasks, traces = [], []
        for number in range(30):
            task_id = f"task-{number}"
            tasks.append({"task_id": task_id, "tier": "small" if number % 2 else "large",
                          "baseline_language": "python", "exclusions": {}})
            for family, cgx_tokens, cgx_seconds in (("a", 20, 2), ("b", 90, 9)):
                for seed in (1, 2):
                    for language, tokens, seconds in (("cgx2", cgx_tokens, cgx_seconds),
                                                      ("python", 100, 10)):
                        observation = row(language, tokens=tokens, seconds=seconds,
                                          family=family, seed=seed)
                        observation["task_id"] = task_id
                        traces.append(observation)
        report = analysis.analyze_ready(
            {"tasks": tasks, "conditions": ["cgx2", "python"], "kind": "holdout"}, traces)
        self.assertEqual(report["gates"]["paired_total_tokens"], "met")
        self.assertEqual(report["family_gates"]["b"]["gates"]["paired_total_tokens"], "failed")
        self.assertEqual(report["release_verdict"], "gates_failed")
        self.assertEqual(set(report["comparison"]["total_token_ratio"]["strata"]),
                         {"a/small", "a/large", "b/small", "b/large"})
        self.assertEqual(report["comparison"]["total_token_ratio"]["distinct_tasks"], 30)

    def test_failure_charge_and_output_tokens_are_separate(self):
        successful = pair("a", "small", "family", 1,
                          row("cgx2", tokens=50), row("python", tokens=100))
        successful["cgx"]["prompt_tokens"] = 40
        successful["cgx"]["completion_tokens"] = 10
        failed = pair("b", "large", "family", 1,
                      row("cgx2", "fail", tokens=1, seconds=1),
                      row("python", tokens=100))
        summary = analysis._condition_summary([successful["cgx"], failed["cgx"]])
        self.assertEqual(summary["charged_tokens"], 1050)
        self.assertEqual(summary["charged_seconds"], 110)
        self.assertEqual(summary["tokens_per_success"], 1050)
        self.assertEqual(analysis._measure([successful, failed], "total_token_ratio",
                                           ["family"], ["large", "small"])["estimate"], 0.5)
        self.assertEqual(analysis._measure([successful, failed], "output_token_ratio",
                                           ["family"], ["large", "small"])["estimate"], 0.2)

    def test_confidence_bound_decisions(self):
        metric = {"estimate": 0.79, "confidence_interval_95": [0.7, 1.01]}
        self.assertEqual(analysis._gate(metric), "uncertain")
        metric["confidence_interval_95"] = [0.7, 0.99]
        self.assertEqual(analysis._gate(metric), "met")
        metric["confidence_interval_95"] = [0.81, 0.95]
        self.assertEqual(analysis._gate(metric), "failed")
        metric = {"estimate": -0.04, "confidence_interval_95": [-0.05, 0.1]}
        self.assertEqual(analysis._gate(metric, completion=True), "uncertain")


if __name__ == "__main__":
    unittest.main()
