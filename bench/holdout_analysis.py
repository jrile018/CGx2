"""Task-clustered paired LLM statistics for audited external traces."""

from collections import defaultdict
import random
import statistics

MIN_INTERVAL_TASKS = 30
BOOTSTRAP_DRAWS = 2000
BOOTSTRAP_SEED = 0
COMPLETION_MARGIN = -0.05
IMPROVEMENT_RATIO = 0.80
METRICS = ("completion_difference", "total_token_ratio", "time_ratio", "output_token_ratio")


def _percentile(values, fraction):
    place = (len(values) - 1) * fraction
    low = int(place)
    high = min(low + 1, len(values) - 1)
    return values[low] + (values[high] - values[low]) * (place - low)


def _tokens(row):
    return row["prompt_tokens"] + row["completion_tokens"] + row.get("tool_tokens", 0)


def _condition_summary(rows):
    successes = sum(row["hidden_test_result"] == "pass" for row in rows)
    charged_seconds = sum(row["wall_seconds"] if row["hidden_test_result"] == "pass"
                          else row["time_budget_seconds"] for row in rows)
    charged_tokens = sum(_tokens(row) if row["hidden_test_result"] == "pass"
                         else row["token_budget"] for row in rows)
    return {"runs": len(rows), "verified_successes": successes,
            "completion_rate": successes / len(rows) if rows else None,
            "charged_seconds": charged_seconds, "charged_tokens": charged_tokens,
            "seconds_per_success": charged_seconds / successes if successes else None,
            "tokens_per_success": charged_tokens / successes if successes else None,
            "timeouts_censored": sum(row["hidden_test_result"] == "timeout" for row in rows)}


def _observations(runs, metric):
    if metric == "completion_difference":
        return [int(p["cgx"]["hidden_test_result"] == "pass")
                - int(p["baseline"]["hidden_test_result"] == "pass") for p in runs]
    successes = [p for p in runs if p["cgx"]["hidden_test_result"] == "pass"
                 and p["baseline"]["hidden_test_result"] == "pass"]
    if metric == "total_token_ratio":
        return [_tokens(p["cgx"]) / _tokens(p["baseline"]) for p in successes]
    if metric == "output_token_ratio":
        return [p["cgx"]["completion_tokens"] / p["baseline"]["completion_tokens"]
                for p in successes if p["baseline"]["completion_tokens"] > 0]
    return [p["cgx"]["wall_seconds"] / p["baseline"]["wall_seconds"] for p in successes]


def _task_family_values(pairs, metric):
    """Reduce repeated seeds before comparing tasks; family remains a paired measure."""
    groups = defaultdict(list)
    for pair in pairs:
        groups[(pair["task_id"], pair["family"])].append(pair)
    rows, ranges = [], []
    for (task_id, family), runs in sorted(groups.items()):
        observations = _observations(runs, metric)
        if not observations:
            continue
        value = (statistics.mean(observations) if metric == "completion_difference"
                 else statistics.median(observations))
        rows.append({"task_id": task_id, "family": family, "tier": runs[0]["tier"],
                     "value": value})
        if len(observations) > 1:
            ranges.append(max(observations) - min(observations))
    return rows, ranges


def _estimate(rows, metric):
    values = [row["value"] for row in rows]
    if not values:
        return None
    return statistics.mean(values) if metric == "completion_difference" else statistics.median(values)


