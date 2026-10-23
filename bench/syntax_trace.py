"""Validate external paired syntax-pilot usage without estimating model tokens."""

import argparse
from collections import defaultdict
import json
import math
from pathlib import Path
import statistics

from syntax_packet import PACKET


REQUIRED = {"schema_version", "task_id", "variant", "model_family",
            "model_version", "seed", "usage_provenance", "input_tokens",
            "output_tokens", "tool_tokens", "diagnostic_tokens", "total_tokens",
            "elapsed_wall_seconds", "repair_iterations", "human_interventions",
            "source_bytes", "source_sha256", "verified_outcome", "verification_ref",
            "token_budget", "time_budget_seconds"}
TOKEN_FIELDS = ("input_tokens", "output_tokens", "tool_tokens", "diagnostic_tokens")


def validate_trace(row, packet=PACKET):
    packet = Path(packet)
    manifest = json.loads((packet / "packet.json").read_text())
    if not isinstance(row, dict) or set(row) != REQUIRED:
        raise ValueError("trace fields must match syntax-pilot schema exactly")
    if row["schema_version"] != 1 or row["variant"] not in ("indent", "brace"):
        raise ValueError("unsupported trace version or variant")
    for key in ("task_id", "model_family", "model_version", "verification_ref"):
        if not isinstance(row[key], str) or not row[key].strip():
            raise ValueError(f"{key} must be a nonempty string")
    task = next((t for t in manifest["tasks"] if t["id"] == row["task_id"]), None)
    if task is None:
        raise ValueError("unknown task ID")
    if type(row["seed"]) is not int or row["seed"] < 0:
        raise ValueError("seed must be nonnegative integer")
    for key in (*TOKEN_FIELDS, "total_tokens", "repair_iterations", "human_interventions", "source_bytes"):
        if type(row[key]) is not int or row[key] < 0:
            raise ValueError(f"{key} must be nonnegative integer")
    if row["total_tokens"] != sum(row[k] for k in TOKEN_FIELDS):
        raise ValueError("inconsistent disjoint token totals")
    if (row["token_budget"] != manifest["token_budget"] or
            row["time_budget_seconds"] != manifest["time_budget_seconds"] or
            row["total_tokens"] > row["token_budget"]):
        raise ValueError("budget mismatch or exceeded")
    if (type(row["elapsed_wall_seconds"]) not in (int, float) or
            not math.isfinite(row["elapsed_wall_seconds"]) or
            not 0 < row["elapsed_wall_seconds"] <= row["time_budget_seconds"]):
        raise ValueError("elapsed wall time must be positive and within budget")
    if row["verified_outcome"] not in ("pass", "fail", "timeout"):
        raise ValueError("verified outcome must be pass, fail, or timeout")
    source_hash = row["source_sha256"]
    if (not isinstance(source_hash, str) or len(source_hash) != 64 or
            any(c not in "0123456789abcdef" for c in source_hash) or
            (row["verified_outcome"] == "pass" and row["source_bytes"] == 0)):
        raise ValueError("final submitted source needs bytes and SHA-256")
    provenance = row["usage_provenance"]
    if not isinstance(provenance, dict):
        raise ValueError("usage provenance required")
    if provenance.get("kind") == "provider_usage":
        needed = {"kind", "provider", "record_id"}
    elif provenance.get("kind") == "local_tokenizer":
        needed = {"kind", "implementation", "version", "token_ids_sha256"}
        value = provenance.get("token_ids_sha256")
        if not isinstance(value, str) or len(value) != 64 or any(c not in "0123456789abcdef" for c in value):
            raise ValueError("local tokenizer needs exact token ID digest")
    else:
        raise ValueError("unknown usage provenance kind")
    if set(provenance) != needed or any(not isinstance(provenance[k], str) or not provenance[k].strip() for k in needed):
        raise ValueError("missing provider or tokenizer provenance")
    return row


def summarize(rows, packet=PACKET):
    validated = [validate_trace(r, packet) for r in rows]
    grouped = defaultdict(dict)
    charges = {"indent": {"tokens": 0, "seconds": 0, "runs": 0},
               "brace": {"tokens": 0, "seconds": 0, "runs": 0}}
    for row in validated:
        key = (row["task_id"], row["model_family"], row["model_version"], row["seed"])
        variant = row["variant"]
        if variant in grouped[key]:
            raise ValueError("duplicate task/model/seed/variant trace")
        grouped[key][variant] = row
        charge = charges[variant]
        charge["runs"] += 1
        charge["tokens"] += row["total_tokens"] if row["verified_outcome"] == "pass" else row["token_budget"]
        charge["seconds"] += row["elapsed_wall_seconds"] if row["verified_outcome"] == "pass" else row["time_budget_seconds"]
    output_ratios = []
    total_ratios = []
    paired = 0
    for pair in grouped.values():
        if set(pair) != {"indent", "brace"}:
            continue
        paired += 1
        left, right = pair["indent"], pair["brace"]
        lp, rp = left["usage_provenance"], right["usage_provenance"]
        if (lp["kind"] != rp["kind"] or
                (lp["kind"] == "provider_usage" and lp["provider"] != rp["provider"]) or
                (lp["kind"] == "local_tokenizer" and
                 (lp["implementation"], lp["version"]) !=
                 (rp["implementation"], rp["version"]))):
            raise ValueError("paired runs use different token accounting")
        if left["verified_outcome"] == right["verified_outcome"] == "pass":
            if left["output_tokens"]:
                output_ratios.append(right["output_tokens"] / left["output_tokens"])
            if left["total_tokens"]:
                total_ratios.append(right["total_tokens"] / left["total_tokens"])
    # This public packet has no independent review or authenticated external
    # run evidence. Syntactically valid JSON must not become a genuine trial.
    return {"validated_traces": len(validated), "validated_paired_traces": paired,
            "genuine_paired_traces": 0,
            "paired_successes": len(total_ratios),
            "brace_over_indent_output_token_ratio": statistics.median(output_ratios) if output_ratios else None,
            "brace_over_indent_total_token_ratio": statistics.median(total_ratios) if total_ratios else None,
            "all_task_charges": charges, "claim": "none"}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("traces", nargs="*", type=Path)
    parser.add_argument("--packet", type=Path, default=PACKET)
    args = parser.parse_args(argv)
    rows = [json.loads(path.read_text()) for path in args.traces]
    print(json.dumps(summarize(rows, args.packet), indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
