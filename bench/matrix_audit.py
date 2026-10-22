"""Audit versioned LLM task matrices without running tasks or inventing traces."""

import argparse
import hashlib
import json
import os
import sys
from pathlib import Path

import runner


TRACE_FIELDS = ("output_tokens", "total_tokens", "iterations", "human_interventions")
PIN_FIELDS = ("compiler", "dependencies", "tools", "prompt", "hardware", "threads")
RESULT_REQUIRED = set(json.loads((Path(__file__).parent / "schema/result.schema.json").read_text())["required"])
EXCLUDED_DIRS = {".git", ".hg", ".svn", "target", "__pycache__", "node_modules", ".venv", "venv", ".pytest_cache"}
CHECKLIST = (
    "Independently review language-neutral task equivalence and acceptable alternative solutions.",
    "Freeze 60–100 fresh tasks, hidden tests, prompts, tools, locks, model families, versions, seeds, budgets, and task order before opening holdout.",
    "Use at least two model families and repeated paired seeds; select the strongest applicable baseline per category in advance.",
    "Keep acceptance tests and gold changes outside model workspaces and strip future repository history.",
    "Retain every run, including failures charged full wall and token budgets; review ambiguous outcomes without exposing gold changes.",
    "Publish definitions after holdout retirement and report uncertainty and failures separately from pilot evidence.",
)


def inside(path, parent):
    return path == parent or parent in path.parents


def issue(findings, severity, code, where, detail):
    findings.append({"severity": severity, "code": code, "where": where, "detail": detail})


def starter_files(workspace):
    files = []
    for base, dirs, names in os.walk(workspace, followlinks=False):
        if any((Path(base) / d).is_symlink() for d in dirs):
            raise ValueError(f"starter symlink directory is not auditable: {base}")
        dirs[:] = sorted(d for d in dirs if d not in EXCLUDED_DIRS)
        for name in sorted(names):
            path = Path(base) / name
            if path.is_symlink():
                raise ValueError(f"starter symlink is not auditable: {path}")
            if path.is_file():
                files.append(path)
    return files


def read_json(path):
    raw = path.read_bytes()
    return json.loads(raw), hashlib.sha256(raw).hexdigest()


