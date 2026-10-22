"""Version 3 task freeze and external LLM run importer.

The importer never runs a model or hidden test. A verified-completion assertion is
accepted only with an external hidden-test result and a matching frozen digest.
"""

import argparse
from collections import Counter, defaultdict
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import sys

from holdout_analysis import analyze_ready


LANGUAGES = {"cgx2", "python", "rust", "typescript", "ocaml"}
TIERS = {"small": (3, 10), "large": (20, 100)}
CATEGORIES = {"memory_sensitive", "package_api", "native_c", "notebook_comprehension",
              "proof_interpretation", "numerical_edge"}
SHA_FIELDS = ("prompt_sha256", "tools_sha256", "documentation_sha256",
              "dependency_lock_sha256", "cgx2_source_sha256", "compiler_sha256",
              "dependency_sha256")
PIN_FIELDS = SHA_FIELDS + ("tool_version", "hardware", "time_budget_seconds",
                         "token_budget", "randomization_seed")
RUN_FIELDS = {"schema_version", "run_id", "task_id", "language", "model_family",
              "model_version", "seed", "order", "started_at_utc", "finished_at_utc",
              "hidden_test_ref", "hidden_test_sha256", "repository_snapshot_sha256",
              "tokenizer_id", "tokenizer_version", "tokenizer_output_path",
              "tokenizer_output_sha256", "prompt_tokens", "completion_tokens",
              "tool_tokens", "total_tokens", "wall_seconds", "outcome",
              "verified_completion", "failure_cause", "final_patch_sha256",
              "raw_trace_path", "raw_trace_sha256"} | set(PIN_FIELDS)
MAX_JSON_BYTES = 32 * 1024 * 1024


def digest(data):
    return hashlib.sha256(data).hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"),
                      ensure_ascii=False).encode("utf-8")


def is_sha(value):
    return isinstance(value, str) and len(value) == 64 and all(c in "0123456789abcdef" for c in value)


def utc(value):
    if not isinstance(value, str) or not value.endswith("Z"):
        raise ValueError("timestamp must be UTC ISO-8601 with Z suffix")
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo != timezone.utc:
        raise ValueError("timestamp must be UTC")
    return parsed


def read_json(path):
    if path.is_symlink() or not path.is_file() or path.stat().st_size > MAX_JSON_BYTES:
        raise ValueError(f"missing, symlinked, or oversized JSON: {path}")
    return json.loads(path.read_text(encoding="utf-8"))


def confined_path(base, name):
    if not isinstance(name, str) or not name or Path(name).is_absolute():
        raise ValueError("path must be a nonempty relative path")
    path = base / name
    if path.is_symlink() or not path.resolve().is_relative_to(base.resolve()):
        raise ValueError("path escapes pack directory or is symlinked")
    return path


def tree_digest(path):
    if not path.is_dir() or path.is_symlink():
        raise ValueError("starter directory missing or symlinked")
    files = []
    for entry in path.rglob("*"):
        if entry.is_symlink() or entry.name == ".git":
            raise ValueError("symlink or repository history in starter")
        if entry.is_file():
            files.append(entry)
    records = [[str(file.relative_to(path)), digest(file.read_bytes())] for file in sorted(files)]
    return len(files), digest(canonical(records))


def finding(items, severity, code, where, detail):
    items.append({"severity": severity, "code": code, "where": where, "detail": detail})


