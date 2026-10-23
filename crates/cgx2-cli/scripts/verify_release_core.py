#!/usr/bin/env python3
"""Exercise a real installed cgx2 outside the checkout; record commands and outcomes.

No installation, package download, host change, measurement, or release claim is
performed by this script. Install the candidate separately with Cargo --offline.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--work-root", type=Path, default=Path("/tmp"),
                        help="Existing scratch directory outside the checkout")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    fixtures = Path(__file__).resolve().parents[1] / "tests/fixtures/release-core"
    report = {"schema_version": 1, "binary": str(binary),
              "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "checks": [], "scope": "installed core command conformance; no timing claims"}
    failed = False

    # Intentionally outside the checkout even if the worker's TMPDIR is inside it.
    with tempfile.TemporaryDirectory(prefix="cgx2-installed-core-", dir=args.work_root) as temporary:
        root = Path(temporary)
        report["working_directory"] = str(root)
        install = root / "bin"
        install.mkdir()
        moved = install / "cgx2"
        shutil.copy2(binary, moved)
        native = binary.with_name("cgx2-native")
        if native.is_file():
            shutil.copy2(native, install / native.name)
        source = root / "scalar.cg"
        shutil.copy2(fixtures / "scalar.cg", source)
        package = root / "package"
        shutil.copytree(fixtures / "package", package)
        env = dict(os.environ, CGX2_PYTHON="/missing/python", CGX2_PYTHON_WORKER="/missing/worker")

        def run(name, argv, expected=0, stdout=None, command_env=None):
            nonlocal failed
            try:
                result = subprocess.run([str(a) for a in argv], cwd=root, env=command_env or env,
                                        text=True, capture_output=True, timeout=120, check=False)
                ok = result.returncode == expected and (stdout is None or result.stdout == stdout)
                report["checks"].append({"name": name, "status": "passed" if ok else "failed",
                                         "argv": [str(a) for a in argv], "exit_code": result.returncode,
                                         "stdout": result.stdout, "stderr": result.stderr})
                failed |= not ok
                return result
            except (OSError, subprocess.TimeoutExpired) as error:
                report["checks"].append({"name": name, "status": "blocked", "reason": str(error),
                                         "argv": [str(a) for a in argv]})
                failed = True
                return None

        def check(name, predicate):
            nonlocal failed
            report["checks"].append({"name": name, "status": "passed" if predicate else "failed"})
            failed |= not predicate

        for name, path in [("single", source), ("package", package)]:
            run(name + " check", [moved, "check", path])
            run(name + " interpreter", [moved, "run", path], stdout="25\n")
            for release in [False, True]:
                out = root / f"{name}-{'release' if release else 'debug'}"
                command = [moved, "build", path, "--out", out] + (["--release"] if release else [])
                result = run(name + " build " + out.name, command)
                if result and result.returncode == 0:
                    run(name + " native " + out.name, [out], stdout="25\n")
            for target in ["rust", "python"]:
                out = root / f"{name}.{'rs' if target == 'rust' else 'pyz' if name == 'package' else 'py'}"
                command = [moved, "emit", path, "--target", target, "--out", out]
                result = run(name + " emit " + target, command)
                if not result or result.returncode:
                    continue
                first = out.read_bytes()
                run(name + " force emit " + target, command + ["--force"])
                check(name + " deterministic " + target, first == out.read_bytes())
                if target == "python":
                    if name == "package":
                        with zipfile.ZipFile(out) as archive:
                            check("Python archive CRC/source members", archive.testzip() is None
                                  and "__main__.py" in archive.namelist()
                                  and all(info.date_time == (1980, 1, 1, 0, 0, 0) for info in archive.infolist()))
                    run(name + " emitted Python", [shutil.which("python3") or "python3", out], stdout="25\n")
                else:
                    executable = root / f"{name}-emitted"
                    compiled = run(name + " emitted Rust compilation", ["rustc", "--edition=2024", out, "-o", executable])
                    if compiled and compiled.returncode == 0:
                        run(name + " emitted Rust", [executable], stdout="25\n")
            first = run(name + " API documented spelling", [moved, "api", "--json", path])
            legacy = run(name + " API legacy spelling", [moved, "api", path, "--json"])
            if first and legacy and first.returncode == legacy.returncode == 0:
                check(name + " API stable JSON", json.loads(first.stdout) == json.loads(legacy.stdout))

        existing = root / "preserved"
        existing.write_bytes(b"keep this artifact")
        for name, text, mode in [("parse", "let =\n", "build"), ("type", "missing\n", "build"),
                                 ("target", "1\n", "target"), ("rustc", "1\n", "rustc"),
                                 ("Python profile", "let r = memory.region(mode: manual, limit: 256)\n", "python")]:
            bad = root / "bad.cg"
            bad.write_text(text)
            command = [moved, "--diagnostic-format=json"]
            if mode in ("target", "python"):
                command += ["emit", bad, "--target", "gpu" if mode == "target" else "python"]
            else:
                command += ["build", bad]
            command += ["--out", existing, "--force"]
            if mode == "rustc":
                command += ["--rustc", "/missing/rustc"]
            result = run(name + " rejection", command, expected=1, stdout="")
            check(name + " output preserved", existing.read_bytes() == b"keep this artifact")
            if result and result.returncode == 1:
                check(name + " structured diagnostic", json.loads(result.stderr)["severity"] == "error")

        unicode = "module release.main\n# café 世界\nfn main() -> i64:\n    missing\n"
        (package / "main.cg").write_text(unicode)
        for mode in ["api", "build", "emit"]:
            command = [moved, "--diagnostic-format=json", mode, package]
            if mode == "api":
                command += ["--json"]
            else:
                command += ["--out", existing, "--force"]
                if mode == "emit":
                    command += ["--target", "rust"]
            result = run(mode + " package negative", command, expected=1, stdout="")
            if result and result.returncode == 1:
                diagnostic = json.loads(result.stderr)
                check(mode + " relative Unicode span", diagnostic["file"] == "main.cg"
                      and diagnostic["span"] == {"start": unicode.encode().index(b"missing"),
                                                  "end": unicode.encode().index(b"missing") + 7})
            check(mode + " package output preserved", existing.read_bytes() == b"keep this artifact")

        # This is the existing, explicitly selected Python scalar route. Native
        # commands above ran with both worker/interpreter paths deliberately absent.
        python = root / "explicit.cg"
        python.write_text('foreign python fn root(x: i64) -> i64 effects(Foreign) caps(ForeignCall) = math.isqrt runtime "local:cpython" ownership copied\n\nfn main() -> i64 effects(Foreign) caps(ForeignCall):\n    root(81)\n')
        selected = shutil.which("python3")
        if selected:
            python_env = dict(os.environ, CGX2_PYTHON=selected, CGX2_PYTHON_TRACE="1")
            python_env.pop("CGX2_PYTHON_WORKER", None)
            result = run("explicit Python worker", [moved, "run", python], stdout="9\n", command_env=python_env)
            if result:
                check("explicit worker reported", "Python worker executable=" in result.stderr)
        else:
            report["checks"].append({"name": "explicit Python worker", "status": "blocked", "reason": "python3 absent"})
            failed = True
        run("native ELF dependencies", ["ldd", moved])
        last = report["checks"][-1]
        if "stdout" in last:
            check("native ELF has no libpython", "libpython" not in (last["stdout"] + last["stderr"]).lower())

    report["status"] = "failed" if failed else "passed"
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"{report['status']}: {len(report['checks'])} checks; {args.output}")
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
