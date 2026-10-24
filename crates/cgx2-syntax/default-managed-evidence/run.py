#!/usr/bin/env python3
"""Bounded source/native/standalone managed acceptance, retaining failed attempts."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import resource
import shlex
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE.parent / "owned-result-runner-evidence"))
from supervisor import supervise, evidence


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def stable(observation):
    result = json.loads(json.dumps(observation))
    for region in result["memory"]["regions"]:
        region["region_id"]["session"] = 0
        region.pop("pause")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--only", help="Run one fixture while retaining all command outcomes")
    parser.add_argument("--repair-only", action="store_true", help="Run the bounded repair matrix, retaining every failed row")
    args = parser.parse_args()
    from repair_runner import main as repair_main
    repair_args = ["--binary-dir", str(args.binary_dir), "--output-dir", str(args.output_dir if args.repair_only else args.output_dir / "repair")]
    if args.repair_only:
        return repair_main(repair_args)
    repair_status = repair_main(repair_args) if not args.only else 0
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    binary = args.binary_dir.resolve() / "cgx2"
    env = {**os.environ, "RUSTUP_TOOLCHAIN": "1.98.1", "CGX2_MANAGED_EVIDENCE": "1",
           "CGX2_OWNED_EVIDENCE": "1"}
    records = []
    report = {"schema_version": 1, "checks": records, "checkpoint_complete": False,
              "release_complete": False, "external_leak_audit_performance": "unrun",
              "bounds": {"wall_seconds": 180, "cpu_seconds_per_process": 120,
                         "address_space_bytes_per_process": 4 * 1024**3,
                         "file_size_bytes_per_process": 512 * 1024**2,
                         "retained_bytes_per_stream": 2 * 1024**2,
                         "aggregate_cgroup_isolation": False},
              "binary_sha256": digest(binary), "runner_sha256": digest(Path(__file__)),
              "runtime_sha256": {p.name: digest(p) for p in sorted((ROOT / "crates/cgx2-runtime/src").glob("*.rs"))}}

    def save():
        report["passed"] = sum(r["passed"] for r in records)
        report["failed"] = sum(not r["passed"] for r in records)
        (output / "product.json").write_text(json.dumps(report, indent=2) + "\n")

    def limits():
        resource.setrlimit(resource.RLIMIT_AS, (4 * 1024**3, 4 * 1024**3))
        resource.setrlimit(resource.RLIMIT_CPU, (120, 120))
        resource.setrlimit(resource.RLIMIT_FSIZE, (512 * 1024**2, 512 * 1024**2))

    def command(label, argv, cwd, rc=0, code=None):
        result = supervise([str(a) for a in argv], cwd=cwd, env=env, limits=limits,
                           cap=2 * 1024**2, timeout=180)
        record = {"label": label, "command": [str(a) for a in argv], "cwd": str(cwd),
                  **evidence(result)}
        record["passed"] = result["reason"] == "exit" and result["exit"] == rc and (
            code is None or code in record["stderr"])
        records.append(record)
        save()
        assert record["passed"], record
        return record

    def assertion(label, condition):
        records.append({"label": label, "passed": bool(condition)})
        save()
        assert condition, label

    def accounting(record):
        return json.loads(next(line.split(" ", 1)[1] for line in record["stderr"].splitlines()
                               if line.startswith("CGX2_MANAGED_EVIDENCE ")))

    with tempfile.TemporaryDirectory(prefix="default-managed-", dir=os.environ["TMPDIR"]) as temp:
        work = Path(temp)
        pinned = command("pinned-rustc", ["rustup", "which", "rustc"], work)["stdout"].strip()
        report["rustc_identity"] = command("rustc-identity", [pinned, "-vV"], work)["stdout"]
        wrapper = work / "rustc-linker"
        wrapper.write_text("#!/bin/sh\nexec " + shlex.quote(pinned) + ' -Clink-arg=-Wl,--threads=2 "$@"\n')
        wrapper.chmod(0o700)
        fixtures = {name: (HERE / (name + ".cg"), expected) for name, expected in [
            ("shapes", "7\n"), ("alias", "42\n"), ("cycle", "42\n"),
            ("pressure", "52\n"), ("unicode", "262144\n"), ("recover", "42\n"),
            ("region-limit", "42\n"), ("hybrid", "49\n")]}
        fixtures["package"] = (HERE / "package", "2\n")
        fixtures["guard-package"] = (HERE / "guard-package", "42\n")
        fixtures["namespace-package"] = (HERE / "namespace-package", "42\n")
        generated = {
            "wide": "record Box:\n    value: i64\nfn main() -> i64:\n    let seed = Box(value: 7)\n    let values = [" + ",".join(["seed"] * 300) + "]\n    length(values)\n",
            "elements": "fn main() -> i64:\n    let values = [" + ",".join(["1"] * 65536) + "]\n    length(values)\n",
        }
        generated["nodes"] = "fn main() -> i64 effects(ManagedAlloc):\n    let a = [" + ",".join(["1"] * 32767) + "]\n    let b = [" + ",".join(["2"] * 32767) + "]\n    let exact = [a, b]\n    let result = managed.try_alloc(append(exact, [3]))\n    match result:\n        Result.Ok(value):\n            0\n        Result.Err(error):\n            match error:\n                AllocationError.ShapeLimit:\n                    42\n                AllocationError.BudgetExceeded:\n                    0\n                AllocationError.RegionLimit:\n                    0\n                AllocationError.HostAllocation:\n                    0\n                AllocationError.InvalidRegion:\n                    0\n"
        for name, text in generated.items():
            source = work / (name + ".cg")
            source.write_text(text)
            (output / (name + ".cg.gz")).write_bytes(gzip.compress(text.encode(), mtime=0))
            fixtures[name] = (source, "300\n" if name == "wide" else "42\n" if name == "nodes" else "65536\n")
        report["sources_sha256"] = {name: digest(source) for name, (source, _) in fixtures.items() if source.is_file()}
        report["package_sources_sha256"] = {str(p.relative_to(HERE)): digest(p) for source, _ in fixtures.values() if source.is_dir() for p in sorted(source.rglob("*")) if p.is_file() and p.suffix in (".cg", ".toml")}
        for name, (source, expected) in fixtures.items():
            if args.only and args.only != name:
                continue
            print("start", name, flush=True)
            command(name + ":check", [binary, "check", source], work)
            interpreted = command(name + ":interpreter", [binary, "run", source], work)
            observed = accounting(interpreted)
            assertion(name + ":value", interpreted["stdout"] == expected)
            assertion(name + ":roots", observed["final_roots"] == observed["baseline_roots"] == 0 and
                      observed["roots_registered"] == observed["roots_released"] and observed["cleanup_failures"] == 0)
            if name == "hybrid":
                rejected_index = command(name + ":effects-explicit-gate", [binary, "memory-effects", source], work, rc=1)
                assertion(name + ":no-synthetic-entry-index", not rejected_index["stdout"])
            else:
                indexed = command(name + ":effects", [binary, "memory-effects", source], work)
                index = json.loads(indexed["stdout"])
                entry = next(f for f in index["functions"] if f["name"] in ("main", "<script>"))
                assertion(name + ":authenticated-effects-parity", index["schema_version"] == 1 and index["summary_scope"] == "concrete-specializations" and index["absent_summary"] == "unknown" and entry["memory_effects_v1"] == observed["memory_effects_v1"] and sorted(entry["behavioral_effects"]) == sorted(observed["behavioral_effects"]))
            if name == "pressure":
                assertion(name + ":real-collection", observed["memory"]["totals"]["collection_count"] > 0 and observed["memory"]["totals"]["total_collected_bytes"] > 0)
            if name == "cycle":
                assertion(name + ":unreachable-cycle-reclaimed", observed["memory"]["totals"]["committed_bytes"] == 0)
            if name in ("recover", "region-limit", "hybrid", "nodes", "guard-package"):
                assertion(name + ":real-recovery", observed["recovered"] > 0)
            for release in [False, True]:
                artifact = work / f"{name}-{release}"
                command(artifact.name + ":build", [binary, "build", source, "--out", artifact,
                        "--rustc", wrapper] + (["--release"] if release else []), work)
                native = command(artifact.name + ":execute", [artifact], work)
                assertion(artifact.name + ":value-and-memory-parity", native["stdout"] == expected and stable(accounting(native)) == stable(observed))
                report.setdefault("artifacts_sha256", {})[artifact.name] = digest(artifact)
            emitted = work / (name + ".rs")
            artifact = work / (name + "-standalone")
            command(name + ":emit-rust", [binary, "emit", source, "--target", "rust", "--out", emitted], work)
            command(name + ":standalone-compile", [wrapper, "--edition=2024", emitted, "-o", artifact], work)
            standalone = command(name + ":standalone-execute", [artifact], work)
            assertion(name + ":standalone-parity", standalone["stdout"] == expected and stable(accounting(standalone)) == stable(observed))
            report.setdefault("artifacts_sha256", {})[emitted.name] = digest(emitted)
            report["artifacts_sha256"][artifact.name] = digest(artifact)
            if name in ("alias", "cycle", "recover", "region-limit", "hybrid", "nodes", "guard-package"):
                artifact = work / (name + (".pyz" if source.is_dir() else ".py"))
                artifact.write_bytes(b"previous artifact\n")
                command(name + ":python-reject", [binary, "emit", source, "--target", "python", "--out", artifact, "--force"], work, rc=1, code="CGE006")
                assertion(name + ":artifact-preserved", artifact.read_bytes() == b"previous artifact\n")
            elif name != "elements":
                artifact = work / (name + (".pyz" if source.is_dir() else ".py"))
                command(name + ":emit-python", [binary, "emit", source, "--target", "python", "--out", artifact], work)
                portable = command(name + ":python-execute", [sys.executable, artifact], work)
                assertion(name + ":portable-value", portable["stdout"] == expected)
            print("finish", name, flush=True)
        failures = [("arithmetic", "CGR006", "division by zero"),
                    ("arg-negative", "CGR007", "arg index must be nonnegative"),
                    ("contract", "CGR015", "requires condition failed"),
                    ("failed-arguments", "CGR006", "division by zero")]
        for name, code, message in failures:
            if args.only and args.only != name:
                continue
            source = HERE / (name + ".cg")
            report["sources_sha256"][name] = digest(source)
            interpreted = command(name + ":interpreter-error", [binary, "run", source], work, rc=1, code=code)
            observed = accounting(interpreted)
            assertion(name + ":primary-and-cleanup", message in interpreted["stderr"] and observed["final_roots"] == 0 and observed["cleanup_failures"] == 0 and observed["recovered"] == 0)
            for release in [False, True]:
                artifact = work / f"{name}-{release}"
                command(artifact.name + ":build", [binary, "build", source, "--out", artifact, "--rustc", wrapper] + (["--release"] if release else []), work)
                native = command(artifact.name + ":execute-error", [artifact], work, rc=1, code=code)
                assertion(artifact.name + ":primary-and-memory-parity", message in native["stderr"] and stable(accounting(native)) == stable(observed))
            emitted = work / (name + ".rs")
            artifact = work / (name + "-standalone")
            command(name + ":emit-rust", [binary, "emit", source, "--target", "rust", "--out", emitted], work)
            command(name + ":standalone-compile", [wrapper, "--edition=2024", emitted, "-o", artifact], work)
            native = command(name + ":standalone-error", [artifact], work, rc=1, code=code)
            assertion(name + ":standalone-primary-and-memory-parity", message in native["stderr"] and stable(accounting(native)) == stable(observed))
        from repair_runner import rejection_cases, rejection_matches, SENTINEL, preserved
        for name, source, expected in rejection_cases():
            if args.only and args.only != name:
                continue
            if source.is_file(): report["sources_sha256"][name] = digest(source)
            rejected = command(name + ":check-reject", [binary, "--diagnostic-format=json", "check", source], work, rc=1)
            assertion(name + ":exact-check-diagnostic", rejection_matches(rejected, expected))
            for target in ["native-debug", "native-release", "rust", "python"]:
                artifact = work / (name + "-" + target)
                artifact.write_bytes(SENTINEL)
                argv = [binary, "--diagnostic-format=json"]
                if target.startswith("native"):
                    argv += ["build", source, "--rustc", wrapper] + (["--release"] if target.endswith("release") else [])
                else:
                    argv += ["emit", source, "--target", target]
                rejected = command(name + ":" + target + "-reject", argv + ["--out", artifact, "--force"], work, rc=1)
                target_expected = expected[:1] if name == "dishonest-package" else expected
                assertion(name + ":" + target + "-exact-diagnostic", rejection_matches(rejected, target_expected))
                assertion(name + ":" + target + "-artifact-preserved", preserved(artifact))
        save()
    return repair_status


if __name__ == "__main__":
    raise SystemExit(main())
