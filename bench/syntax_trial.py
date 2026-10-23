"""Fail-closed intake for exact public-context syntax-pilot trial traces.

Validation checks consistency and leaks. A JSON receipt alone cannot prove a
provider call or an isolation claim; an independent operator must authenticate
those records before a trace can be labelled genuine.
"""

import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import statistics
import tempfile

from syntax_export import _strings, export as create_export, verify_export
from syntax_packet import PACKET


PROTOCOL = PACKET / "trial-protocol.json"
TRACE_FIELDS = {
    "schema_version", "task_id", "variant", "model_family", "model_version",
    "model_settings", "seed", "order_position", "export_manifest_sha256",
    "visible_files", "transcript", "provider_usage_records", "output_tokens",
    "input_tokens", "elapsed_wall_seconds", "repair_iterations",
    "human_interventions", "final_source", "final_source_sha256",
    "compiler_success", "test_success", "verification_ref", "isolation",
    "operator_authentication", "token_budget", "time_budget_seconds",
}
ISOLATION_FIELDS = {
    "status", "cgroup_cpu", "cgroup_memory", "cgroup_pids",
    "private_unmounted", "network_disabled", "grader_separate_process",
}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def public_context(export_dir):
    """Return the exact first model message for a public trial export."""
    export_dir = Path(export_dir)
    manifest = verify_export(export_dir)
    names = sorted(manifest["files_sha256"])
    files = {name: (export_dir / name).read_text() for name in names}
    files["export-manifest.json"] = (export_dir / "export-manifest.json").read_text()
    return json.dumps({
        "schema_version": 1,
        "export_manifest_sha256": sha((export_dir / "export-manifest.json").read_bytes()),
        "files": files,
    }, sort_keys=True, separators=(",", ":"))


def _private_bytes(packet):
    root = Path(packet) / "private"
    if root.is_symlink():
        raise ValueError("private tree is symlinked")
    for path in root.rglob("*"):
        if path.is_symlink():
            raise ValueError("private tree contains symlink")
        if path.is_file():
            yield path.read_bytes()


