"""Run positive and adversarial Rust fixtures in the scored worker profile.

This is a local isolation check, never a benchmark score.  On a host without
the required kernel capabilities it returns unsupported_isolation and runs no
fixture.  A capable worker must pass every fixture before accepting scoring.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import tempfile

import isolation


FIXTURES = Path(__file__).resolve().parent / "fixtures/isolation"
CASES = {
    "positive": b"ok\n",
    "private_read": b"denied\n",
    "alternate_file_api": b"denied\n",
    "process_exec": b"denied\n",
    "network_connect": b"denied\n",
    "symlink_traversal": b"denied\n",
    "memory_exhaustion": b"denied\n",
    "output_exhaustion": None,
}


def run(toolchain, compiler_relative, private_path):
    result = {"status": "unrun", "cases": [], "profile": isolation.profile_evidence()}
    try:
        result["capability"] = isolation.probe()
    except (isolation.UnsupportedIsolation, OSError) as error:
        result.update(status="unsupported_isolation", reason=str(error))
        return result
    toolchain = Path(toolchain).resolve(strict=True)
    relative = Path(compiler_relative)
    if relative.is_absolute() or not relative.parts or ".." in relative.parts:
        raise ValueError("compiler-relative must stay in toolchain")
    compiler = (toolchain / compiler_relative).resolve(strict=True)
    if not compiler.is_relative_to(toolchain) or not compiler.is_file():
        raise ValueError("compiler must be in toolchain root")
    result["compiler_sha256"] = isolation.file_sha256(compiler)
    result["worker_sha256"] = isolation.file_sha256(Path(__file__).parent / "sandbox_worker.py")
    for name, expected in CASES.items():
        with tempfile.TemporaryDirectory(prefix="cgx2-isolation-selftest-") as directory:
            stage = Path(directory) / "workspace"
            stage.mkdir(mode=0o755)
            source = (FIXTURES / f"{name}.rs").read_text(encoding="utf-8")
            (stage / "main.rs").write_text(source.replace("{PRIVATE}", str(private_path)),
                                            encoding="utf-8")
            compiler_path = f"/toolchain/{compiler_relative}"
            (stage / ".cgx2-worker.json").write_text(json.dumps({
                "compiler": compiler_path,
                "build_argv": [compiler_path, "--edition=2024", "/workspace/main.rs",
                               "-o", "/scratch/program"]}), encoding="utf-8")
            shutil.copyfile(Path(__file__).parent / "sandbox_worker.py", stage / ".cgx2-worker.py")
            shutil.copyfile(Path(__file__).parent / "isolation.py", stage / "isolation.py")
            for path in stage.iterdir():
                path.chmod(0o644)
            fd = isolation.export_seccomp_fd()
            try:
                argv = isolation.profile_argv(stage, toolchain, fd,
                                              ["/usr/bin/python3", "/workspace/.cgx2-worker.py"])
                observed = isolation.capture(argv, fd)
            finally:
                os.close(fd)
        ready = b"cgx2_isolation_ready\n" in observed["stderr"]
        passed = ready and (observed["reason"] == "output_limit" if expected is None
                            else observed["reason"] is None and observed["exit_code"] == 0
                            and observed["stdout"] == expected)
        result["cases"].append({"name": name, "passed": passed,
                                "reason": observed["reason"], "exit_code": observed["exit_code"],
                                "stdout_sha256": hashlib.sha256(observed["stdout"]).hexdigest(),
                                "stderr_sha256": hashlib.sha256(observed["stderr"]).hexdigest()})
        if not passed:
            result.update(status="failed", reason=f"fixture:{name}")
            return result
    result["status"] = "passed"
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toolchain-root", required=True, type=Path)
    parser.add_argument("--compiler-relative", required=True)
    parser.add_argument("--private-path", type=Path, default=Path(__file__).resolve().parent /
                        "fixtures/task-candidates/v3/private/accept.py")
    args = parser.parse_args(argv)
    result = run(args.toolchain_root, args.compiler_relative, args.private_path)
    print(json.dumps(result, sort_keys=True))
    return 0 if result["status"] == "passed" else 2


if __name__ == "__main__":
    raise SystemExit(main())
