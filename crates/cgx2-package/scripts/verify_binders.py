#!/usr/bin/env python3
"""Exercise exported generic binders through real CGx2 product routes; no timings."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


CRATE = Path(__file__).resolve().parents[1]
ROOT = CRATE.parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    options = parser.parse_args()
    binary = (options.binary_dir / "cgx2").resolve()
    output = options.output.resolve()
    os.environ.setdefault("RUSTUP_TOOLCHAIN", "1.98.1")
    os.environ.setdefault("CARGO_BUILD_JOBS", "2")
    checks = []
    fixtures = {
        name: CRATE / "fixtures" / name
        for name in ["binder-record", "binder-variant", "binder-nested"]
    }
    fixtures["reviewed-repro"] = (
        ROOT / "plans/release-completion/s2b-review/evidence/binder-package"
    )
    with tempfile.TemporaryDirectory(prefix="product-binders-", dir=os.environ["TMPDIR"]) as directory:
        scratch = Path(directory).resolve()

        def run(name, command, expected="", error=None):
            command = [str(item) for item in command]
            record = {"name": name, "command": command, "cwd": str(scratch)}
            try:
                completed = subprocess.run(
                    command, cwd=scratch, capture_output=True, text=True, timeout=120,
                )
                passed = (
                    completed.returncode == 0 and (expected is None or completed.stdout == expected)
                    if error is None
                    else completed.returncode != 0 and error in completed.stderr
                )
                record.update(
                    passed=passed, exit_code=completed.returncode,
                    stdout=completed.stdout, stderr=completed.stderr,
                    expected_stdout=expected if error is None else None,
                    expected_error=error,
                )
            except (OSError, subprocess.TimeoutExpired) as exception:
                record.update(passed=False, failure=str(exception))
            checks.append(record)
            return record["passed"]

        for name, fixture in fixtures.items():
            run(f"{name}:check", [binary, "check", fixture])
            run(f"{name}:run", [binary, "run", fixture], "42\n")
            for release in [False, True]:
                profile = "release" if release else "debug"
                executable = scratch / f"{name}-{profile}"
                command = [binary, "build", fixture, "--out", executable]
                if release:
                    command.append("--release")
                if run(f"{name}:build-{profile}", command, expected=None):
                    run(f"{name}:native-{profile}", [executable], "42\n")
            emitted = scratch / f"{name}.pyz"
            if run(f"{name}:emit-python", [binary, "emit", fixture, "--target", "python", "--out", emitted]):
                run(f"{name}:python", [sys.executable, emitted], "42\n")

        mutations = [
            ("record-argument", "binder-record", "src/main.cg", "value: 42", "value: true", "CGT003"),
            ("variant-argument", "binder-variant", "src/main.cg", ".Value(42)", ".Value(true)", "CGT003"),
            ("nested-argument", "binder-nested", "src/main.cg", "array(right.value)", "array([true])", "CGT003"),
            ("qualified-nominal", "binder-nested", "src/main.cg", "let nominal = app.left.T(value: 1)", "let nominal = app.right.T.Nominal(true)", "CGT003"),
            ("module-identity", "binder-nested", "src/main.cg", "let right = app.right.Box<i64>", "let right: app.left.Box<i64> = app.right.Box<i64>", "CGT003"),
            ("constraint", "binder-nested", "src/main.cg", "fn main() -> i64:", "fn bad<U>(value: U) -> U effects() caps():\n    app.left.identity<U>(value)\nfn main() -> i64:", "CGT039"),
            ("private", "binder-record", "src/dep.cg", "pub record Box<T>", "record Box<T>", "CGT022"),
            ("hidden-effect", "binder-nested", "src/left.cg", "    value\npub fn equal", "    let count = arg_count()\n    value\npub fn equal", "CGT012"),
        ]
        for name, fixture, relative, old, new, code in mutations:
            package = scratch / name
            shutil.copytree(fixtures[fixture], package)
            path = package / relative
            text = path.read_text()
            assert old in text, name
            path.write_text(text.replace(old, new))
            run(f"{name}:reject-check", [binary, "check", package], error=code)
            for target in ["native", "rust", "python"]:
                artifact = scratch / f"{name}-{target}.out"
                sentinel = b"preserved artifact\n"
                artifact.write_bytes(sentinel)
                command = (
                    [binary, "build", package, "--out", artifact, "--force"]
                    if target == "native"
                    else [binary, "emit", package, "--target", target, "--out", artifact, "--force"]
                )
                run(f"{name}:reject-{target}", command, error=code)
                checks.append({
                    "name": f"{name}:preserve-{target}",
                    "passed": artifact.read_bytes() == sentinel,
                })

    report = {
        "schema_version": 1,
        "scope": "S2b package exported binder repair only",
        "reviewed_base": "b595724e1b086c604220cc5b165c141e65a0531f",
        "release_complete": False,
        "passed": sum(check["passed"] for check in checks),
        "failed": sum(not check["passed"] for check in checks),
        "checks": checks,
        "fixture_sha256": {
            str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
            for fixture in fixtures.values() for path in sorted(fixture.rglob("*"))
            if path.is_file()
        },
        "implementation_sha256": {
            str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in [CRATE / "src/lib.rs", CRATE / "tests/binders.rs", Path(__file__).resolve()]
        },
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"{report['passed']} passed; {report['failed']} failed; {output}")
    return int(report["failed"] != 0)


if __name__ == "__main__":
    raise SystemExit(main())