def validate_trace(row, export_dir, packet=PACKET, protocol=PROTOCOL):
    if not isinstance(row, dict) or set(row) != TRACE_FIELDS:
        raise ValueError("trial trace fields do not match v2 contract")
    if row["schema_version"] != 2:
        raise ValueError("unsupported trial trace version")
    protocol = json.loads(Path(protocol).read_text()) if not isinstance(protocol, dict) else protocol
    packet_manifest = json.loads((Path(packet) / "packet.json").read_text())
    export_dir = Path(export_dir)
    exported = verify_export(export_dir)
    if exported["packet_sha256"] != protocol["packet_sha256"]:
        raise ValueError("packet/protocol hash mismatch")
    if (row["task_id"], row["variant"]) != (exported["task_id"], exported["variant"]):
        raise ValueError("trace/export task mismatch")
    with tempfile.TemporaryDirectory() as temporary:
        expected = create_export(row["task_id"], row["variant"],
                                 Path(temporary) / "expected", packet=packet)
    if exported != expected:
        raise ValueError("export differs from frozen public packet")
    order = next((o for o in protocol["order"] if o["task_id"] == row["task_id"]), None)
    if order is None or row["order_position"] not in (1, 2):
        raise ValueError("unregistered task or order")
    if row["variant"] != order["first" if row["order_position"] == 1 else "second"]:
        raise ValueError("trace violates balanced order")
    if row["export_manifest_sha256"] != sha((export_dir / "export-manifest.json").read_bytes()):
        raise ValueError("export manifest digest mismatch")
    visible = row["visible_files"]
    expected_visible = set(exported["files_sha256"]) | {"export-manifest.json"}
    if not isinstance(visible, dict) or set(visible) != expected_visible:
        raise ValueError("visible file set differs from public export")
    for name, content in visible.items():
        if not isinstance(content, str) or content.encode() != (export_dir / name).read_bytes():
            raise ValueError(f"visible file differs from public export: {name}")
    transcript = row["transcript"]
    if not isinstance(transcript, list) or not transcript:
        raise ValueError("exact model transcript required")
    for message in transcript:
        if (not isinstance(message, dict) or set(message) != {"role", "content"}
                or message["role"] not in ("system", "user", "assistant", "tool")
                or not isinstance(message["content"], str)):
            raise ValueError("invalid model transcript message")
    if transcript[0] != {"role": "user", "content": public_context(export_dir)}:
        raise ValueError("transcript must begin with exact public export context")
    # Assistant answers may independently equal a reference. Inspect only
    # material supplied to the model, including tool output and repair prompts.
    supplied = [message for message in transcript if message["role"] != "assistant"]
    supplied_texts = list(visible.values()) + [message["content"] for message in supplied]
    for content in list(supplied_texts):
        try:
            supplied_texts.extend(_strings(json.loads(content)))
        except json.JSONDecodeError:
            pass
    context = b"\n".join(content.encode() for content in supplied_texts)
    if b"private/reference" in context or b"private/oracle" in context:
        raise ValueError("private path in visible context")
    if any(secret and secret in context for secret in _private_bytes(packet)):
        raise ValueError("private reference or oracle in visible context")
    for field in ("model_family", "model_version", "verification_ref", "final_source"):
        if not isinstance(row[field], str) or not row[field]:
            raise ValueError(f"{field} required")
    settings = row["model_settings"]
    if (not isinstance(settings, dict) or set(settings) !=
            {"temperature", "top_p", "max_output_tokens"}):
        raise ValueError("complete model settings required")
    if (type(settings["temperature"]) not in (int, float)
            or type(settings["top_p"]) not in (int, float)
            or not math.isfinite(settings["temperature"])
            or not math.isfinite(settings["top_p"])
            or settings["temperature"] < 0 or not 0 < settings["top_p"] <= 1
            or type(settings["max_output_tokens"]) is not int
            or settings["max_output_tokens"] < 1):
        raise ValueError("invalid model settings")
    for field in ("seed", "output_tokens", "input_tokens", "repair_iterations", "human_interventions"):
        if type(row[field]) is not int or row[field] < 0:
            raise ValueError(f"invalid {field}")
    if row["human_interventions"] != 0:
        raise ValueError("human intervention invalidates blinded trial")
    if (type(row["elapsed_wall_seconds"]) not in (int, float)
            or not math.isfinite(row["elapsed_wall_seconds"])
            or row["elapsed_wall_seconds"] <= 0):
        raise ValueError("invalid wall time")
    if (row["token_budget"] != packet_manifest["token_budget"]
            or row["time_budget_seconds"] != packet_manifest["time_budget_seconds"]
            or row["input_tokens"] + row["output_tokens"] > row["token_budget"]
            or row["elapsed_wall_seconds"] > row["time_budget_seconds"]):
        raise ValueError("budget mismatch or overrun")
    records = row["provider_usage_records"]
    if not isinstance(records, list) or not records:
        raise ValueError("raw provider usage records required")
    seen = set()
    for record in records:
        if (not isinstance(record, dict) or set(record) !=
                {"provider", "response_id", "input_tokens", "output_tokens", "raw_response"}):
            raise ValueError("incomplete provider usage record")
        if (not isinstance(record["provider"], str) or not record["provider"]
                or not isinstance(record["response_id"], str) or not record["response_id"]
                or type(record["input_tokens"]) is not int or record["input_tokens"] < 0
                or type(record["output_tokens"]) is not int or record["output_tokens"] < 0):
            raise ValueError("invalid provider usage values")
        if record["response_id"] in seen:
            raise ValueError("duplicate provider response ID")
        if not isinstance(record["raw_response"], str):
            raise ValueError("raw provider response required")
        try:
            raw = json.loads(record["raw_response"])
        except json.JSONDecodeError as error:
            raise ValueError("invalid raw provider response") from error
        if (not isinstance(raw, dict) or raw.get("id") != record["response_id"]
                or not isinstance(raw.get("usage"), dict)
                or raw["usage"].get("input_tokens") != record["input_tokens"]
                or raw["usage"].get("output_tokens") != record["output_tokens"]):
            raise ValueError("reported usage disagrees with raw provider response")
        seen.add(record["response_id"])
    if (row["input_tokens"] != sum(r["input_tokens"] for r in records)
            or row["output_tokens"] != sum(r["output_tokens"] for r in records)
            or row["output_tokens"] == 0):
        raise ValueError("provider usage totals missing or inconsistent")
    if row["final_source_sha256"] != sha(row["final_source"].encode()):
        raise ValueError("final source SHA-256 mismatch")
    if type(row["compiler_success"]) is not bool or type(row["test_success"]) is not bool:
        raise ValueError("compiler/test success must be boolean")
    isolation = row["isolation"]
    if (not isinstance(isolation, dict) or set(isolation) != ISOLATION_FIELDS
            or isolation["status"] != "pass"
            or any(isolation[k] is not True for k in ISOLATION_FIELDS - {"status"})):
        raise ValueError("failed or unsupported trial isolation")
    auth = row["operator_authentication"]
    if (not isinstance(auth, dict) or set(auth) !=
            {"provider_receipts_verified", "isolation_receipt_verified", "grader_ref_verified"}
            or any(value is not True for value in auth.values())):
        raise ValueError("independent receipt authentication required")
    return row


