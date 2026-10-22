"""Version 2 paired LLM holdout audit, external-trace ingestion, and analysis.

This module makes no model calls. A ready audit is a mechanical check, not proof
that an external trace or reviewer judgment is genuine.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path

from matrix_audit import inside, starter_files
from holdout_analysis import analyze_ready, markdown


LANGUAGES = {"cgx2", "python", "rust", "typescript", "ocaml"}
TRACE_KEYS = {"schema_version", "task_id", "hidden_test_version", "language",
              "model_family", "model_version", "run_id", "seed", "order",
              "allowed_tools", "documentation_budget", "token_budget",
              "time_budget_seconds", "prompt_tokens", "completion_tokens",
              "wall_seconds", "final_patch_sha256", "hidden_test_result",
              "failure_cause", "hardware", "toolchain"}
TRACE_EXTRA_KEYS = {"tool_tokens", "tokenizer_version"}
PROTOCOL_KEYS = {"allowed_tools", "documentation_budget", "token_budget",
                 "time_budget_seconds", "hardware", "toolchain"}
RUN_KEYS = {"task_id", "language", "model_family", "model_version", "run_id",
            "seed", "order", "trace"}
FREEZE_KEYS = {"task_set_sha256", "prompts_sha256", "tools_sha256", "locks_sha256",
               "acceptance_sha256", "order_seed", "frozen_at_utc", "reviewer_ids"}
MAX_TRACE_BYTES = 65536
MAX_MATRIX_BYTES = 2097152


def _finding(findings, severity, code, where, detail):
    findings.append({"severity": severity, "code": code, "where": where, "detail": detail})


def _nonempty(value):
    return isinstance(value, str) and bool(value.strip())


def _number(value, *, positive=False):
    return type(value) in (int, float) and (value > 0 if positive else value >= 0)


def _sha(value):
    return isinstance(value, str) and len(value) == 64 and all(c in "0123456789abcdef" for c in value)


def _path(base, name):
    if not _nonempty(name) or Path(name).is_absolute():
        raise ValueError("path must be a nonempty relative path")
    candidate = base
    for part in Path(name).parts:
        candidate = candidate / part
        if candidate.is_symlink():
            raise ValueError("symlinked paths are not auditable")
    return (base / name).resolve()


def _read_json(path, limit=MAX_TRACE_BYTES):
    if path.stat().st_size > limit:
        raise ValueError(f"JSON exceeds {limit} bytes")
    return json.loads(path.read_text(encoding="utf-8"))


def _task_index(matrix, base, findings):
    tasks = {}
    for number, task in enumerate(matrix.get("tasks", [])):
        where = f"tasks[{number}]"
        if not isinstance(task, dict) or not _nonempty(task.get("task_id")):
            _finding(findings, "invalid", "task_schema", where, "task_id required")
            continue
        task_id = task["task_id"]
        if task_id in tasks:
            _finding(findings, "invalid", "duplicate_task_id", where, task_id)
            continue
        task = dict(task)
        tasks[task_id] = task
        if (task.get("tier") not in ("small", "large") or not _nonempty(task.get("category"))
                or not _nonempty(task.get("hidden_test_version")) or not _sha(task.get("hidden_test_sha256"))):
            _finding(findings, "invalid", "task_schema", where,
                     "tier, category, hidden_test_version, and hidden_test_sha256 required")
            continue
        applicable = task.get("applicable_languages")
        exclusions = task.get("exclusions")
        conditions = set(matrix["conditions"])
        if (not isinstance(applicable, list) or any(not isinstance(x, str) for x in applicable)
                or len(applicable) != len(set(applicable))
                or not set(applicable) <= conditions or "cgx2" not in applicable
                or task.get("baseline_language") not in set(applicable) - {"cgx2"}
                or not isinstance(exclusions, dict) or set(exclusions) != conditions - set(applicable)
                or any(not _nonempty(reason) for reason in exclusions.values())):
            _finding(findings, "invalid", "applicability", where,
                     "partition all conditions into applicable languages and preregistered exclusions; choose an applicable baseline")
            continue
        try:
            starters = task.get("starters")
            if starters is None and matrix["kind"] == "public_calibration":
                starters = {language: task.get("starter") for language in applicable}
            if not isinstance(starters, dict) or set(starters) != set(applicable):
                raise ValueError("one starter path per applicable language is required")
            if matrix["kind"] == "holdout" and len(set(starters.values())) != len(starters):
                raise ValueError("holdout language starters must be distinct")
            statement = _path(base, task.get("statement"))
            hidden = _path(base, task.get("hidden_tests"))
            gold = _path(base, task.get("gold_changes"))
            if not statement.is_file() or not hidden.is_file() or not gold.is_file():
                raise ValueError("statement, hidden tests, and gold must exist")
            if hidden.is_symlink() or gold.is_symlink() or statement.is_symlink():
                raise ValueError("symlinked fixture paths are not auditable")
            if hidden == gold:
                raise ValueError("hidden tests and gold must be separate")
            if hashlib.sha256(hidden.read_bytes()).hexdigest() != task["hidden_test_sha256"]:
                raise ValueError("hidden test bytes differ from registered SHA-256")
            resolved_starters = {}
            for language, name in starters.items():
                starter = _path(base, name)
                if not starter.is_dir() or starter.is_symlink():
                    raise ValueError(f"{language} starter must be a real directory")
                if inside(hidden, starter) or inside(gold, starter) or (starter / ".git").exists():
                    raise ValueError("hidden tests, gold, or repository history leak into starter")
                if any((Path(root) / ".git").exists() for root, _, _ in os.walk(starter)):
                    raise ValueError("nested repository history leaks into starter")
                files = starter_files(starter)
                low, high = (3, 10) if task["tier"] == "small" else (20, 100)
                if not low <= len(files) <= high:
                    _finding(findings, "invalid", "starter_file_count", where,
                             f"{language}: {len(files)} files; {task['tier']} needs {low}–{high}")
                markers = task.get("forbidden_markers")
                if not isinstance(markers, list) or not markers or any(not _nonempty(x) for x in markers):
                    _finding(findings, "invalid", "leak_markers", where, "nonempty task-specific markers required")
                else:
                    for file in files:
                        content = file.read_bytes()
                        if any(marker.encode("utf-8") in content for marker in markers):
                            _finding(findings, "invalid", "hidden_leak", where, str(file.relative_to(starter)))
                resolved_starters[language] = starter
            if matrix["kind"] == "holdout" and len(set(resolved_starters.values())) != len(resolved_starters):
                raise ValueError("holdout language starters resolve to the same directory")
            # A hidden file in another task's starter is caught after all tasks are indexed.
            task["_resolved_paths"] = (resolved_starters, hidden, gold)
        except (ValueError, OSError, KeyError) as error:
            _finding(findings, "invalid", "fixture_path", where, str(error))
    starters = [starter for task in tasks.values() if "_resolved_paths" in task
                for starter in task["_resolved_paths"][0].values()]
    for task_id, task in tasks.items():
        if "_resolved_paths" in task:
            _, hidden, gold = task["_resolved_paths"]
            if any(inside(hidden, starter) or inside(gold, starter) for starter in starters):
                _finding(findings, "invalid", "hidden_leak", task_id, "private fixture lies in an agent starter")
    return tasks


def _check_trace(trace, run, task, protocol, findings, where, *, holdout=False):
    if (not isinstance(trace, dict) or
            not TRACE_KEYS <= set(trace) or not set(trace) <= TRACE_KEYS | TRACE_EXTRA_KEYS or
            (holdout and not TRACE_EXTRA_KEYS <= set(trace))):
        _finding(findings, "invalid", "trace_schema", where,
                 "v2 trace fields required; holdout also requires actual tool tokens and tokenizer version")
        return False
    valid = True

    def fail(code, detail):
        nonlocal valid
        valid = False
        _finding(findings, "invalid", code, where, detail)

    if trace["schema_version"] != 2 or any(trace[k] != run[k] for k in RUN_KEYS - {"trace"}):
        fail("trace_identity", "trace identifiers, seed, or order differ from run entry")
    if trace["hidden_test_version"] != task.get("hidden_test_version"):
        fail("hidden_test_version", "trace uses a different hidden-test version")
    if any(trace[k] != protocol[k] for k in PROTOCOL_KEYS):
        fail("mixed_budget", "model tool, documentation, time, token, hardware, or toolchain budget differs")
    if type(trace["token_budget"]) is not int or not _number(trace["time_budget_seconds"], positive=True):
        fail("trace_schema", "token and wall budgets must be numeric")
    if any(not _nonempty(trace[k]) for k in ("model_family", "model_version", "run_id", "allowed_tools",
                                                 "documentation_budget", "hardware", "toolchain")):
        fail("trace_schema", "required text fields must be nonempty")
    if any(type(trace[k]) is not int or trace[k] < 0 for k in ("seed", "order", "prompt_tokens", "completion_tokens")):
        fail("trace_schema", "seed, order, and token counts must be nonnegative integers")
    if ("tool_tokens" in trace and (type(trace["tool_tokens"]) is not int or trace["tool_tokens"] < 0)
            or "tokenizer_version" in trace and not _nonempty(trace["tokenizer_version"])):
        fail("trace_schema", "tool_tokens must be nonnegative and tokenizer_version nonempty")
    if not _number(trace["wall_seconds"]) or trace["wall_seconds"] > protocol["time_budget_seconds"]:
        fail("trace_bounds", "wall time must fit the run budget")
    if (type(trace["prompt_tokens"]) is int and type(trace["completion_tokens"]) is int
            and type(trace.get("tool_tokens", 0)) is int
            and trace["prompt_tokens"] + trace["completion_tokens"] + trace.get("tool_tokens", 0)
            > protocol["token_budget"]):
        fail("trace_bounds", "total model tokens exceed budget")
    if not _sha(trace["final_patch_sha256"]):
        fail("patch_hash", "final patch SHA-256 is required")
    outcome = trace["hidden_test_result"]
    if outcome not in ("pass", "fail", "timeout"):
        fail("trace_outcome", "hidden-test result must be pass, fail, or timeout")
    if (outcome == "pass" and trace["failure_cause"] is not None) or (
            outcome != "pass" and not _nonempty(trace["failure_cause"])):
        fail("trace_outcome", "failures need a cause; successes cannot have one")
    if (outcome == "pass" and _number(trace["wall_seconds"])
            and type(trace["prompt_tokens"]) is int and type(trace["completion_tokens"]) is int
            and type(trace.get("tool_tokens", 0)) is int
            and (trace["wall_seconds"] == 0 or trace["prompt_tokens"] + trace["completion_tokens"]
                 + trace.get("tool_tokens", 0) == 0)):
        fail("trace_bounds", "successful runs need positive time and token usage")
    return valid


def audit(matrix, base, external_traces=None):
    """Return a mechanical audit and validated traces, with no inferred missing runs."""
    base = Path(base).resolve()
    findings = []
    if not isinstance(matrix, dict) or matrix.get("schema_version") != 2 or matrix.get("kind") not in ("public_calibration", "holdout"):
        return {"schema_version": 2, "status": "invalid", "findings": [
            {"severity": "invalid", "code": "matrix_schema", "where": "matrix", "detail": "requires v2 public_calibration or holdout"}],
                "validated_traces": []}
    conditions = matrix.get("conditions")
    protocol = matrix.get("protocol")
    if (not isinstance(conditions, list) or any(not isinstance(c, str) for c in conditions)
            or set(conditions) != LANGUAGES or len(conditions) != len(LANGUAGES)
            or not isinstance(matrix.get("tasks"), list) or not isinstance(matrix.get("runs"), list)
            or not isinstance(protocol, dict) or set(protocol) != PROTOCOL_KEYS):
        _finding(findings, "invalid", "matrix_schema", "matrix", "five conditions, tasks, runs, and exact protocol fields required")
        return {"schema_version": 2, "status": "invalid", "findings": findings, "validated_traces": []}
    if (any(not _nonempty(protocol[k]) for k in ("allowed_tools", "documentation_budget", "hardware", "toolchain"))
            or type(protocol["token_budget"]) is not int or not 0 < protocol["token_budget"] <= 10000000
            or not _number(protocol["time_budget_seconds"], positive=True)
            or protocol["time_budget_seconds"] > 86400):
        _finding(findings, "invalid", "protocol_schema", "protocol", "bounded, nonempty protocol fields required")
        return {"schema_version": 2, "status": "invalid", "findings": findings, "validated_traces": []}
    tasks = _task_index(matrix, base, findings)
    supplied = {}
    if external_traces is not None:
        for source in external_traces:
            try:
                source = Path(source)
                if source.is_symlink() or not source.is_file():
                    raise ValueError("external trace must be a regular file")
                trace = _read_json(source)
                run_id = trace.get("run_id") if isinstance(trace, dict) else None
                if not _nonempty(run_id):
                    raise ValueError("external trace needs a run_id")
                if run_id in supplied:
                    _finding(findings, "invalid", "duplicate_external_trace", str(source), run_id)
                supplied[run_id] = (trace, source)
            except (ValueError, OSError, json.JSONDecodeError) as error:
                _finding(findings, "invalid", "external_trace_file", str(source), str(error))
    if not tasks:
        _finding(findings, "incomplete", "no_tasks", "matrix", "no tasks registered")
    if matrix["kind"] == "holdout":
        freeze = matrix.get("freeze")
        if (not isinstance(freeze, dict) or set(freeze) != FREEZE_KEYS
                or any(not _sha(freeze[k]) for k in FREEZE_KEYS if k.endswith("sha256"))
                or type(freeze["order_seed"]) is not int
                or not _nonempty(freeze["frozen_at_utc"])
                or not isinstance(freeze["reviewer_ids"], list) or not freeze["reviewer_ids"]
                or any(not _nonempty(x) for x in freeze["reviewer_ids"])):
            _finding(findings, "incomplete", "holdout_freeze", "matrix", "predeclared hashes, order seed, time, and reviewers required")
        else:
            task_bytes = json.dumps(matrix["tasks"], sort_keys=True, separators=(",", ":"),
                                    ensure_ascii=False).encode("utf-8")
            acceptance_bytes = json.dumps(sorted((task_id, task.get("hidden_test_sha256"))
                                                  for task_id, task in tasks.items()),
                                          separators=(",", ":")).encode("utf-8")
            if hashlib.sha256(task_bytes).hexdigest() != freeze["task_set_sha256"]:
                _finding(findings, "invalid", "task_freeze_mismatch", "matrix", "task list changed after freeze")
            if hashlib.sha256(acceptance_bytes).hexdigest() != freeze["acceptance_sha256"]:
                _finding(findings, "invalid", "acceptance_freeze_mismatch", "matrix", "hidden-test digests changed after freeze")
        if not 60 <= len(tasks) <= 100:
            _finding(findings, "incomplete", "holdout_size", "matrix", "60–100 reviewed tasks required")
        if {task.get("tier") for task in tasks.values()} != {"small", "large"}:
            _finding(findings, "incomplete", "tier_coverage", "matrix", "both repository tiers required")
        required_categories = {"memory", "modularity", "foreign_library", "comprehension_repair"}
        if not required_categories <= {task.get("category") for task in tasks.values()}:
            _finding(findings, "incomplete", "category_coverage", "matrix", "memory, modularity, foreign-library, and comprehension/repair tasks required")
    seen_ids = set()
    seen_keys = set()
    expected = {}
    orders = {}
    validated = []
    for number, run in enumerate(matrix["runs"]):
        where = f"runs[{number}]"
        if not isinstance(run, dict) or set(run) != RUN_KEYS or any(not _nonempty(run[k]) for k in
                ("task_id", "language", "model_family", "model_version", "run_id", "trace")) or any(
                    type(run[k]) is not int or run[k] < 0 for k in ("seed", "order")):
            _finding(findings, "invalid", "run_schema", where, "versioned run identity and trace path required")
            continue
        task = tasks.get(run["task_id"])
        if task is None or run["language"] not in task.get("applicable_languages", []):
            _finding(findings, "invalid", "run_applicability", where, "unknown task or excluded language")
            continue
        if run["run_id"] in seen_ids:
            _finding(findings, "invalid", "duplicate_run_id", where, run["run_id"])
        seen_ids.add(run["run_id"])
        key = (run["task_id"], run["model_family"], run["model_version"], run["seed"])
        condition_key = key + (run["language"],)
        if condition_key in seen_keys:
            _finding(findings, "invalid", "duplicate_run", where, str(condition_key))
        seen_keys.add(condition_key)
        expected.setdefault(key, set()).add(run["language"])
        if run["order"] in orders.setdefault(key, set()):
            _finding(findings, "invalid", "duplicate_order", where, "condition order repeats within paired run")
        orders[key].add(run["order"])
        try:
            if external_traces is None:
                path = _path(base, run["trace"])
                if not path.is_file():
                    _finding(findings, "incomplete", "missing_trace", where, run["trace"])
                    continue
                trace = _read_json(path)
            else:
                item = supplied.get(run["run_id"])
                if item is None:
                    _finding(findings, "incomplete", "missing_trace", where, run["run_id"])
                    continue
                trace, path = item
            if any(inside(path, starter) for t in tasks.values() if "_resolved_paths" in t
                   for starter in t["_resolved_paths"][0].values()):
                raise ValueError("trace must remain outside agent starters")
            if _check_trace(trace, run, task, protocol, findings, where,
                            holdout=matrix["kind"] == "holdout"):
                validated.append(trace)
        except (ValueError, OSError, json.JSONDecodeError) as error:
            _finding(findings, "invalid", "trace_file", where, str(error))
    if not matrix["runs"]:
        _finding(findings, "incomplete", "no_runs", "matrix", "no expected model runs registered")
    if external_traces is not None:
        unknown = set(supplied) - seen_ids
        for run_id in sorted(unknown):
            _finding(findings, "invalid", "unregistered_external_trace", run_id,
                     "external trace has no preregistered run")
    for task_id in tasks:
        if not any(key[0] == task_id for key in expected):
            _finding(findings, "incomplete", "task_runs_missing", task_id, "no expected runs for task")
    for key, present in expected.items():
        needed = set(tasks[key[0]].get("applicable_languages", []))
        if present != needed:
            _finding(findings, "incomplete", "unpaired_condition", str(key), str(sorted(needed - present)))
    if matrix["kind"] == "holdout":
        grid = {key[1:] for key in expected}
        for task_id in tasks:
            missing = grid - {key[1:] for key in expected if key[0] == task_id}
            if missing:
                _finding(findings, "incomplete", "unequal_model_grid", task_id,
                         f"missing model/version/seed combinations: {sorted(missing)}")
        versions = {}
        for family, version, _ in grid:
            versions.setdefault(family, set()).add(version)
        if any(len(values) != 1 for values in versions.values()):
            _finding(findings, "invalid", "mixed_model_version", "runs",
                     "one frozen version per model family required")
        tokenizer_versions = {}
        for trace in validated:
            tokenizer_versions.setdefault(trace["model_family"], set()).add(trace["tokenizer_version"])
        if any(len(values) != 1 for values in tokenizer_versions.values()):
            _finding(findings, "invalid", "mixed_tokenizer_version", "runs",
                     "one frozen tokenizer version per model family required")
        for task_id in tasks:
            runs = [key for key in expected if key[0] == task_id]
            families = {key[1] for key in runs}
            if len(families) < 2 or any(len({key[3] for key in runs if key[1:3] == model}) < 2
                                        for model in {key[1:3] for key in runs}):
                _finding(findings, "incomplete", "model_coverage", task_id, "two families and repeated paired seeds required")
    status = "invalid" if any(item["severity"] == "invalid" for item in findings) else (
        "incomplete" if findings else "ready")
    return {"schema_version": 2, "kind": matrix["kind"], "status": status,
            "task_count": len(tasks), "expected_run_count": len(matrix["runs"]),
            "validated_run_count": len(validated), "findings": findings,
            "validated_traces": validated,
            "meaning": "mechanical validation; external trace authenticity and independent review require human evidence"}


def analyze(matrix, base, external_traces=None):
    audited = audit(matrix, base, external_traces)
    report = {"schema_version": 2, "scope": matrix.get("kind") if isinstance(matrix, dict) else None,
              "status": audited["status"], "audit": {k: v for k, v in audited.items() if k != "validated_traces"},
              "conditions": None, "selected_baseline": None, "comparison": None,
              "baseline_breakdown": None, "exclusions_by_condition": None, "task_counts": None,
              "bootstrap": None, "gates": None, "family_gates": None, "release_verdict": None,
              "runtime_evidence": "unmeasured: no application-shaped CGx2/Rust or PyTorch workload in this matrix"}
    if audited["status"] != "ready":
        report["uncertainty"] = "Incomplete or invalid traces; no numeric completion rate or gate verdict."
        return report
    report.update(analyze_ready(matrix, audited["validated_traces"]))
    return report


def ingest(matrix, base, trace, patch):
    """Validate an externally supplied observation against a matrix and exact patch bytes."""
    if not isinstance(trace, dict):
        raise ValueError("external trace must be an object")
    matches = [run for run in matrix.get("runs", []) if isinstance(run, dict) and run.get("run_id") == trace.get("run_id")]
    if len(matches) != 1:
        raise ValueError("run_id must identify exactly one preregistered run")
    run = matches[0]
    task = next((task for task in matrix["tasks"] if task.get("task_id") == run["task_id"]), None)
    if task is None:
        raise ValueError("run task is absent")
    hidden = _path(Path(base).resolve(), task["hidden_tests"])
    if hashlib.sha256(hidden.read_bytes()).hexdigest() != task["hidden_test_sha256"]:
        raise ValueError("hidden-test bytes differ from registered version digest")
    findings = []
    _check_trace(trace, run, task, matrix["protocol"], findings, "external trace",
                 holdout=matrix["kind"] == "holdout")
    if findings:
        raise ValueError("external trace fails contract: " + ", ".join(x["code"] for x in findings))
    patch = Path(patch)
    if not patch.is_file() or patch.stat().st_size > 10485760:
        raise ValueError("patch must be a regular file of at most 10 MiB")
    digest = hashlib.sha256(patch.read_bytes()).hexdigest()
    if digest != trace["final_patch_sha256"]:
        raise ValueError("final patch hash does not match patch bytes")
    starters = task.get("starters") or {language: task.get("starter") for language in task["applicable_languages"]}
    starter = _path(Path(base).resolve(), starters[run["language"]])
    if inside(patch.resolve(), starter):
        raise ValueError("external patch must be outside the agent starter")
    return trace


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("audit", "report", "ingest"))
    parser.add_argument("--matrix", type=Path, required=True)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--markdown", type=Path)
    parser.add_argument("--trace-input", type=Path, action="append",
                        help="external trace path; repeat for audit/report without writing or ingesting")
    parser.add_argument("--patch", type=Path)
    args = parser.parse_args(argv)
    try:
        if args.out and args.out.exists():
            raise FileExistsError(args.out)
        if args.markdown and (args.action != "report" or args.markdown.exists()):
            raise ValueError("--markdown requires report and a new output path")
        matrix = _read_json(args.matrix, MAX_MATRIX_BYTES)
        base = args.matrix.resolve().parent
        if args.action == "ingest":
            if args.trace_input is None or len(args.trace_input) != 1 or args.patch is None or args.out is None:
                raise ValueError("ingest needs --trace-input, --patch, and --out")
            incoming = _read_json(args.trace_input[0])
            value = ingest(matrix, base, incoming, args.patch)
            run = next(run for run in matrix["runs"] if run["run_id"] == value["run_id"])
            if args.out.resolve() != _path(base, run["trace"]):
                raise ValueError("--out must equal this run's preregistered trace path")
            for task in matrix["tasks"]:
                starters = task.get("starters") or {language: task.get("starter")
                                                    for language in task["applicable_languages"]}
                if any(inside(args.out.resolve(), _path(base, name)) for name in starters.values()):
                    raise ValueError("registered trace path must be outside every agent starter")
        else:
            value = (audit(matrix, base, args.trace_input) if args.action == "audit"
                     else analyze(matrix, base, args.trace_input))
            if args.action == "audit":
                value = {k: v for k, v in value.items() if k != "validated_traces"}
        rendered = json.dumps(value, indent=2, sort_keys=True) + "\n"
        if args.out:
            args.out.parent.mkdir(parents=True, exist_ok=True)
            args.out.write_text(rendered, encoding="utf-8")
        else:
            print(rendered, end="")
        if args.markdown:
            args.markdown.parent.mkdir(parents=True, exist_ok=True)
            args.markdown.write_text(markdown(value), encoding="utf-8")
        if args.action == "ingest":
            return 0
        return {"ready": 0, "incomplete": 1, "invalid": 2}[value["status"]]
    except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        parser.exit(2, f"holdout error: {error}\n")


if __name__ == "__main__":
    raise SystemExit(main())