def audit(pack, base, run_paths=()):
    """Validate definitions and genuine paired observations before analysis."""
    base = Path(base).resolve()
    findings = []
    validated = []
    if (not isinstance(pack, dict) or pack.get("schema_version") != 3
            or pack.get("kind") not in ("candidate_pack", "holdout")
            or not isinstance(pack.get("tasks"), list)
            or not isinstance(pack.get("expected_runs"), list)
            or not isinstance(pack.get("protocol"), dict)):
        finding(findings, "invalid", "pack_schema", "pack", "version 3 task pack required")
        return {"status": "insufficient_evidence", "findings": findings, "validated_runs": []}
    holdout = pack["kind"] == "holdout"
    tasks = {}
    all_workspaces = []
    all_hidden = []
    for index, task in enumerate(pack["tasks"]):
        where = f"tasks[{index}]"
        if not isinstance(task, dict):
            finding(findings, "invalid", "task_schema", where, "task must be an object")
            continue
        task_id = task.get("task_id")
        if not isinstance(task_id, str) or not task_id or task_id in tasks:
            finding(findings, "invalid", "duplicate_task_id", where, "task ID missing or repeated")
            continue
        tasks[task_id] = task
        tier = task.get("tier")
        applicable = task.get("applicable_languages")
        if (not isinstance(tier, str) or tier not in TIERS
                or not isinstance(task.get("category"), str) or task["category"] not in CATEGORIES
                or task.get("status") not in ("candidate", "frozen")
                or not isinstance(task.get("acceptance_criteria"), list)
                or not task["acceptance_criteria"]
                or any(not isinstance(c, str) or not c.strip() for c in task["acceptance_criteria"])
                or not isinstance(applicable, list)
                or any(not isinstance(language, str) for language in applicable)
                or len(applicable) != len(set(applicable))
                or not isinstance(task.get("baseline_language"), str)
                or not {"cgx2", task.get("baseline_language")} <= set(applicable)
                or not set(applicable) <= LANGUAGES
                or not isinstance(task.get("baseline_rationale"), str)
                or not task["baseline_rationale"].strip()
                or not isinstance(task.get("hidden_test_ref"), str)
                or not task["hidden_test_ref"].strip()
                or not isinstance(task.get("exclusions"), dict)
                or set(task["exclusions"]) != LANGUAGES - set(applicable)
                or any(not isinstance(reason, str) or not reason.strip() for reason in task["exclusions"].values())
                or ("planned_repository_files" in task and
                    (type(task["planned_repository_files"]) is not int or
                     not TIERS[tier][0] <= task["planned_repository_files"] <= TIERS[tier][1]))):
            finding(findings, "invalid", "task_schema", where, "category, tier, criteria, baseline, applicability, and opaque hidden reference required")
            continue
        if task["status"] != "frozen":
            finding(findings, "missing", "independent_review", task_id, "candidate is not a reviewed holdout task")
            continue
        try:
            reviewers = task["reviewer_ids"]
            if (not isinstance(reviewers, list) or any(not isinstance(r, str) or not r for r in reviewers)
                    or len(set(reviewers)) < 2):
                raise ValueError("two distinct independent reviewer IDs required")
            if utc(task["reviewed_at_utc"]) > utc(pack["freeze"]["frozen_at_utc"]):
                raise ValueError("review occurred after freeze")
            hidden = confined_path(base, task["hidden_test_path"])
            if not hidden.is_file() or digest(hidden.read_bytes()) != task["hidden_test_sha256"]:
                raise ValueError("hidden acceptance bytes differ frozen digest")
            snapshots = task["repository_snapshots"]
            workspaces = task["workspace_paths"]
            if set(snapshots) != set(applicable) or set(workspaces) != set(applicable):
                raise ValueError("each applicable language needs a frozen repository snapshot")
            resolved = []
            for language in applicable:
                workspace = confined_path(base, workspaces[language])
                count, actual = tree_digest(workspace)
                if not TIERS[tier][0] <= count <= TIERS[tier][1]:
                    raise ValueError(f"{language} starter has {count} files outside {tier} tier")
                if not is_sha(snapshots[language]) or actual != snapshots[language]:
                    raise ValueError(f"{language} repository snapshot digest differs")
                if hidden.resolve().is_relative_to(workspace.resolve()):
                    raise ValueError("hidden acceptance test leaks into model workspace")
                resolved.append(workspace.resolve())
            if len(set(resolved)) != len(resolved):
                raise ValueError("languages share a model workspace")
            all_workspaces.extend(resolved)
            all_hidden.append(hidden.resolve())
        except (KeyError, TypeError, ValueError, OSError) as error:
            finding(findings, "invalid", "task_freeze", task_id, str(error))
    if any(hidden.is_relative_to(workspace) for hidden in all_hidden for workspace in all_workspaces):
        finding(findings, "invalid", "hidden_leak", "pack", "hidden acceptance test lies in a model workspace")
    if len(set(all_workspaces)) != len(all_workspaces):
        finding(findings, "invalid", "shared_workspace", "pack", "tasks or languages share a model workspace")
    if not holdout:
        if any(task.get("status") == "frozen" for task in tasks.values()):
            finding(findings, "invalid", "candidate_frozen", "pack", "frozen tasks require holdout pack")
        unpinned = [key for key in SHA_FIELDS + ("tool_version", "hardware")
                    if not pack["protocol"].get(key)]
        if unpinned:
            finding(findings, "missing", "protocol_not_frozen", "pack", f"unassigned pins: {', '.join(unpinned)}")
    else:
        if not 60 <= len(tasks) <= 100 or {t.get("tier") for t in tasks.values()} != set(TIERS):
            finding(findings, "missing", "holdout_size", "pack", "60–100 tasks across both tiers required")
        if not CATEGORIES <= {t.get("category") for t in tasks.values()}:
            finding(findings, "missing", "category_coverage", "pack", "all six categories required")
        if any(t.get("status") != "frozen" for t in tasks.values()):
            finding(findings, "missing", "unfrozen_task", "pack", "all holdout tasks must be reviewed and frozen")
        try:
            freeze = pack["freeze"]
            utc(freeze["frozen_at_utc"])
            if digest(canonical(pack["tasks"])) != freeze["task_set_sha256"]:
                raise ValueError("task definitions changed after freeze")
            if digest(canonical(pack["expected_runs"])) != freeze["run_grid_sha256"]:
                raise ValueError("preregistered run grid changed after freeze")
            protocol = pack["protocol"]
            if digest(canonical(protocol)) != freeze["protocol_sha256"]:
                raise ValueError("protocol pins changed after freeze")
            if any(not is_sha(protocol.get(key)) for key in SHA_FIELDS):
                raise ValueError("exact prompt/tool/docs/lock/CGx2/compiler/dependency hashes required")
            if not isinstance(protocol.get("tool_version"), str) or not protocol["tool_version"]:
                raise ValueError("tool version required")
            if not isinstance(protocol.get("hardware"), str) or not protocol["hardware"]:
                raise ValueError("hardware pin required")
            if (type(protocol.get("time_budget_seconds")) not in (int, float)
                    or protocol["time_budget_seconds"] <= 0
                    or type(protocol.get("token_budget")) is not int or protocol["token_budget"] <= 0
                    or type(protocol.get("randomization_seed")) is not int):
                raise ValueError("time, token, and randomization budgets required")
        except (KeyError, TypeError, ValueError) as error:
            finding(findings, "invalid", "freeze", "pack", str(error))
    expected = {}
    pair_grid = defaultdict(set)
    condition_keys = set()
    orders = defaultdict(set)
    model_versions = defaultdict(set)
    for index, entry in enumerate(pack["expected_runs"]):
        where = f"expected_runs[{index}]"
        if (not isinstance(entry, dict)
                or set(entry) != {"run_id", "task_id", "language", "model_family", "model_version", "seed", "order"}
                or any(not isinstance(entry[k], str) for k in
                       ("run_id", "task_id", "language", "model_family", "model_version"))
                or type(entry["seed"]) is not int or type(entry["order"]) is not int):
            finding(findings, "invalid", "expected_run_schema", where, "exact preregistered run identity required")
            continue
        run_id = entry["run_id"]
        key = (entry["task_id"], entry["model_family"], entry["model_version"], entry["seed"])
        if (not isinstance(run_id, str) or not run_id or run_id in expected
                or entry["task_id"] not in tasks
                or entry["language"] not in tasks[entry["task_id"]].get("applicable_languages", [])
                or entry["seed"] < 0 or entry["order"] < 0
                or not entry["model_family"] or not entry["model_version"]):
            finding(findings, "invalid", "duplicate_or_bad_run", where, "invalid or duplicate preregistered run")
            continue
        expected[run_id] = entry
        if (*key, entry["language"]) in condition_keys:
            finding(findings, "invalid", "duplicate_condition", where, "task/model/seed/language repeated")
        condition_keys.add((*key, entry["language"]))
        if entry["order"] in orders[key]:
            finding(findings, "invalid", "duplicate_order", where, "paired condition order repeated")
        orders[key].add(entry["order"])
        model_versions[entry["model_family"]].add(entry["model_version"])
        pair_grid[key].add(entry["language"])
    if holdout and any(len(versions) != 1 for versions in model_versions.values()):
        finding(findings, "invalid", "mixed_model_version", "pack", "one frozen version per model family required")
    supplied = {}
    for path in run_paths:
        try:
            path = Path(path)
            run = read_json(path)
            run_id = run.get("run_id")
            if run_id in supplied:
                raise ValueError("duplicate supplied run ID")
            supplied[run_id] = (run, path)
        except (OSError, ValueError, AttributeError, json.JSONDecodeError) as error:
            finding(findings, "invalid", "run_file", str(path), str(error))
    for run_id in sorted(set(supplied) - set(expected)):
        finding(findings, "invalid", "unregistered_run", run_id, "run was not preregistered")
    for run_id, entry in expected.items():
        if run_id not in supplied:
            finding(findings, "missing", "missing_run", run_id, "registered raw run absent")
            continue
        run, path = supplied[run_id]
        try:
            task = tasks[entry["task_id"]]
            if set(run) != RUN_FIELDS or run["schema_version"] != 3:
                raise ValueError("exact version 3 run fields required")
            if any(run[k] != entry[k] for k in entry):
                raise ValueError("run identity differs preregistration")
            if any(run[k] != pack["protocol"][k] for k in PIN_FIELDS):
                raise ValueError("run differs frozen protocol pin or budget")
            if (run["hidden_test_ref"] != task["hidden_test_ref"]
                    or run["hidden_test_sha256"] != task["hidden_test_sha256"]
                    or run["repository_snapshot_sha256"] != task["repository_snapshots"][run["language"]]):
                raise ValueError("run differs task/acceptance snapshot")
            started, finished = utc(run["started_at_utc"]), utc(run["finished_at_utc"])
            if started < utc(pack["freeze"]["frozen_at_utc"]) or finished < started:
                raise ValueError("run chronology precedes freeze or reverses time")
            workspaces = all_workspaces
            tokenizer_path = confined_path(base, run["tokenizer_output_path"])
            raw_path = confined_path(base, run["raw_trace_path"])
            if any(p.resolve().is_relative_to(w) for p in (path, tokenizer_path, raw_path) for w in workspaces):
                raise ValueError("raw run or tokenizer evidence leaks into model workspace")
            tokenizer = read_json(tokenizer_path)
            if digest(tokenizer_path.read_bytes()) != run["tokenizer_output_sha256"]:
                raise ValueError("tokenizer output hash mismatch")
            if (set(tokenizer) != {"tokenizer_id", "tokenizer_version", "input_token_ids", "output_token_ids", "tool_token_ids"}
                    or tokenizer["tokenizer_id"] != run["tokenizer_id"]
                    or tokenizer["tokenizer_version"] != run["tokenizer_version"]
                    or not all(isinstance(tokenizer[k], list) and all(type(v) is int and v >= 0 for v in tokenizer[k])
                               for k in ("input_token_ids", "output_token_ids", "tool_token_ids"))):
                raise ValueError("actual tokenizer ID arrays and version required")
            counts = [len(tokenizer[k]) for k in ("input_token_ids", "output_token_ids", "tool_token_ids")]
            if counts != [run["prompt_tokens"], run["completion_tokens"], run["tool_tokens"]] or sum(counts) != run["total_tokens"]:
                raise ValueError("tokenizer counts or total tokens mismatch")
            raw = read_json(raw_path)
            if digest(raw_path.read_bytes()) != run["raw_trace_sha256"]:
                raise ValueError("raw trace hash mismatch")
            verification = raw.get("verification") if isinstance(raw, dict) else None
            if (not isinstance(raw, dict) or raw.get("run_id") != run_id or not isinstance(raw.get("events"), list)
                    or not isinstance(verification, dict)
                    or verification.get("verifier_id") in (None, "")
                    or verification.get("hidden_test_ref") != run["hidden_test_ref"]
                    or verification.get("hidden_test_sha256") != run["hidden_test_sha256"]
                    or verification.get("result") != run["outcome"]
                    or verification.get("final_patch_sha256") != run["final_patch_sha256"]):
                raise ValueError("raw trace needs matching external hidden-test verification")
            if (not is_sha(run["final_patch_sha256"]) or run["total_tokens"] > run["token_budget"]
                    or type(run["wall_seconds"]) not in (int, float) or not 0 <= run["wall_seconds"] <= run["time_budget_seconds"]):
                raise ValueError("patch hash, token, or wall budget invalid")
            if run["outcome"] not in ("pass", "fail", "timeout") or run["verified_completion"] != (run["outcome"] == "pass"):
                raise ValueError("verified completion must match hidden-test outcome")
            if (run["outcome"] == "pass" and (run["failure_cause"] is not None or run["wall_seconds"] == 0 or run["total_tokens"] == 0)):
                raise ValueError("successful run needs measured time/tokens and no failure cause")
            if run["outcome"] != "pass" and (not isinstance(run["failure_cause"], str) or not run["failure_cause"].strip()):
                raise ValueError("failed run needs explicit cause")
            validated.append(run)
        except (KeyError, TypeError, ValueError, OSError, json.JSONDecodeError) as error:
            finding(findings, "invalid", "run_provenance", run_id, str(error))
    if holdout:
        grid = {(key[1], key[2], key[3]) for key in pair_grid}
        for task_id, task in tasks.items():
            for family, version, seed in grid:
                present = pair_grid.get((task_id, family, version, seed), set())
                if present != set(task.get("applicable_languages", [])):
                    finding(findings, "missing", "unpaired_run", task_id, f"{family}/{version}/{seed}: {sorted(present)}")
        families = {key[1] for key in pair_grid}
        if len(families) < 2 or any(len({key[3] for key in pair_grid if key[1] == family}) < 2 for family in families):
            finding(findings, "missing", "model_coverage", "pack", "two model families and repeated paired seeds required")
        if not expected:
            finding(findings, "missing", "no_runs", "pack", "no genuine paired runs registered")
        by_pair = defaultdict(list)
        for run in validated:
            by_pair[(run["task_id"], run["model_family"], run["model_version"], run["seed"])].append(run)
        for key, pair in by_pair.items():
            ordered = sorted(pair, key=lambda run: run["order"])
            if any(utc(left["started_at_utc"]) > utc(right["started_at_utc"])
                   for left, right in zip(ordered, ordered[1:])):
                finding(findings, "invalid", "run_order", str(key), "wall chronology contradicts preregistered order")
        tokenizer_versions = defaultdict(set)
        for run in validated:
            tokenizer_versions[run["model_family"]].add((run["tokenizer_id"], run["tokenizer_version"]))
        if any(len(versions) != 1 for versions in tokenizer_versions.values()):
            finding(findings, "invalid", "mixed_tokenizer", "runs", "one frozen tokenizer per model family required")
    elif not expected:
        finding(findings, "missing", "no_runs", "pack", "no model runs preregistered; candidate pack is planning only")
    counts = Counter((task.get("tier"), task.get("status")) for task in tasks.values())
    exclusions = Counter(language for task in tasks.values() for language in task.get("exclusions", {}))
    return {"schema_version": 3, "status": "ready" if not findings and holdout else "insufficient_evidence",
            "task_count": len(tasks), "candidate_count": sum(t.get("status") == "candidate" for t in tasks.values()),
            "tier_status_counts": {f"{tier}/{status}": n for (tier, status), n in sorted(counts.items())},
            "category_counts": dict(sorted(Counter(t.get("category") for t in tasks.values()).items())),
            "exclusions_by_language": dict(sorted(exclusions.items())),
            "expected_run_count": len(expected), "validated_run_count": len(validated),
            "findings": findings, "validated_runs": validated}