def paired_report(rows, exports, protocol=PROTOCOL, packet=PACKET):
    """Validate and summarize a complete grid of operator-attested traces."""
    protocol = json.loads(Path(protocol).read_text()) if not isinstance(protocol, dict) else protocol
    if protocol["status"] != "frozen" or not protocol.get("registered_model"):
        raise ValueError("model/settings protocol not frozen before trials")
    expected = {item["task_id"] for item in protocol["order"]}
    expected_exports = {(task_id, variant) for task_id in expected
                        for variant in ("indent", "brace")}
    if not isinstance(exports, dict) or set(exports) != expected_exports:
        raise ValueError("one export per registered task and variant required")
    by_task = {}
    model = protocol["registered_model"]
    response_ids = set()
    export_hashes = set()
    for row in rows:
        if not isinstance(row, dict):
            raise ValueError("trial row must be an object")
        key = (row.get("task_id"), row.get("variant"))
        if key not in expected_exports:
            raise ValueError("unknown trial task or variant")
        validate_trace(row, exports[key], packet=packet, protocol=protocol)
        if row["export_manifest_sha256"] in export_hashes:
            raise ValueError("reused public export in paired report")
        export_hashes.add(row["export_manifest_sha256"])
        for record in row["provider_usage_records"]:
            if record["response_id"] in response_ids:
                raise ValueError("reused provider response ID in paired report")
            response_ids.add(record["response_id"])
        if row["task_id"] not in expected or row["variant"] in by_task.get(row["task_id"], {}):
            raise ValueError("unknown or duplicate trial")
        if any(row[key] != model[key] for key in ("model_family", "model_version", "model_settings", "seed")):
            raise ValueError("model, settings, or seed differs from registration")
        by_task.setdefault(row["task_id"], {})[row["variant"]] = row
    if set(by_task) != expected or any(set(pair) != {"indent", "brace"} for pair in by_task.values()):
        raise ValueError("incomplete six-pair grid")
    differences = []
    repair_differences = []
    output_ratios = []
    total_ratios = []
    wall_ratios = []
    charges = {variant: {"tokens": 0, "seconds": 0, "successes": 0}
               for variant in ("indent", "brace")}
    for task_id in sorted(expected):
        pair = by_task[task_id]
        providers = {record["provider"] for row in pair.values()
                     for record in row["provider_usage_records"]}
        if len(providers) != 1:
            raise ValueError("paired runs use different providers")
        passed = {}
        for variant, row in pair.items():
            success = row["compiler_success"] and row["test_success"]
            passed[variant] = success
            charge = charges[variant]
            charge["successes"] += int(success)
            charge["tokens"] += row["input_tokens"] + row["output_tokens"] if success else row["token_budget"]
            charge["seconds"] += row["elapsed_wall_seconds"] if success else row["time_budget_seconds"]
        differences.append(int(passed["indent"]) - int(passed["brace"]))
        repair_differences.append(pair["indent"]["repair_iterations"] -
                                  pair["brace"]["repair_iterations"])
        if passed["indent"] and passed["brace"]:
            if pair["brace"]["output_tokens"]:
                output_ratios.append(pair["indent"]["output_tokens"] / pair["brace"]["output_tokens"])
            brace_total = pair["brace"]["input_tokens"] + pair["brace"]["output_tokens"]
            if brace_total:
                total_ratios.append((pair["indent"]["input_tokens"] + pair["indent"]["output_tokens"]) / brace_total)
            wall_ratios.append(pair["indent"]["elapsed_wall_seconds"] /
                               pair["brace"]["elapsed_wall_seconds"])

    def interval(values, statistic):
        if not values:
            return None
        rng = random.Random(18)
        samples = sorted(statistic([rng.choice(values) for _ in values]) for _ in range(4096))
        return [samples[102], samples[3993]]

    for charge in charges.values():
        charge["tokens_per_success"] = (charge["tokens"] / charge["successes"]
                                        if charge["successes"] else None)
        charge["seconds_per_success"] = (charge["seconds"] / charge["successes"]
                                         if charge["successes"] else None)
    return {
        "scope": "six_public_scalar_task_pairs_only",
        "validated_paired_traces": len(expected),
        "authenticity": "operator_attested; external receipts require independent verification",
        "completion_difference_indent_minus_brace": statistics.mean(differences),
        "completion_difference_95pct_task_bootstrap": interval(differences, statistics.mean),
        "mean_repair_difference_indent_minus_brace": statistics.mean(repair_differences),
        "repair_difference_95pct_task_bootstrap": interval(repair_differences, statistics.mean),
        "paired_successes": len(output_ratios),
        "indent_over_brace_output_token_ratio": statistics.median(output_ratios) if output_ratios else None,
        "output_ratio_95pct_task_bootstrap": interval(output_ratios, statistics.median),
        "indent_over_brace_total_token_ratio": statistics.median(total_ratios) if total_ratios else None,
        "total_ratio_95pct_task_bootstrap": interval(total_ratios, statistics.median),
        "indent_over_brace_wall_time_ratio": statistics.median(wall_ratios) if wall_ratios else None,
        "wall_ratio_95pct_task_bootstrap": interval(wall_ratios, statistics.median),
        "all_task_charges": charges,
        "improvement_claim": "none; six public tasks cannot establish a release claim",
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--trace", type=Path, required=True)
    parser.add_argument("--export", type=Path, required=True)
    args = parser.parse_args(argv)
    trace = validate_trace(json.loads(args.trace.read_text()), args.export)
    print(json.dumps({"status": "structurally_valid_not_proof_of_authenticity",
                      "task_id": trace["task_id"], "variant": trace["variant"]}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