def audit(matrix, base):
    base = Path(base).resolve()
    findings = []
    if not isinstance(matrix, dict) or matrix.get("schema_version") != 1 or matrix.get("kind") not in ("public_calibration", "holdout"):
        return {"schema_version": 1, "status": "invalid", "findings": [{"severity": "invalid", "code": "matrix_schema",
                "where": "matrix", "detail": "requires schema_version 1 and kind public_calibration or holdout"}],
                "checked_entries": 0, "checklist": CHECKLIST}
    conditions = matrix.get("conditions")
    entries = matrix.get("entries")
    if not isinstance(conditions, list) or len(conditions) < 2 or not isinstance(entries, list):
        issue(findings, "invalid", "matrix_schema", "matrix", "conditions need at least two variants and entries must be an array")
        return {"schema_version": 1, "status": "invalid", "findings": findings, "checked_entries": 0, "checklist": CHECKLIST}
    expected = set()
    for condition in conditions:
        if not isinstance(condition, dict) or set(condition) != {"language", "variant"} or not all(
                isinstance(condition[k], str) and condition[k] for k in ("language", "variant")):
            issue(findings, "invalid", "condition_schema", "matrix", "condition needs language and variant")
            continue
        key = (condition["language"], condition["variant"])
        if key in expected:
            issue(findings, "invalid", "duplicate_condition", "matrix", str(key))
        expected.add(key)
    if matrix["kind"] == "holdout":
        freeze = matrix.get("freeze")
        required = ("task_set_sha256", "prompts_sha256", "tools_sha256", "locks_sha256", "acceptance_sha256",
                    "order_seed", "frozen_at_utc", "reviewer_ids")
        if (not isinstance(freeze, dict) or any(k not in freeze for k in required)
                or any(not isinstance(freeze.get(k), str) or not freeze[k] for k in required
                       if k not in ("order_seed", "reviewer_ids"))
                or type(freeze.get("order_seed")) is not int
                or not isinstance(freeze.get("reviewer_ids"), list) or not freeze["reviewer_ids"]):
            issue(findings, "incomplete", "holdout_freeze", "matrix", "missing preregistered freeze fields")
    keys = set()
    groups = {}
    group_settings = {}
    checked = 0
    for index, entry in enumerate(entries):
        where = f"entries[{index}]"
        if not isinstance(entry, dict) or not all(isinstance(entry.get(k), str) and entry[k] for k in
                                                 ("manifest", "result", "workspace", "hidden_tests", "gold_changes")):
            issue(findings, "invalid", "entry_schema", where, "manifest, result, workspace, hidden_tests, gold_changes paths required")
            continue
        checked += 1
        manifest_path = (base / entry["manifest"]).resolve()
        result_path = (base / entry["result"]).resolve()
        workspace = (base / entry["workspace"]).resolve()
        hidden = (base / entry["hidden_tests"]).resolve()
        gold = (base / entry["gold_changes"]).resolve()
        if not manifest_path.is_file():
            issue(findings, "incomplete", "missing_manifest", where, entry["manifest"])
            continue
        try:
            manifest, manifest_hash = read_json(manifest_path)
            runner._manifest(manifest)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            issue(findings, "invalid", "invalid_manifest", where, str(error))
            continue
        key = tuple(manifest[k] for k in ("task_id", "tier", "model_id", "seed", "language", "variant"))
        if key in keys:
            issue(findings, "invalid", "duplicate_key", where, str(key))
        keys.add(key)
        group = key[:4]
        groups.setdefault(group, set()).add(key[4:])
        settings = (manifest["timeout_seconds"], manifest["budget"]["total_tokens"],
                    *[manifest["versions"].get(k) for k in ("tools", "prompt", "hardware", "threads")])
        if group in group_settings and group_settings[group] != settings:
            issue(findings, "invalid", "paired_settings_mismatch", where, "paired variants have different time/token budgets or prompt/tools/hardware/thread pins")
        group_settings.setdefault(group, settings)
        if key[4:] not in expected:
            issue(findings, "invalid", "unexpected_condition", where, str(key[4:]))
        if not workspace.is_dir() or (manifest_path.parent / manifest["cwd"]).resolve() != workspace:
            issue(findings, "invalid", "workspace_mismatch", where, "workspace must equal manifest cwd and exist")
            continue
        if (workspace / ".git").exists() or any((Path(root) / ".git").exists() for root, _, _ in os.walk(workspace)):
            issue(findings, "invalid", "history_leak", where, "starter workspace contains .git history")
        try:
            files = starter_files(workspace)
        except ValueError as error:
            issue(findings, "invalid", "starter_symlink", where, str(error))
            files = []
        lower, upper = (3, 10) if manifest["tier"] == "small" else (20, 100)
        if not lower <= len(files) <= upper:
            issue(findings, "invalid", "starter_file_count", where, f"{len(files)} files; {manifest['tier']} requires {lower}–{upper}")
        if not hidden.is_file() or inside(hidden, workspace) or not gold.exists() or inside(gold, workspace) or hidden == gold:
            issue(findings, "invalid", "hidden_test_leak", where, "hidden tests and gold changes must exist outside workspace")
        if not any((workspace / part).resolve() == hidden for part in manifest["hidden_test_argv"]):
            issue(findings, "invalid", "hidden_test_path", where, "hidden_test_argv must name the external hidden test path")
        markers = entry.get("forbidden_markers")
        if not isinstance(markers, list) or not markers or any(not isinstance(m, str) or not m for m in markers):
            issue(findings, "incomplete", "leak_markers_missing", where, "supply task-specific source/gold/history markers")
        else:
            for file in files:
                content = file.read_bytes()
                if any(marker.encode("utf-8") in content for marker in markers):
                    issue(findings, "invalid", "source_leak_marker", where, str(file.relative_to(workspace)))
        versions = manifest["versions"]
        if any(not versions.get(k) for k in PIN_FIELDS) or "/" not in manifest["model_id"]:
            issue(findings, "incomplete", "missing_pin", where, "pin compiler, dependencies, tools, prompt, hardware, threads and model family/version")
        if not result_path.is_file():
            issue(findings, "incomplete", "missing_result", where, entry["result"])
            continue
        try:
            result, _ = read_json(result_path)
            if not isinstance(result, dict):
                raise ValueError("result must be an object")
        except (OSError, ValueError, json.JSONDecodeError) as error:
            issue(findings, "invalid", "invalid_result", where, str(error))
            continue
        missing_result_fields = RESULT_REQUIRED - result.keys()
        if missing_result_fields:
            issue(findings, "invalid", "result_schema", where,
                  "missing result v1 fields: " + ", ".join(sorted(missing_result_fields)))
        if result.get("schema_version") != 1 or any(result.get(k) != manifest[k] for k in
                ("task_id", "tier", "language", "variant", "model_id", "seed", "timeout_seconds")) or result.get("budget_total_tokens") != manifest["budget"]["total_tokens"]:
            issue(findings, "invalid", "result_key_mismatch", where, "result identifiers/budgets differ from manifest")
        provenance = result.get("provenance")
        if not isinstance(provenance, dict) or provenance.get("manifest_sha256") != manifest_hash:
            issue(findings, "invalid", "manifest_hash_mismatch", where, "result provenance does not match exact manifest bytes")
        elif provenance.get("versions") != versions or not provenance.get("started_at_utc"):
            issue(findings, "invalid", "invalid_provenance", where, "versions or timestamp missing/mismatched")
        if any(type(result.get(k)) is not int or result[k] < 0 for k in TRACE_FIELDS):
            issue(findings, "incomplete", "missing_trace", where, "all real token/iteration/intervention fields required")
        elif result["output_tokens"] > result["total_tokens"]:
            issue(findings, "invalid", "invalid_trace", where, "output_tokens exceeds total_tokens")
        trace_name = manifest.get("trace_path")
        trace_path = (manifest_path.parent / trace_name).resolve() if trace_name else None
        if trace_path is None or not trace_path.is_file() or inside(trace_path, workspace):
            issue(findings, "incomplete", "missing_trace_source", where, "trace_path must refer to a preserved file outside workspace")
        else:
            try:
                trace, _ = read_json(trace_path)
                if not isinstance(trace, dict) or any(
                        result.get(k) is not None and trace.get(k) != result.get(k) for k in TRACE_FIELDS):
                    issue(findings, "invalid", "trace_result_mismatch", where, "result usage differs from saved trace")
            except (ValueError, OSError, json.JSONDecodeError) as error:
                issue(findings, "invalid", "invalid_trace_source", where, str(error))
        if type(result.get("verified_completion")) is not bool:
            issue(findings, "invalid", "result_completion", where, "verified_completion must be Boolean")
        elif result["verified_completion"] is False:
            if result.get("failure_reason") in (None, "") or result.get("charged_wall_seconds") != manifest["timeout_seconds"]:
                issue(findings, "invalid", "failed_budget_charge", where, "failed run needs reason and full wall budget charge")
            if result.get("budget_total_tokens") != manifest["budget"]["total_tokens"]:
                issue(findings, "invalid", "failed_budget_charge", where, "failed run needs full token budget recorded")
        elif result.get("failure_reason") is not None:
            issue(findings, "invalid", "result_completion", where, "success cannot have failure reason")
        if type(result.get("wall_seconds")) not in (int, float) or result["wall_seconds"] < 0 or type(
                result.get("charged_wall_seconds")) not in (int, float) or result["charged_wall_seconds"] < 0:
            issue(findings, "invalid", "result_wall_time", where, "wall_seconds and charged_wall_seconds must be nonnegative numbers")
    for group, present in groups.items():
        for missing in sorted(expected - present):
            issue(findings, "incomplete", "unpaired_condition", str(group), str(missing))
    if not groups:
        issue(findings, "incomplete", "empty_matrix", "matrix", "no task groups")
    if matrix["kind"] == "holdout":
        task_ids = {group[0] for group in groups}
        if not 60 <= len(task_ids) <= 100:
            issue(findings, "incomplete", "holdout_size", "matrix", "holdout needs 60–100 distinct tasks")
        families = {group[2].split("/", 1)[0] for group in groups}
        if len(families) < 2:
            issue(findings, "incomplete", "model_families", "matrix", "holdout needs at least two model families")
        if any(len({group[2].split("/", 1)[0] for group in groups if group[0] == task}) < 2
               for task in task_ids):
            issue(findings, "incomplete", "model_family_coverage", "matrix", "each holdout task needs two model families")
        task_models = {(group[0], group[2]) for group in groups}
        if any(len({group[3] for group in groups if group[0] == task and group[2] == model}) < 2
               for task, model in task_models):
            issue(findings, "incomplete", "repeated_seeds", "matrix", "holdout needs repeated paired seeds per task/model")
    status = "invalid" if any(f["severity"] == "invalid" for f in findings) else (
        "incomplete" if findings else "ready")
    return {"schema_version": 1, "kind": matrix["kind"], "status": status,
            "checked_entries": checked, "task_groups": len(groups), "conditions": [list(c) for c in sorted(expected)],
            "findings": findings, "checklist": CHECKLIST,
            "meaning": "mechanical audit only; human equivalence review and holdout validity are separate"}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--matrix", type=Path, required=True)
    parser.add_argument("--out", type=Path, help="new JSON output file; stdout if omitted")
    args = parser.parse_args(argv)
    try:
        matrix, _ = read_json(args.matrix)
        result = audit(matrix, args.matrix.resolve().parent)
        rendered = json.dumps(result, indent=2, sort_keys=True) + "\n"
        if args.out:
            if args.out.exists():
                raise FileExistsError(args.out)
            args.out.write_text(rendered)
            print(args.out)
        else:
            print(rendered, end="")
        return {"ready": 0, "incomplete": 1, "invalid": 2}[result["status"]]
    except (ValueError, OSError, json.JSONDecodeError) as error:
        parser.exit(2, f"matrix audit setup error: {error}\n")


if __name__ == "__main__":
    raise SystemExit(main())
