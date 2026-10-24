#!/usr/bin/env python3
"""Coordinator-only reconciliation; never edits historical fixtures or runners."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shlex
import shutil
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
ORIGINAL = ROOT / "crates/cgx2-syntax/owned-result-evidence"
sys.path.insert(0, str(ROOT / "crates/cgx2-syntax/owned-result-runner-evidence"))
from supervisor import supervise, evidence

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--binary-dir", type=Path, required=True)
    p.add_argument("--output-dir", type=Path, required=True)
    args = p.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    binary = args.binary_dir.resolve() / "cgx2"
    loader = importlib.util.spec_from_file_location("historical_result_runner", ORIGINAL / "run.py")
    module = importlib.util.module_from_spec(loader)
    loader.loader.exec_module(module)
    with tempfile.TemporaryDirectory(prefix="reconciled-result-", dir=os.environ["TMPDIR"]) as tmp:
        work = Path(tmp)
        shutil.copytree(ORIGINAL / "fixtures", work / "fixtures")
        shutil.copyfile(ORIGINAL / "positive.json", work / "positive.json")
        expected = json.loads((ORIGINAL / "expected.json").read_text())
        superseded = expected.pop("shadow-builtin")
        reserved = (work / "fixtures/shadow-builtin.cg").read_text().replace("try_owned: i64", "owned: i64")
        (work / "fixtures/reserved-owned.cg").write_text(reserved)
        ambiguous = "record AllocationError:\n    x: i64\n" + (work / "fixtures/recover.cg").read_text()
        (work / "fixtures/ambiguous-error.cg").write_text(ambiguous)
        expected["reserved-owned"] = {"code": "CGT082", "span": {"start": 33, "end": 43}, "slice": "owned: i64"}
        expected["ambiguous-error"] = {"code": "CGT081", "span": {"start": 81, "end": 114}, "slice": "Result<owned<R>, AllocationError>"}
        (work / "expected.json").write_text(json.dumps(expected, indent=2) + "\n")
        module.HERE = work  # Only input manifest moves; authenticated runtime ROOT stays original.
        original_argv = sys.argv
        sys.argv = ["run.py", "--binary-dir", str(args.binary_dir), "--output", str(args.output_dir / "reconciled-product.json")]
        try:
            module.main()
        finally:
            sys.argv = original_argv
        records = []
        def command(label, argv, rc=0, diagnostic=None):
            result = supervise([str(a) for a in argv], cwd=work, env={**os.environ, "CGX2_OWNED_EVIDENCE": "1"},
                               limits=module.limits, cap=module.CAP, timeout=120)
            record = {"label": label, "command": [str(a) for a in argv], **evidence(result)}
            record["passed"] = result["reason"] == "exit" and result["exit"] == rc and (diagnostic is None or diagnostic in record["stderr"])
            records.append(record)
            assert record["passed"], record
            return record["stdout"], record["stderr"]
        def assertion(label, condition):
            records.append({"label": label, "passed": bool(condition)})
            assert condition, label
        def accounting(err):
            return json.loads(next(x.split(" ", 1)[1] for x in err.splitlines() if x.startswith("CGX2_OWNED_EVIDENCE ")))
        try:
            source = ORIGINAL / "fixtures/shadow-builtin.cg"
            command("restored-shadow:check", [binary, "check", source])
            out, err = command("restored-shadow:interpreter", [binary, "run", source])
            observed = accounting(err)
            assertion("restored-shadow:value-cleanup", out == "0\n" and observed["allocations"] == observed["releases"] == observed["live_bytes"] == 0)
            rustc, _ = command("pinned-rustc", ["rustup", "which", "rustc"])
            wrapper = work / "rustc-linker"
            wrapper.write_text("#!/bin/sh\nexec " + shlex.quote(rustc.strip()) + ' -Clink-arg=-Wl,--threads=2 "$@"\n')
            wrapper.chmod(0o700)
            for release in (False, True):
                artifact = work / f"restored-shadow-{release}"
                command(artifact.name + ":build", [binary, "build", source, "--out", artifact, "--rustc", wrapper] + (["--release"] if release else []))
                native, err = command(artifact.name + ":execute", [artifact])
                assertion(artifact.name + ":parity", native == out and accounting(err) == observed)
            runtime = work / "runtime"
            runtime.mkdir()
            for name in module.RUNTIME:
                shutil.copyfile(ROOT / "crates/cgx2-runtime/src" / name, runtime / name)
            rlib = work / "libcgx2_runtime.rlib"
            command("standalone-runtime", [wrapper, "--edition=2021", "--crate-name", "cgx2_runtime", "--crate-type", "rlib", runtime / "lib.rs", "-o", rlib])
            emitted, artifact = work / "restored-shadow.rs", work / "restored-shadow-standalone"
            command("restored-shadow:emit-rust", [binary, "emit", source, "--target", "rust", "--out", emitted])
            command("restored-shadow:standalone-compile", [wrapper, "--edition=2024", emitted, "--extern", f"cgx2_runtime={rlib}", "-o", artifact])
            native, err = command("restored-shadow:standalone-execute", [artifact])
            assertion("restored-shadow:standalone-parity", native == out and accounting(err) == observed)
            artifact = work / "restored-shadow.py"
            artifact.write_bytes(b"previous artifact\n")
            command("restored-shadow:python-reject", [binary, "emit", source, "--target", "python", "--out", artifact, "--force"], rc=1, diagnostic="CGE006")
            assertion("restored-shadow:python-preserved", artifact.read_bytes() == b"previous artifact\n")
        finally:
            report = {"status": "coordinator-reconciliation", "historical_files_modified": False,
                      "superseded_negative": superseded, "superseded_negative_count": 9,
                      "historical_fixture_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                      "runner_sha256": hashlib.sha256((ORIGINAL / "run.py").read_bytes()).hexdigest(),
                      "new_negative_expectations": {k: expected[k] for k in ("reserved-owned", "ambiguous-error")},
                      "checks": records, "passed": sum(x["passed"] for x in records), "failed": sum(not x["passed"] for x in records),
                      "release_complete": False}
            (args.output_dir / "restored-shadow.json").write_text(json.dumps(report, indent=2) + "\n")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
