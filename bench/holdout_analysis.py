"""Deterministic paired metrics and task-cluster uncertainty for audited traces."""

import random
import statistics

MIN_INTERVAL_TASKS = 30
BOOTSTRAP_DRAWS = 2000


def _percentile(sorted_values, fraction):
    place = (len(sorted_values) - 1) * fraction
    low = int(place)
    high = min(low + 1, len(sorted_values) - 1)
    return sorted_values[low] + (sorted_values[high] - sorted_values[low]) * (place - low)


def _measure(pairs, value):
    task_ids = sorted({task_id for task_id, _ in pairs})
    estimate = value([pair for _, pair in pairs]) if pairs else None
    if len(task_ids) < MIN_INTERVAL_TASKS:
        return {"count": len(pairs), "distinct_tasks": len(task_ids), "estimate": estimate,
                "interval_status": "insufficient_power", "confidence_interval_95": None}
    by_task = {task_id: [pair for task, pair in pairs if task == task_id] for task_id in task_ids}
    rng = random.Random(0)
    samples = []
    for _ in range(BOOTSTRAP_DRAWS):
        sample = [pair for _ in task_ids for pair in by_task[task_ids[rng.randrange(len(task_ids))]]]
        samples.append(value(sample))
    samples.sort()
    return {"count": len(pairs), "distinct_tasks": len(task_ids), "estimate": estimate,
            "interval_status": "computed",
            "confidence_interval_95": [_percentile(samples, 0.025), _percentile(samples, 0.975)]}


def analyze_ready(matrix, traces):
    report = {}

    def condition_summary(rows):
        successes = sum(row["hidden_test_result"] == "pass" for row in rows)
        charged_seconds = sum(row["wall_seconds"] if row["hidden_test_result"] == "pass"
                              else row["time_budget_seconds"] for row in rows)
        charged_tokens = sum(row["prompt_tokens"] + row["completion_tokens"] if row["hidden_test_result"] == "pass"
                             else row["token_budget"] for row in rows)
        return {"runs": len(rows), "verified_successes": successes,
                "completion_rate": successes / len(rows) if rows else None,
                "charged_seconds": charged_seconds, "charged_tokens": charged_tokens,
                "seconds_per_success": charged_seconds / successes if successes else None,
                "tokens_per_success": charged_tokens / successes if successes else None,
                "timeouts_censored": sum(row["hidden_test_result"] == "timeout" for row in rows)}
    conditions = {language: condition_summary([row for row in traces if row["language"] == language])
                  for language in matrix["conditions"] if any(row["language"] == language for row in traces)}
    keyed = {(row["task_id"], row["model_family"], row["model_version"], row["seed"], row["language"]): row
             for row in traces}
    paired = []
    for task in matrix["tasks"]:
        task_id = task["task_id"]
        baseline = task["baseline_language"]
        for key, cgx in keyed.items():
            if key[0] == task_id and key[4] == "cgx2":
                other = keyed.get(key[:4] + (baseline,))
                if other is not None:
                    paired.append((task_id, (cgx, other)))
    completion = _measure(paired, lambda ps: statistics.mean(
        int(c["hidden_test_result"] == "pass") - int(b["hidden_test_result"] == "pass") for c, b in ps))
    success_pairs = [(task_id, pair) for task_id, pair in paired if all(
        row["hidden_test_result"] == "pass" for row in pair)]

    def ratio(field):
        return _measure(success_pairs, lambda ps: statistics.median(
            (c[field] if field == "wall_seconds" else c["prompt_tokens"] + c["completion_tokens"])
            / (b[field] if field == "wall_seconds" else b["prompt_tokens"] + b["completion_tokens"])
            for c, b in ps))
    time_ratio = ratio("wall_seconds")
    token_ratio = ratio("tokens")
    comparison = {"baseline_rule": "each task's preregistered baseline_language",
                  "matched_runs": len(paired), "paired_successes": len(success_pairs),
                  "completion_difference": completion, "time_ratio": time_ratio,
                  "total_token_ratio": token_ratio,
                  "ratio_direction": "CGx2 / task baseline on runs passed by both"}
    report.update(conditions=conditions,
                  selected_baseline=condition_summary([baseline for _, (_, baseline) in paired]),
                  comparison=comparison)
    if matrix["kind"] == "holdout":
        gates = {}
        ci = completion["confidence_interval_95"]
        gates["completion_noninferiority"] = (
            "uncertain" if ci is None else
            "met" if completion["estimate"] >= -0.05 and ci[0] > -0.05 else
            "failed" if completion["estimate"] < -0.05 or ci[1] <= -0.05 else "uncertain")
        for key, metric in (("paired_total_tokens", token_ratio), ("paired_wall_time", time_ratio)):
            ci = metric["confidence_interval_95"]
            gates[key] = ("uncertain" if ci is None else
                          "failed" if metric["estimate"] > 0.8 or ci[0] >= 1 else
                          "met" if ci[1] < 1 else "uncertain")
        report["gates"] = gates
        report["release_verdict"] = ("gates_failed" if "failed" in gates.values() else
                                     "uncertain" if "uncertain" in gates.values() else "gates_met")
    report["uncertainty"] = ("Task-cluster bootstrap 95% intervals; at least 30 distinct eligible tasks per metric. "
                             "Cost includes failures at full budget; timeouts are censored in all-task cost. "
                             "Public calibration never supports a release verdict.")
    return report


def markdown(report):
    lines = [f"# CGx2 {report['scope']} paired report", "", f"Audit: **{report['status']}**.", ""]
    if report["comparison"] is None:
        lines += [report["uncertainty"], "", "No LLM comparison or completion rate is available."]
    else:
        c = report["comparison"]
        lines += [f"Matched runs: {c['matched_runs']}; paired successes: {c['paired_successes']}.", "",
                  "| Gate metric | Estimate | 95% interval |", "| --- | ---: | --- |"]
        for key in ("completion_difference", "total_token_ratio", "time_ratio"):
            item = c[key]
            lines.append(f"| {key} | {item['estimate']} | {item['confidence_interval_95'] or item['interval_status']} |")
        lines += ["", "| Condition | Successes / runs | Tokens per success | Seconds per success |",
                  "| --- | ---: | ---: | ---: |"]
        for language, item in report["conditions"].items():
            lines.append(f"| {language} | {item['verified_successes']} / {item['runs']} | "
                         f"{item['tokens_per_success']} | {item['seconds_per_success']} |")
        baseline = report["selected_baseline"]
        lines.append(f"| selected task baselines | {baseline['verified_successes']} / {baseline['runs']} | "
                     f"{baseline['tokens_per_success']} | {baseline['seconds_per_success']} |")
        lines += ["", "All-task failures are charged the full token and wall-time budget.",
                  f"Release verdict: {report['release_verdict'] or 'none'}.", "",
                  report["uncertainty"]]
    lines += ["", f"Runtime evidence: {report['runtime_evidence']}."]
    return "\n".join(lines) + "\n"
