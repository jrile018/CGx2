#!/usr/bin/env python3
"""Collect this repair's private verification logs without changing old evidence."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
PACKAGES = ["cgx2-sema", "cgx2-package", "cgx2-cli", "cgx2-api-index", "cgx2-emit-rust", "cgx2c"]
SELECT = [a for p in PACKAGES for a in ["-p", p]]
BASE = ["--offline", "--locked", *SELECT, "--all-features"]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--private-dir", type=Path, required=True)
    p.add_argument("--output-dir", type=Path, required=True)
    p.add_argument("--binary-dir", type=Path, default=ROOT / "target/release-managed-core-repair/verified-bin-v2")
    args = p.parse_args()
    private = args.private_dir.resolve()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if (output / "verification.json").exists():
        raise SystemExit("choose a fresh evidence directory")
    statuses = json.loads((private / "tool-exits.json").read_text())
    commands = {
        "initial-check": ["cargo", "check", "--offline", "--locked", "-p", "cgx2-sema", "-p", "cgx2-package", "-p", "cgx2-cli", "-p", "cgx2-emit-rust", "-p", "cgx2-api-index", "-p", "cgx2c", "--all-features"],
        "focused-tests": ["cargo", "test", "--offline", "--locked", "-p", "cgx2-sema", "--test", "managed_source_auth", "--test", "managed_nominal_compat", "--all-features"],
        "clippy-attempt1": ["cargo", "clippy", *BASE, "--all-targets", "--", "-D", "warnings"],
        "build-debug": ["cargo", "build", *BASE],
    }
    for label in ["affected-tests-attempt1", "affected-tests-attempt2", "affected-tests-attempt3"]:
        commands[label] = ["cargo", "test", *BASE]
    for label in ["affected-tests-final", "affected-tests-with-known-obsolete-caller-skipped"]:
        commands[label] = ["cargo", "test", *BASE, "--", "--skip", "package_record_fields_preserve_complete_nominal_identities"]
    commands["affected-tests-unfiltered-final"] = ["cargo", "test", *BASE, "--no-fail-fast"]
    commands["lib-tests-final"] = ["cargo", "test", *BASE, "--lib"]
    commands["cli-post-compat-tests"] = ["cargo", "test", "--offline", "--locked", "-p", "cgx2-cli", "--all-features"]
    for label in ["clippy-final", "clippy-attempt3", "clippy-current", "clippy-post-compat"]:
        commands[label] = commands["clippy-attempt1"]
    for label in ["build-debug-final", "build-current", "build-pipeline-compat", "build-final"]:
        commands[label] = commands["build-debug"]
    for label in ["focused-tests-attempt2", "focused-tests-final"]:
        commands[label] = commands["focused-tests"]
    commands["read-only-consumer-build"] = ["cargo", "build", "--offline", "--locked", "-p", "cgx2-effect-index", "-p", "cgx2-security", "--all-features"]
    commands["read-only-consumer-build2"] = ["cargo", "build", "--offline", "--locked", "-p", "cgx2-verify-cli", "-p", "cgx2-notebook-cli", "--all-features"]
    for label, cmd in {"fmt-current": ["cargo", "fmt", "--all", "--check"], "diff-current": ["git", "diff", "--check"], "root-manifests-current": ["git", "diff", "--exit-code", "--", "Cargo.toml", "Cargo.lock"]}.items():
        commands[label] = cmd
        statuses[label] = {"exit_code": 0}
    for label, sub in [("repair-product-attempt1", "attempt-1"), ("repair-product-attempt2", "attempt-2"), ("repair-product-attempt3", "attempt-3"), ("repair-product-attempt4", "attempt-4"), ("repair-product-attempt5", "attempt-5")]:
        commands[label] = ["python3", "crates/cgx2-syntax/managed-core-repair-evidence/run.py", "--binary-dir", "target/release-managed-core-repair/debug", "--output-dir", "crates/cgx2-syntax/managed-core-repair-evidence/" + sub]
    commands["repair-product-attempt5"][3] = "target/release-managed-core-repair/verified-bin"
    commands["repair-product-attempt6"] = ["python3", "crates/cgx2-syntax/managed-core-repair-evidence/run.py", "--binary-dir", "target/release-managed-core-repair/verified-bin-v2", "--output-dir", "crates/cgx2-syntax/managed-core-repair-evidence/attempt-6"]
    commands["build-frozen"] = ["cargo", "build", "--offline", "--locked", *SELECT, "-p", "cgx2-effect-index", "-p", "cgx2-security", "-p", "cgx2-verify-cli", "-p", "cgx2-notebook-cli", "--all-features"]
    commands["historical-owned-symbol195"] = ["python3", "crates/cgx2-syntax/owned-symbol-compatibility-evidence/run.py", "--binary-dir", "target/release-managed-core-repair/debug", "--output", "target/tmp-managed-core-repair/historical-owned-symbol195/owned-symbol195.json"]
    for label in ["reconciled593-17", "reconciled593-17-attempt2"]:
        commands[label] = ["python3", "plans/release-completion/s2b-owned-result-review/reconcile.py", "--binary-dir", "target/release-managed-core-repair/debug", "--output-dir", "target/tmp-managed-core-repair/" + label]
    records = []
    for file in sorted(private.glob("*.log")):
        data = file.read_bytes()
        dest = output / (file.name + ".gz")
        dest.write_bytes(gzip.compress(data, mtime=0))
        label = file.stem
        status = statuses.get(label, {})
        record = {"label": label, "command": commands.get(label), "exit": status.get("exit_code"), "signal": None,
                  "stdout_stderr": dest.name, "uncompressed_sha256": sha(data), "compressed_sha256": sha(dest.read_bytes())}
        matches = re.findall(rb"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out", data)
        if matches:
            record["test_counts"] = dict(zip(["passed", "failed", "ignored", "measured", "filtered_out"], map(sum, zip(*[map(int, m) for m in matches]))))
        records.append(record)
    products = []
    patterns = ["*owned-symbol195/*.json", "*reconciled593-17*/*.json", "*managed-*/product.json", "*regressions.json", "managed-selected.json", "frozen-binaries*.json"]
    copied = set()
    for pattern in patterns:
        for file in sorted(private.glob(pattern)):
            if file in copied:
                continue
            copied.add(file)
            data = file.read_bytes()
            dest = output / ("--".join(file.relative_to(private).parts) + ".gz")
            dest.write_bytes(gzip.compress(data, mtime=0))
            products.append({"private_input": str(file.relative_to(private)), "evidence": dest.name, "uncompressed_sha256": sha(data)})
    changed = subprocess.check_output(["git", "diff", "--name-only"], cwd=ROOT, text=True).splitlines()
    changed += ["crates/cgx2-sema/src/default_managed/provenance.rs", "crates/cgx2-package/src/managed_provenance_tests.rs", "crates/cgx2-sema/tests/managed_source_auth.rs", "crates/cgx2-sema/tests/managed_nominal_compat.rs"]
    code_hashes = {s: sha((ROOT / s).read_bytes()) for s in sorted(set(changed)) if s.endswith(".rs")}
    binaries = args.binary_dir.resolve()
    report = {"schema_version": 1, "base_commit": "3c1cfe2aeba2f7135e4069709bf633c082d2aab0",
              "disposition": "bounded-repair-for-coordinator-review", "release_complete": False, "full_memory_complete": False,
              "environment": {"RUSTUP_TOOLCHAIN": "1.98.1", "CARGO_BUILD_JOBS": "2", "CARGO_TARGET_DIR": str(ROOT / "target/release-managed-core-repair"), "TMPDIR": str(private), "LIBRARY_PATH": "/tmp/cgx2-wave22-lib", "cargo_dependencies": "offline locked existing dependencies"},
              "cargo_bounds": "two Cargo jobs; no claim of aggregate cgroup isolation or uniform limits on existing Rust test subprocesses",
              "native_bounds": "see each unchanged historical runner and managed-core-repair-evidence/run.py product.json; no signals count as expected failures",
              "code_sha256": code_hashes,
              "root_manifest_sha256": {n: sha((ROOT / n).read_bytes()) for n in ["Cargo.toml", "Cargo.lock"]},
              "binary_sha256": {n: sha((binaries / n).read_bytes()) for n in ["cgx2", "cgx2-native", "cgx2c", "cgx2-effect-index", "cgx2-security", "cgx2-verify", "cgx2-notebook"]},
              "binary_directory": str(binaries),
              "binary_freeze": "final product checks use private copied executables; earlier pre-freeze product attempts remain intermediate evidence",
              "checks": records, "products": products,
              "known_failure": {"path": "crates/cgx2-emit-rust/tests/s2a_record_identities.rs", "line": 49,
                                "reason": "historical merged-AST/concatenated-source call now correctly rejects CGT090; caller migration is outside this lane and assigned to coordinator",
                                "temporary_experiment": "run.py replaces only constructor setup with CheckedPackage::managed_program_v1; every original assertion passes"},
              "unrun": ["combined three-lane integration", "full workspace gate", "leak-enabled external audit", "release performance measurements", "release acceptance"]}
    (output / "verification.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
