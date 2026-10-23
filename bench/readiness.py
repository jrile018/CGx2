"""One-command, fail-honest readiness report for the public syntax pilot."""

import argparse
import hashlib
import json
from pathlib import Path
import shlex
import shutil
import subprocess

import isolation
from syntax_packet import PACKET, ROOT, validate
from syntax_trace import summarize


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def command_output(argv):
    result = subprocess.run(argv, capture_output=True, text=True, check=False)
    return result.stdout.strip() if result.returncode == 0 else None


def tool_binary(name):
    if shutil.which("rustup"):
        actual = command_output(["rustup", "which", name])
        if actual:
            return Path(actual)
    found = shutil.which(name)
    return Path(found).resolve() if found else None


def toolchain_check(packet):
    lock = json.loads((packet / "toolchain-lock.json").read_text())
    project = ROOT.parent
    observed = {"cargo_lock_sha256": digest(project / "Cargo.lock"),
                "rustc_version": command_output(["rustc", "--version"]),
                "cargo_version": command_output(["cargo", "--version"]),
                "compiler_source_git_tree": command_output(["git", "rev-parse", "HEAD:crates/cgx2c"])}
    for tool in ("rustc", "cargo"):
        path = tool_binary(tool)
        observed[f"{tool}_sha256"] = digest(path) if path and path.is_file() else None
    keys = set(observed)
    return {"status": "pass" if all(observed[k] == lock[k] for k in keys) else "mismatch",
            "build_recipe": lock["build_recipe"], "frozen_hashes": {k: lock[k] for k in keys},
            "observed": observed}


def build_compiler(packet):
    lock = json.loads((packet / "toolchain-lock.json").read_text())
    argv = shlex.split(lock["build_recipe"])
    target_index = argv.index("--target-dir")
    compiler = Path(argv[target_index + 1]) / "debug/cgx2c"
    result = subprocess.run(argv, cwd=ROOT.parent, capture_output=True, text=True, check=False)
    if result.returncode or not compiler.is_file():
        return None, {"status": "failed", "exit_status": result.returncode,
                      "stderr": result.stderr[-2000:]}
    return compiler, {"status": "pass", "compiler_sha256": digest(compiler)}


def report(packet=PACKET, compiler=None, trace_paths=(), build=False):
    packet = Path(packet).resolve()
    rows = [json.loads(Path(path).read_text()) for path in trace_paths]
    toolchain = toolchain_check(packet)
    build_status = {"status": "unrun"}
    if build:
        if toolchain["status"] == "pass":
            compiler, build_status = build_compiler(packet)
        else:
            build_status = {"status": "blocked_toolchain_mismatch"}
    elif compiler:
        build_status = {"status": "supplied", "compiler_sha256": digest(compiler)}
    task_check = validate(packet, compiler)
    try:
        capability = isolation.probe()
        isolation_check = {"status": "supported_isolation", "capability": capability}
    except isolation.UnsupportedIsolation as exc:
        isolation_check = {"status": "unsupported_isolation", "reason": str(exc)}
    return {"schema_version": 1, "scope": "public_syntax_pilot_only",
            "independent_human_review": "unrun",
            "toolchain": toolchain, "compiler_build": build_status,
            "task_equivalence": task_check,
            "candidate_isolation_probe": isolation_check,
            "paired_model_traces": summarize(rows, packet),
            "improvement_claim": "none"}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--packet", type=Path, default=PACKET)
    parser.add_argument("--compiler", type=Path)
    parser.add_argument("--build", action="store_true", help="run frozen offline compiler recipe")
    parser.add_argument("--trace", type=Path, action="append", default=[])
    parser.add_argument("--out", type=Path)
    args = parser.parse_args(argv)
    if args.build and args.compiler:
        parser.error("choose --build or --compiler")
    value = report(args.packet, args.compiler, args.trace, args.build)
    rendered = json.dumps(value, indent=2, sort_keys=True) + "\n"
    if args.out:
        args.out.write_text(rendered)
    print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
