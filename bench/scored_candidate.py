"""Fail-closed scored-submission path for independently frozen CGx2 tasks.

The private oracle stays on the host.  A candidate sees one generated call,
its copied sources, a read-only toolchain, and scratch; it never sees expected
answers or private fixture files.  The bundled Wave 14 tasks are still unrun
candidates and therefore cannot be scored by this command yet.
"""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile

import candidate_packet
import isolation
import task_evidence


BENCH = Path(__file__).resolve().parent
DEFAULT_PACK = BENCH / "fixtures/task-candidates/v3"
PRIVATE_ORACLE = DEFAULT_PACK / "private/accept.py"
PIN_FIELDS = (
    "prompt_sha256", "tools_sha256", "documentation_sha256",
    "dependency_lock_sha256", "cgx2_source_sha256", "compiler_sha256",
    "dependency_sha256",
)


def _oracle(base):
    path = base / "private/accept.py"
    spec = importlib.util.spec_from_file_location("cgx2_private_accept", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _sha(data):
    return hashlib.sha256(data).hexdigest()


def _safe_relative(name):
    path = Path(name)
    if path.is_absolute() or not path.parts or any(part in (".", "..") for part in path.parts):
        raise ValueError("unsafe task path")
    return path


def _check_workspace(base, task, card, language, submitted):
    """Validate the exact file set and copy it without following source links."""
    starter = (base / _safe_relative(task["workspace_paths"][language])).resolve(strict=True)
    supplied = Path(submitted)
    if supplied.is_symlink() or not supplied.is_dir():
        raise ValueError("candidate workspace must be a real directory")
    supplied = supplied.resolve(strict=True)
    if supplied.is_relative_to(base.resolve()) or starter.is_relative_to(supplied):
        raise ValueError("bundled fixture cannot be a scored submission")
    expected = {p.relative_to(starter) for p in starter.rglob("*") if p.is_file()}
    actual = set()
    for path in supplied.rglob("*"):
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise ValueError("symlinks and special files are forbidden")
        if path.is_file():
            actual.add(path.relative_to(supplied))
    if actual != expected:
        raise ValueError("workspace file set differs from pinned starter")
    allowed = {_safe_relative(name).relative_to(language) for name in card["allowed_files"]
               if _safe_relative(name).parts[0] == language}
    if not allowed <= expected:
        raise ValueError("reviewed editable files differ from starter")
    for relative in actual - allowed:
        if (supplied / relative).read_bytes() != (starter / relative).read_bytes():
            raise ValueError(f"file outside reviewed edit set changed: {relative}")
    for relative in actual:
        if relative.suffix not in (".cg", ".rs") and relative not in (Path("cgx2.toml"), Path("README.md")):
            raise ValueError(f"unexpected candidate file: {relative}")
    return supplied


def _preflight(base, task_id, language, submitted, toolchain, compiler_relative):
    base = Path(base).resolve(strict=True)
    pack, cards = candidate_packet.read_material(base)
    tasks = {task["task_id"]: task for task in pack["tasks"]}
    if task_id not in tasks or task_id not in cards:
        raise ValueError("unknown task")
    task, card = tasks[task_id], cards[task_id]
    if language not in ("cgx2", "rust") or language not in task["applicable_languages"]:
        raise ValueError("unsupported task language")
    if candidate_packet.review_state(task, card["review"]) != "approved":
        raise ValueError("task lacks independent frozen approval")
    freeze = pack.get("freeze")
    if not isinstance(freeze, dict) or not freeze.get("frozen_at_utc"):
        raise ValueError("pack has no immutable freeze record")
    task_evidence.review_records(task, base, task_evidence.utc(freeze["frozen_at_utc"]))
    protocol = pack["protocol"]
    for field in PIN_FIELDS:
        value = protocol.get(field)
        if not task_evidence.is_sha(value):
            raise ValueError(f"unfrozen protocol pin: {field}")
    for field in ("tool_version", "hardware"):
        if not isinstance(protocol.get(field), str) or not protocol[field].strip():
            raise ValueError(f"unfrozen protocol pin: {field}")
    starter = base / _safe_relative(task["workspace_paths"][language])
    _, starter_sha = task_evidence.tree_digest(starter)
    if starter_sha != task["repository_snapshots"][language]:
        raise ValueError("starter snapshot pin changed")
    prompt = base / _safe_relative(task["prompt_path"])
    if isolation.file_sha256(prompt) != task["prompt_sha256"]:
        raise ValueError("task prompt pin changed")
    private = base / _safe_relative(task["hidden_test_path"])
    if private.resolve(strict=True) != (base / "private/accept.py").resolve(strict=True):
        raise ValueError("unexpected private oracle path")
    if isolation.file_sha256(private) != task["hidden_test_sha256"]:
        raise ValueError("private oracle pin changed")
    workspace = _check_workspace(base, task, card, language, submitted)
    toolchain = Path(toolchain).resolve(strict=True)
    if not toolchain.is_dir() or toolchain.is_symlink():
        raise ValueError("toolchain root must be a real directory")
    if (base.is_relative_to(toolchain) or workspace.is_relative_to(toolchain)
            or Path.home().is_relative_to(toolchain) or BENCH.is_relative_to(toolchain)):
        raise ValueError("toolchain mount would expose repository, candidate, or home")
    relative = _safe_relative(compiler_relative)
    compiler = (toolchain / relative).resolve(strict=True)
    if not compiler.is_relative_to(toolchain) or not compiler.is_file():
        raise ValueError("compiler must be inside read-only toolchain")
    compiler_sha = isolation.file_sha256(compiler)
    if language == "cgx2" and compiler_sha != protocol["compiler_sha256"]:
        raise ValueError("CGx2 compiler does not match frozen pin")
    if compiler.stat().st_size > 256 * 1024**2:
        raise ValueError("compiler exceeds identity bound")
    return {"base": base, "task": task, "card": card, "workspace": workspace,
            "toolchain": toolchain, "compiler_relative": relative,
            "compiler_sha256": compiler_sha, "pack_sha256": isolation.file_sha256(base / "pack.json"),
            "card_sha256": isolation.file_sha256(base / "task-cards.json"),
            "oracle_sha256": isolation.file_sha256(private)}


def _make_case(source, stage, spec, language, args, group, compiler_relative):
    shutil.copytree(source, stage, symlinks=True)
    isolation.tree_sha256(stage)  # Refuse any source link before adding worker files.
    module = f"grid.{group}" if group else spec["module"]
    function = "charge" if group else spec["function"]
    if language == "cgx2":
        cg_args = ", ".join(f"(0 - {-value})" if value < 0 else str(value) for value in args)
        call = f"{module}.{function}({cg_args})"
        (stage / "src/main.cg").write_text(
            f"module {module.split('.')[0]}.main\nimport {module}\n"
            f"fn main() -> i64:\n    {call}\n", encoding="utf-8")
        build = [f"/toolchain/{compiler_relative.as_posix()}", "build", "--package",
                 "/workspace", "--out", "/scratch/program"]
    else:
        rust_module = group or spec["rust_module"]
        call = f"{rust_module}::{function}({', '.join(map(str, args))})"
        main = stage / "main.rs"
        original = main.read_text(encoding="utf-8")
        if "fn main()" not in original:
            raise ValueError("pinned Rust entry point missing")
        main.write_text(original[:original.index("fn main()")]
                        + f'fn main() {{ println!("{{}}", {call}); }}\n', encoding="utf-8")
        build = [f"/toolchain/{compiler_relative.as_posix()}", "--edition=2024",
                 "/workspace/main.rs", "-o", "/scratch/program"]
    (stage / ".cgx2-worker.json").write_text(json.dumps({"compiler": build[0],
                                                           "build_argv": build}), encoding="utf-8")
    shutil.copyfile(BENCH / "sandbox_worker.py", stage / ".cgx2-worker.py")
    shutil.copyfile(BENCH / "isolation.py", stage / "isolation.py")
    for path in stage.rglob("*"):
        path.chmod(0o755 if path.is_dir() else 0o644)
    stage.chmod(0o755)
    return build


def _cases(spec):
    for args, expected in spec["cases"]:
        yield None, args, expected
    for group, rate in spec.get("group_rates", {}).items():
        for units in (10, -3):
            yield group, (units,), rate * units


def _evaluate(observed, expected):
    if observed["reason"]:
        return False, observed["reason"]
    stderr = observed["stderr"].decode("utf-8", "replace")
    if "cgx2_isolation_ready\n" not in stderr:
        for build_reason in ("build_failed", "build_timeout", "build_output_limit",
                             "missing_build_output"):
            if stderr.startswith(build_reason + "\n"):
                return False, build_reason
        return False, "runtime_isolation_not_confirmed"
    if expected == "overflow":
        passed = observed["exit_code"] != 0 and "overflow" in stderr.lower()
        return passed, None if passed else "overflow_mismatch"
    text = observed["stdout"].decode("utf-8", "replace")
    lines = text.replace("\r\n", "\n").strip().splitlines()
    passed = observed["exit_code"] == 0 and lines == [str(expected)]
    return passed, None if passed else "output_mismatch"


def score(base, task_id, language, submitted, toolchain, compiler_relative):
    """Return machine-readable evidence; no subprocess before all gates pass."""
    evidence = {"schema_version": 1, "task_id": task_id, "language": language,
                "status": "unrun", "reason": None, "cases": [],
                "profile": isolation.profile_evidence()}
    try:
        context = _preflight(base, task_id, language, submitted, toolchain, compiler_relative)
    except (ValueError, OSError, KeyError, TypeError) as error:
        evidence["reason"] = f"preflight:{error}"
        return evidence
    evidence.update({key: context[key] for key in
                     ("compiler_sha256", "pack_sha256", "card_sha256", "oracle_sha256")})
    evidence["worker_sha256"] = isolation.file_sha256(BENCH / "sandbox_worker.py")
    evidence["isolation_sha256"] = isolation.file_sha256(BENCH / "isolation.py")
    evidence["toolchain_root"] = str(context["toolchain"])
    evidence["compiler_relative"] = context["compiler_relative"].as_posix()
    # Freeze one exact submission snapshot for every case.  Copy links as
    # links, then reject them; never follow a changed source link to the host.
    with tempfile.TemporaryDirectory(prefix="cgx2-submission-snapshot-") as snapshot_dir:
        snapshot = Path(snapshot_dir) / "candidate"
        shutil.copytree(context["workspace"], snapshot, symlinks=True)
        try:
            _check_workspace(context["base"], context["task"], context["card"],
                             language, snapshot)
            evidence["workspace_sha256"] = isolation.tree_sha256(snapshot)
        except (ValueError, OSError) as error:
            evidence["reason"] = f"snapshot:{error}"
            return evidence
        return _score_snapshot(evidence, context, task_id, language, snapshot)


def _score_snapshot(evidence, context, task_id, language, snapshot):
    try:
        evidence["capability"] = isolation.probe()
    except (isolation.UnsupportedIsolation, OSError) as error:
        evidence["status"] = "unsupported_isolation"
        evidence["reason"] = str(error)
        return evidence
    try:
        evidence["toolchain_sha256"] = isolation.toolchain_sha256(context["toolchain"])
    except (ValueError, OSError) as error:
        evidence["reason"] = f"toolchain_identity:{error}"
        return evidence
    oracle = _oracle(context["base"])
    if task_id not in oracle.SPECS:
        evidence["reason"] = "private_oracle_missing_task"
        return evidence
    spec = oracle.SPECS[task_id]
    for index, (group, args, expected) in enumerate(_cases(spec)):
        with tempfile.TemporaryDirectory(prefix="cgx2-scored-") as directory:
            stage = Path(directory) / "workspace"
            _make_case(snapshot, stage, spec, language, args, group,
                       context["compiler_relative"])
            try:
                fd = isolation.export_seccomp_fd()
                try:
                    argv = isolation.profile_argv(stage, context["toolchain"], fd,
                                                  ["/usr/bin/python3", "/workspace/.cgx2-worker.py"])
                    observed = isolation.capture(argv, fd)
                finally:
                    os.close(fd)
            except (isolation.UnsupportedIsolation, OSError) as error:
                evidence["status"] = "unsupported_isolation"
                evidence["reason"] = f"launch:{error}"
                return evidence
        passed, reason = _evaluate(observed, expected)
        evidence["cases"].append({"index": index, "group": group, "input_sha256":
                                   _sha(json.dumps(args).encode()), "passed": passed,
                                   "reason": reason, "exit_code": observed["exit_code"],
                                   "stdout_sha256": _sha(observed["stdout"]),
                                   "stderr_sha256": _sha(observed["stderr"])})
        if not passed:
            evidence["status"] = ("unsupported_isolation" if reason ==
                                  "runtime_isolation_not_confirmed" else "failed")
            evidence["reason"] = reason
            return evidence
    evidence["status"] = "passed"
    return evidence


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--probe", action="store_true", help="check isolation without a submission")
    parser.add_argument("--pack", type=Path, default=DEFAULT_PACK)
    parser.add_argument("--task")
    parser.add_argument("--language", choices=("cgx2", "rust"))
    parser.add_argument("--candidate-workspace", type=Path)
    parser.add_argument("--toolchain-root", type=Path)
    parser.add_argument("--compiler-relative")
    args = parser.parse_args(argv)
    if args.probe:
        try:
            result = {"status": "supported_isolation", "capability": isolation.probe(),
                      "profile": isolation.profile_evidence()}
        except (isolation.UnsupportedIsolation, OSError) as error:
            result = {"status": "unsupported_isolation", "reason": str(error),
                      "profile": isolation.profile_evidence()}
    else:
        if not all((args.task, args.language, args.candidate_workspace,
                    args.toolchain_root, args.compiler_relative)):
            parser.error("scoring needs --task, --language, --candidate-workspace, "
                         "--toolchain-root, and --compiler-relative")
        result = score(args.pack, args.task, args.language, args.candidate_workspace,
                       args.toolchain_root, args.compiler_relative)
    print(json.dumps(result, sort_keys=True))
    return 0 if result["status"] in ("passed", "supported_isolation") else 2


if __name__ == "__main__":
    raise SystemExit(main())