def report(pack, base, run_paths=()):
    result = audit(pack, base, run_paths)
    runs = result.pop("validated_runs")
    if result["status"] != "ready":
        return {"schema_version": 3, "status": "insufficient_evidence", "summary": result,
                "comparison": None, "release_verdict": "insufficient_evidence"}
    matrix = {"kind": "holdout", "conditions": sorted(LANGUAGES), "tasks": pack["tasks"]}
    analysis = analyze_ready(matrix, [{**run, "hidden_test_result": run["outcome"]} for run in runs])
    return {"schema_version": 3, "status": "ready", "summary": result,
            "comparison": analysis, "release_verdict": analysis["release_verdict"]}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("validate", "summarize", "report"))
    parser.add_argument("--pack", required=True, type=Path)
    parser.add_argument("--run", action="append", type=Path, default=[])
    parser.add_argument("--out", type=Path)
    args = parser.parse_args(argv)
    try:
        pack = read_json(args.pack)
        result = report(pack, args.pack.parent, args.run) if args.command == "report" else audit(pack, args.pack.parent, args.run)
        result.pop("validated_runs", None)
    except (OSError, ValueError, TypeError, KeyError, json.JSONDecodeError) as error:
        result = {"schema_version": 3, "status": "insufficient_evidence", "findings": [
            {"severity": "invalid", "code": "input", "where": "CLI", "detail": str(error)}]}
    output = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.out:
        args.out.write_text(output, encoding="utf-8")
    else:
        sys.stdout.write(output)
    return 0 if result["status"] == "ready" else 1


if __name__ == "__main__":
    raise SystemExit(main())
