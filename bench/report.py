"""Summarize on-demand baseline/pilot results without release claims."""

import argparse
import itertools
import json
import random
import statistics
from pathlib import Path


def _require_result(row):
    if not isinstance(row, dict):
        raise ValueError("result must be a JSON object")
    for key in ("task_id", "tier", "language", "variant", "model_id"):
        if not isinstance(row.get(key), str) or not row[key]:
            raise ValueError(f"result {key} must be a nonempty string")
    for key in ("seed", "budget_total_tokens"):
        if type(row.get(key)) is not int:
            raise ValueError(f"result {key} must be an integer")
    if row["budget_total_tokens"] <= 0:
        raise ValueError("result budget_total_tokens must be positive")
    if type(row.get("verified_completion")) is not bool:
        raise ValueError("verified_completion must be Boolean")
    for key in ("wall_seconds", "charged_wall_seconds", "timeout_seconds"):
        if type(row.get(key)) not in (int, float) or row[key] < 0:
            raise ValueError(f"result {key} must be nonnegative")
    for key in ("total_tokens", "output_tokens"):
        if row.get(key) is not None and (type(row[key]) is not int or row[key] < 0):
            raise ValueError(f"result {key} must be nonnegative or null")
    return row


def _condition(row):
    return row["language"], row["variant"]


def _case(row):
    return row["task_id"], row["tier"], row["model_id"], row["seed"]


def _median(values):
    return statistics.median(values) if values else None


def _quantile(sorted_values, fraction):
    place = (len(sorted_values) - 1) * fraction
    low = int(place)
    high = min(low + 1, len(sorted_values) - 1)
    return sorted_values[low] + (sorted_values[high] - sorted_values[low]) * (place - low)


def _metric(values, estimate_fn):
    count = len(values)
    estimate = estimate_fn(values) if count else None
    if count < 30:
        return {"count": count, "estimate": estimate, "interval_status": "insufficient_data",
                "confidence_interval_95": None}
    rng = random.Random(0)
    samples = sorted(estimate_fn([values[rng.randrange(count)] for _ in range(count)]) for _ in range(10000))
    return {"count": count, "estimate": estimate, "interval_status": "computed",
            "confidence_interval_95": [_quantile(samples, 0.025), _quantile(samples, 0.975)]}


def _condition_summary(label, rows):
    successes = sum(row["verified_completion"] for row in rows)
    charged_seconds = sum(row["timeout_seconds"] if not row["verified_completion"] else row["wall_seconds"] for row in rows)
    charged_tokens = [row["budget_total_tokens"] if not row["verified_completion"] else row.get("total_tokens") for row in rows]
    total_tokens = [row["total_tokens"] for row in rows if row.get("total_tokens") is not None]
    output_tokens = [row["output_tokens"] for row in rows if row.get("output_tokens") is not None]
    token_charge = sum(charged_tokens) if all(value is not None for value in charged_tokens) else None
    return {
        "language": label[0], "variant": label[1], "runs": len(rows),
        "verified_successes": successes, "completion_rate": successes / len(rows),
        "median_wall_seconds": _median([row["wall_seconds"] for row in rows if row["verified_completion"]]),
        "median_total_tokens": _median(total_tokens), "median_output_tokens": _median(output_tokens),
        "missing_total_token_traces": len(rows) - len(total_tokens),
        "missing_output_token_traces": len(rows) - len(output_tokens),
        "all_task_charged_seconds": charged_seconds,
        "all_task_charged_tokens": token_charge,
        "seconds_per_verified_success": charged_seconds / successes if successes else None,
        "tokens_per_verified_success": token_charge / successes if successes and token_charge is not None else None,
    }


def _comparison(left_label, right_label, left, right):
    common = sorted(left.keys() & right.keys())
    completion = [int(right[k]["verified_completion"]) - int(left[k]["verified_completion"]) for k in common]
    both = [(left[k], right[k]) for k in common if left[k]["verified_completion"] and right[k]["verified_completion"]]

    def ratios(key):
        return [r[key] / l[key] for l, r in both if l.get(key) is not None and r.get(key) is not None and l[key] > 0]

    return {
        "left": {"language": left_label[0], "variant": left_label[1]},
        "right": {"language": right_label[0], "variant": right_label[1]},
        "matched_pairs": len(common), "unmatched_left": len(left) - len(common),
        "unmatched_right": len(right) - len(common),
        "completion_difference": _metric(completion, statistics.mean),
        "time_ratio": _metric(ratios("wall_seconds"), statistics.median),
        "total_token_ratio": _metric(ratios("total_tokens"), statistics.median),
        "output_token_ratio": _metric(ratios("output_tokens"), statistics.median),
        "ratio_direction": "right / left on pairs completed by both conditions",
    }


def summarize(rows):
    indexed = {}
    for row in rows:
        row = _require_result(row)
        label = _condition(row)
        key = (label, _case(row))
        if key in indexed:
            raise ValueError(f"duplicate result for {key}")
        indexed[key] = row
    groups = {}
    for (label, case), row in indexed.items():
        groups.setdefault(label, {})[case] = row
    labels = sorted(groups)
    comparisons = [_comparison(a, b, groups[a], groups[b]) for a, b in itertools.combinations(labels, 2)]
    return {
        "schema_version": 1, "report_scope": "pilot_or_smoke_only",
        "cgx2_language_result_measured": False, "release_conclusion": None,
        "conditions": [_condition_summary(label, list(groups[label].values())) for label in labels],
        "comparisons": comparisons,
        "notes": ["Matched pairs require the same task_id, tier, model_id, and seed.",
                  "Failures consume their full time and token budgets in all-task cost-to-success.",
                  "A six-task pilot cannot support a release conclusion."],
    }


def load_results(directory):
    directory = Path(directory)
    if not directory.is_dir():
        raise ValueError("results must be a directory")
    files = sorted(directory.glob("*.json"))
    if not files:
        raise ValueError("results directory contains no JSON files")
    rows = [json.loads(path.read_text(encoding="utf-8")) for path in files]
    summarize(rows)  # Reject duplicates and malformed rows before returning them.
    return rows


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--results", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        data = summarize(load_results(args.results))
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    except (ValueError, OSError, json.JSONDecodeError) as error:
        parser.exit(2, f"benchmark report error: {error}\n")


if __name__ == "__main__":
    main()