def _measure(pairs, metric, families, tiers):
    rows, ranges = _task_family_values(pairs, metric)
    task_ids = sorted({row["task_id"] for row in rows})
    strata = {}
    for family in families:
        for tier in tiers:
            subset = [row for row in rows if row["family"] == family and row["tier"] == tier]
            strata[f"{family}/{tier}"] = {"task_count": len({r["task_id"] for r in subset}),
                                           "estimate": _estimate(subset, metric)}
    result = {"count": len(rows), "distinct_tasks": len(task_ids),
              "estimate": _estimate(rows, metric), "strata": strata,
              "seed_variation": {"clusters_with_multiple_observations": len(ranges),
                                 "median_within_cluster_range": statistics.median(ranges)
                                 if ranges else None},
              "interval_status": "insufficient_power", "confidence_interval_95": None}
    if len(task_ids) < MIN_INTERVAL_TASKS or any(cell["task_count"] < 2 for cell in strata.values()):
        return result
    by_task = defaultdict(list)
    for row in rows:
        by_task[row["task_id"]].append(row)
    tier_tasks = {tier: sorted({r["task_id"] for r in rows if r["tier"] == tier}) for tier in tiers}
    rng = random.Random(BOOTSTRAP_SEED)
    samples = []
    for _ in range(BOOTSTRAP_DRAWS):
        sampled = [row for tier in tiers for _ in tier_tasks[tier]
                   for row in by_task[tier_tasks[tier][rng.randrange(len(tier_tasks[tier]))]]]
        samples.append(_estimate(sampled, metric))
    samples.sort()
    result.update(interval_status="computed",
                  confidence_interval_95=[_percentile(samples, 0.025),
                                          _percentile(samples, 0.975)])
    return result


def _gate(metric, *, completion=False):
    interval = metric["confidence_interval_95"]
    if interval is None:
        return "uncertain"
    if completion:
        if interval[0] > COMPLETION_MARGIN:
            return "met"
        return "failed" if interval[1] <= COMPLETION_MARGIN else "uncertain"
    if interval[0] > IMPROVEMENT_RATIO:
        return "failed"
    if metric["estimate"] <= IMPROVEMENT_RATIO and interval[1] < 1:
        return "met"
    return "uncertain"


def _gates(metrics):
    return {"completion_noninferiority": _gate(metrics["completion_difference"], completion=True),
            "paired_total_tokens": _gate(metrics["total_token_ratio"]),
            "paired_wall_time": _gate(metrics["time_ratio"])}


def analyze_ready(matrix, traces):
    tasks = {task["task_id"]: task for task in matrix["tasks"]}
    keyed = {(row["task_id"], row["model_family"], row["model_version"], row["seed"],
              row["language"]): row for row in traces}
    pairs = []
    for task_id, task in sorted(tasks.items()):
        for key, cgx in sorted(keyed.items()):
            if key[0] != task_id or key[4] != "cgx2":
                continue
            baseline = keyed.get(key[:4] + (task["baseline_language"],))
            if baseline is not None:
                pairs.append({"task_id": task_id, "family": key[1], "tier": task["tier"],
                              "baseline_language": task["baseline_language"],
                              "cgx": cgx, "baseline": baseline})
    families = sorted({p["family"] for p in pairs})
    tiers = sorted({p["tier"] for p in pairs})
    comparison = {metric: _measure(pairs, metric, families, tiers) for metric in METRICS}
    comparison.update({"baseline_rule": "each task's preregistered baseline_language",
                       "matched_runs": len(pairs), "matched_tasks": len({p["task_id"] for p in pairs}),
                       "paired_successes": sum(p["cgx"]["hidden_test_result"] == "pass" and
                                               p["baseline"]["hidden_test_result"] == "pass" for p in pairs),
                       "ratio_direction": "CGx2 / task baseline on runs passed by both"})
    conditions = {language: _condition_summary([r for r in traces if r["language"] == language])
                  for language in matrix["conditions"] if any(r["language"] == language for r in traces)}
    by_baseline = {}
    for language in sorted({p["baseline_language"] for p in pairs}):
        subset = [p for p in pairs if p["baseline_language"] == language]
        by_baseline[language] = {
            "task_count": len({p["task_id"] for p in subset}), "matched_runs": len(subset),
            "paired_successes": sum(p["cgx"]["hidden_test_result"] == "pass" and
                                    p["baseline"]["hidden_test_result"] == "pass" for p in subset),
            "completion_difference": _estimate(_task_family_values(
                subset, "completion_difference")[0], "completion_difference"),
            "total_token_ratio": _estimate(_task_family_values(
                subset, "total_token_ratio")[0], "total_token_ratio")}
    report = {"conditions": conditions,
              "selected_baseline": _condition_summary([p["baseline"] for p in pairs]),
              "comparison": comparison, "baseline_breakdown": by_baseline,
              "exclusions_by_condition": {language: sum(language in task["exclusions"]
                                                       for task in tasks.values())
                                          for language in matrix["conditions"]},
              "task_counts": {"registered": len(tasks), "matched": comparison["matched_tasks"],
                              "by_tier": {tier: sum(t["tier"] == tier for t in tasks.values())
                                          for tier in sorted({t["tier"] for t in tasks.values()})}},
              "bootstrap": {"unit": "task; paired families and seeds move together",
                            "stratification": "repository tier; family/tier coverage required",
                            "seed": BOOTSTRAP_SEED, "draws": BOOTSTRAP_DRAWS,
                            "min_distinct_tasks": MIN_INTERVAL_TASKS},
              "gates": None, "family_gates": None, "release_verdict": None,
              "uncertainty": "95% task bootstrap intervals; seed variation is descriptive. "
                             "Public calibration is not release evidence."}
    if matrix["kind"] == "holdout":
        gates = _gates(comparison)
        family_gates = {}
        for family in families:
            subset = [p for p in pairs if p["family"] == family]
            metrics = {metric: _measure(subset, metric, [family], tiers) for metric in METRICS}
            family_gates[family] = {"metrics": metrics, "gates": _gates(metrics)}
        report["gates"] = gates
        report["family_gates"] = family_gates
        decisions = list(gates.values()) + [gate for item in family_gates.values()
                                             for gate in item["gates"].values()]
        report["release_verdict"] = ("gates_failed" if "failed" in decisions else
                                     "uncertain" if "uncertain" in decisions else "gates_met")
    return report


