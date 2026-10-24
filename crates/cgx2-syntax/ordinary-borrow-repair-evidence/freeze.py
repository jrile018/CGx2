#!/usr/bin/env python3
"""Freeze canonical final consumers and probe dependencies, never invoke Cargo."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    destination = Path(sys.argv[1]).resolve()
    destination.mkdir(exist_ok=False)
    source = ROOT / "target/release-ordinary-borrow-nominal/debug"
    report = dict(schema_version=1, base="89464a76c9bd84a3b090ec58a14ca6be0043c6ea",
                  source_directory=str(source), binary_directory=str(destination),
                  artifact_sha256={}, dependency_sha256={}, source_sha256={}, toolchain={})
    names = ["cgx2", "cgx2-native", "cgx2c", "cgx2-effect-index", "cgx2-security",
             "cgx2-verify", "cgx2-notebook", "libcgx2_cli.rlib", "libcgx2_sema.rlib",
             "libcgx2_package.rlib", "libcgx2_emit_rust.rlib"]
    for name in names:
        shutil.copy2(source / name, destination / name)
        expected = digest(source / name)
        assert digest(destination / name) == expected
        (destination / name).chmod(0o400 if name.endswith(".rlib") else 0o500)
        report["artifact_sha256"][name] = expected
    (destination / "deps").mkdir()
    for path in sorted((source / "deps").iterdir()):
        if path.is_file() and path.suffix in (".rlib", ".rmeta", ".so"):
            target = destination / "deps" / path.name
            shutil.copy2(path, target)
            expected = digest(path)
            assert digest(target) == expected
            target.chmod(0o400)
            report["dependency_sha256"][str(target.relative_to(destination))] = expected
    tracked = subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT).decode().split("\0")
    owned = "crates/cgx2-syntax/ordinary-borrow-repair-evidence"
    paths = {p for p in tracked if p.endswith((".rs", ".toml", ".lock", ".py"))}
    paths.update(str(p.relative_to(ROOT)) for p in (ROOT / owned).glob("*.py"))
    paths.add("crates/cgx2-sema/tests/ordinary_borrow_nominal_repair.rs")
    for name in sorted(paths):
        report["source_sha256"][name] = digest(ROOT / name)
    for name in ["rustc", "rustdoc", "cargo"]:
        path = Path(subprocess.check_output(["rustup", "which", "--toolchain", "1.98.1", name]).decode().strip()).resolve()
        report["toolchain"][name] = dict(path=str(path), sha256=digest(path))
    (destination / "manifest.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(dict(artifacts=len(names), dependencies=len(report["dependency_sha256"]),
                         sources=len(report["source_sha256"]), destination=str(destination))))


if __name__ == "__main__":
    main()