def markdown(report):
    lines = [f"# CGx2 {report['scope']} paired report", "", f"Audit: **{report['status']}**.", ""]
    if report["comparison"] is None:
        lines += [report["uncertainty"], "", "No LLM comparison or completion rate is available."]
    else:
        c = report["comparison"]
        lines += [f"Tasks: {c['matched_tasks']} matched / {report['task_counts']['registered']} registered; "
                  f"runs: {c['matched_runs']} matched; paired successes: {c['paired_successes']}.", "",
                  "| Metric | Estimate | 95% task interval |", "| --- | ---: | --- |"]
        for key in METRICS:
            item = c[key]
            lines.append(f"| {key} | {item['estimate']} | "
                         f"{item['confidence_interval_95'] or item['interval_status']} |")
        lines += ["", "| Condition | Successes / runs | Tokens per success | Seconds per success |",
                  "| --- | ---: | ---: | ---: |"]
        for language, item in report["conditions"].items():
            lines.append(f"| {language} | {item['verified_successes']} / {item['runs']} | "
                         f"{item['tokens_per_success']} | {item['seconds_per_success']} |")
        baseline = report["selected_baseline"]
        lines.append(f"| selected task baselines | {baseline['verified_successes']} / {baseline['runs']} | "
                     f"{baseline['tokens_per_success']} | {baseline['seconds_per_success']} |")
        lines += ["", f"Excluded tasks by condition: {report['exclusions_by_condition']}.",
                  f"Baseline breakdown: {report['baseline_breakdown']}.",
                  f"Gates by model family: "
                  f"{ {family: item['gates'] for family, item in (report['family_gates'] or {}).items()} }.",
                  "Failures are charged full token and wall-time budgets in all-task cost.",
                  f"Release verdict: {report['release_verdict'] or 'none'}.", "", report["uncertainty"]]
    lines += ["", f"Runtime evidence: {report['runtime_evidence']}."]
    return "\n".join(lines) + "\n"
