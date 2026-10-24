#!/usr/bin/env python3
"""Bounded ordinary-memory product checks; each invocation preserves a new attempt."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import shlex
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE.parent / "owned-result-runner-evidence"))
from supervisor import supervise, evidence


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def normal(record, expected=0):
    return record["reason"] == "exit" and record["exit"] == expected and record.get("signal") is None


def limits():
    resource.setrlimit(resource.RLIMIT_AS, (4 * 1024**3, 4 * 1024**3))
    resource.setrlimit(resource.RLIMIT_CPU, (120, 120))
    resource.setrlimit(resource.RLIMIT_FSIZE, (512 * 1024**2, 512 * 1024**2))


def observed(record):
    prefix = "CGX2_MEMORY_EVIDENCE "
    rows = [line[len(prefix):] for line in record["stderr"].splitlines() if line.startswith(prefix)]
    if len(rows) != 1:
        return None
    try:
        return json.loads(rows[0])
    except json.JSONDecodeError:
        return None


def stable(value):
    if isinstance(value, dict):
        return {k: stable(v) for k, v in value.items() if k != "pause"}
    if isinstance(value, list):
        return [stable(v) for v in value]
    return value


def rejection(record, code, start, end):
    if not normal(record, 1):
        return False
    try:
        data = json.loads(record.get("stdout") or record.get("stderr", ""))
    except (json.JSONDecodeError, TypeError):
        return False
    rows = data if isinstance(data, list) else data.get("diagnostics", [data] if "code" in data else [])
    return len(rows) == 1 and rows[0].get("code") == code and rows[0].get("span") == {"start": start, "end": end}


def suite_limits():
    resource.setrlimit(resource.RLIMIT_AS, (4 * 1024**3, 4 * 1024**3))
    resource.setrlimit(resource.RLIMIT_FSIZE, (512 * 1024**2, 512 * 1024**2))
    resource.setrlimit(resource.RLIMIT_CPU, (1800, 1800))


def historical_shadow_failure(report):
    failures=[r for r in report.get("commands",[]) if r.get("passed") is False]
    return report.get("passed")==523 and report.get("failed")==1 and len(failures)==1 and failures[0].get("label")=="shadow-builtin:reject" and normal(failures[0],0)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--regressions", action="store_true")
    args = parser.parse_args()
    binary = args.binary_dir.resolve()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=False)
    runner_source=Path(__file__).read_bytes()
    (output / "runner-snapshot.py").write_bytes(runner_source)
    fixture_dir = output / "fixtures-snapshot"
    fixture_dir.mkdir()
    fixture_before = {p.name:digest(p) for p in (HERE / "fixtures").glob("*.cg")}
    for name in fixture_before:
        (fixture_dir / name).write_bytes((HERE / "fixtures" / name).read_bytes())
    outside = output / "outside"
    outside.mkdir()
    wrapper = output / "rustc-two-threads"
    wrapper.write_text('#!/bin/sh\nexec rustc -C link-arg=-Wl,--threads=2 "$@"\n')
    wrapper.chmod(0o700)
    env = dict(os.environ, RUSTUP_TOOLCHAIN="1.98.1", PYTHONDONTWRITEBYTECODE="1", CGX2_MEMORY_EVIDENCE="1")
    records = []
    assertions = []
    artifact_paths = sorted(p for p in binary.iterdir() if p.is_file())
    before = {p.name: digest(p) for p in artifact_paths}

    def run(name, argv, cwd=ROOT, extra=None, suite=False):
        result = supervise([str(a) for a in argv], cwd=str(cwd), env=dict(env, **(extra or {})), limits=suite_limits if suite else limits, cap=2 * 1024**2, timeout=3600 if suite else 180)
        record = dict(name=name, argv=[str(a) for a in argv],cwd=str(cwd),extra_env=extra or {},**evidence(result))
        records.append(record)
        return record

    def check(name, condition):
        assertions.append({"name": name, "passed": bool(condition)})

    manifest_path=binary.parent / "frozen.json"
    if manifest_path.is_file():
        manifest=json.loads(manifest_path.read_text())
        expected=dict(manifest["binary_sha256"],**manifest["probe_library_sha256"])
        check("frozen-manifest-matches-input",all(before.get(k)==v for k,v in expected.items()))
    else:
        check("frozen-manifest-matches-input",False)

    for fixture, expected in [("region", "2\n"), ("session", "42\n"), ("guard", "42\n"), ("copy-cycle", "42\n"), ("nested-guards", "42\n")]:
        source = fixture_dir / (fixture + ".cg")
        checked = run(fixture + "-check", [binary / "cgx2", "check", source])
        check(fixture + "-check", normal(checked))
        interpreted = run(fixture + "-interpreter", [binary / "cgx2", "run", source])
        check(fixture + "-value", normal(interpreted) and interpreted["stdout"] == expected)
        observation = observed(interpreted)
        check(fixture + "-roots", observation is not None and observation.get("baseline_roots") == observation.get("final_roots") == 0 and observation.get("cleanup_failures") == 0)
        for mode in ["debug", "release", "standalone"]:
            artifact = output / (fixture + "-" + mode)
            if mode == "standalone":
                rust = outside / (fixture + ".rs")
                emitted = run(fixture + "-emit", [binary / "cgx2c", "emit-rust", source, "--out", rust])
                check(fixture + "-emit", normal(emitted))
                built = run(fixture + "-standalone-build", [wrapper, "--edition=2021", rust, "-o", artifact], cwd=outside)
            else:
                command = [binary / "cgx2c", "build", source, "--out", artifact, "--rustc", wrapper]
                if mode == "release":
                    command.append("--release")
                built = run(fixture + "-" + mode + "-build", command)
            check(fixture + "-" + mode + "-build", normal(built))
            if normal(built):
                actual = run(fixture + "-" + mode, [artifact], cwd=outside)
                check(fixture + "-" + mode + "-value", normal(actual) and actual["stdout"] == expected)
                check(fixture + "-" + mode + "-memory", observation is not None and observed(actual) is not None and stable(observed(actual)) == stable(observation))
        sentinel = output / (fixture + ".py")
        sentinel.write_bytes(b"original artifact\n")
        original = digest(sentinel)
        rejected = run(fixture + "-python", [binary / "cgx2c", "emit-python", source, "--out", sentinel, "--force"])
        check(fixture + "-python-code", normal(rejected, 1) and "CGE006" in rejected["stderr"])
        check(fixture + "-python-artifact", digest(sentinel) == original)

    for fixture,needle,width in [("cross-region","xs):",2),("fallback-resource","memory.default_region()",len("memory.default_region()"))]:
        source = fixture_dir / (fixture+".cg")
        text = source.read_text()
        start = len(text[:text.index(needle)].encode("utf-8"))
        rejected = run(fixture+"-check", [binary / "cgx2", "--diagnostic-format=json", "check", source])
        check(fixture+"-exact-diagnostic", rejection(rejected, "CGT095", start, start + width))
        sentinel = output / (fixture+"-native")
        sentinel.write_bytes(b"original native artifact\n")
        original = digest(sentinel)
        rejected = run(fixture+"-build", [binary / "cgx2c", "build", source, "--out", sentinel, "--force"])
        check(fixture+"-build-rejected", normal(rejected, 1) and "CGT095" in rejected["stderr"])
        check(fixture+"-build-preserved", digest(sentinel) == original)

    session = fixture_dir / "session.cg"
    probe_source=output / "entry-probe.rs"
    probe_source.write_text('extern crate cgx2_cli; fn main(){let source=std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();match cgx2_cli::run_checked_source_with_failure_at_reserve(&source,&[],1){Ok((result,_))=>println!("{}",result.value),Err(errors)=>{for failure in errors {eprintln!("{}: {} {:?}",failure.error.code,failure.error.message,failure.error.span);}std::process::exit(1)}}}')
    probe=output / "entry-probe"
    built=run("session-entry-probe-build",[wrapper,"--edition=2021",probe_source,"--extern",f"cgx2_cli={binary / 'libcgx2_cli.rlib'}","-L",f"dependency={binary / 'deps'}","-o",probe])
    check("session-entry-probe-build",normal(built))
    interpreted = run("session-entry-fault", [probe, session])
    check("session-entry-fault-value", normal(interpreted) and interpreted["stdout"] == "-10\n")
    check("session-entry-fault-unavailable-metrics", observed(interpreted) == {"session_entered": False, "metrics": None})
    for mode in ["debug", "release", "standalone"]:
        artifact = output / ("session-entry-fault-" + mode)
        command = [binary / "cgx2c", "build", session, "--out", artifact, "--test-fail-reserve", "1", "--rustc", wrapper]
        if mode == "release":
            command.append("--release")
        if mode=="standalone":
            command=[wrapper,"--edition=2021",'--cfg=feature="fault-injection"',outside / "session.rs","-o",artifact]
        built = run("session-entry-fault-" + mode + "-build", command,extra={"CGX2_TEST_FAIL_RESERVE":"1"} if mode=="standalone" else None)
        check("session-entry-fault-" + mode + "-build", normal(built))
        if normal(built):
            actual = run("session-entry-fault-" + mode, [artifact], cwd=outside)
            check("session-entry-fault-" + mode + "-value", normal(actual) and actual["stdout"] == "-10\n")
            check("session-entry-fault-" + mode + "-memory", observed(actual) == observed(interpreted))

    # The package certificate rechecks source snapshots before publication.
    package = output / "package"
    package.mkdir()
    (package / "cgx2.toml").write_text('schema_version = 1\nname = "memory"\nentry = "main.cg"\nsources = ["main.cg", "lib.cg"]\n')
    (package / "lib.cg").write_text('module app.lib\npub fn build<R: memory.RegionName>(r: shared<memory.Region<R>>) -> Result<memory.Value<R, List<i64>>, AllocationError> effects(ManagedAlloc):\n    memory.try_in(r, [20, 22])\n')
    (package / "main.cg").write_text('module app.main\nimport app.lib\nfn scalar() -> i64:\n    42\nfn main() -> i64 effects(ManagedAlloc):\n    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n            match app.lib.build(borrow(r)):\n                Result.Ok(xs):\n                    length(xs)\n                Result.Err(error):\n                    -2\n        Result.Err(error):\n            -1\n')
    package_snapshot = output / "package-snapshot"
    package_snapshot.mkdir()
    for path in package.iterdir():
        (package_snapshot / path.name).write_bytes(path.read_bytes())
    checked = run("package-check", [binary / "cgx2", "--diagnostic-format=json", "check", package])
    check("package-check", normal(checked))
    interpreted = run("package-interpreter", [binary / "cgx2", "run", package])
    check("package-value", normal(interpreted) and interpreted["stdout"] == "2\n")
    baseline = observed(interpreted)
    for mode in ["debug", "release", "standalone"]:
        artifact = output / ("package-" + mode)
        if mode == "standalone":
            generated = outside / "package.rs"
            emitted = run("package-emit", [binary / "cgx2c", "emit-rust", "--package", package, "--out", generated])
            check("package-emit", normal(emitted))
            command = [wrapper, "--edition=2021", generated, "-o", artifact]
        else:
            command = [binary / "cgx2c", "build", "--package", package, "--out", artifact, "--rustc", wrapper]
            if mode == "release": command.append("--release")
        built = run("package-" + mode + "-build", command)
        check("package-" + mode + "-build", normal(built))
        if normal(built):
            actual = run("package-" + mode + "-run", [artifact])
            check("package-" + mode + "-value", normal(actual) and actual["stdout"] == "2\n")
            check("package-" + mode + "-memory", baseline is not None and observed(actual) is not None and stable(observed(actual)) == stable(baseline))
    for target in [package, fixture_dir / "region.cg"]:
        metadata = run("memory-api-" + target.name, [binary / "cgx2", "memory-effects", target])
        try: index = json.loads(metadata["stdout"])
        except (ValueError, TypeError): index = {}
        check("memory-api-version-" + target.name, normal(metadata) and index.get("schema_version") == 2 and index.get("absent_summary") == "unknown")
        functions = {f["name"]: f["memory_effects_v2"] for f in index.get("functions", [])}
        check("memory-api-region-" + target.name, functions.get("main", {}).get("may_create_region") is True)
        if target == package:
            check("memory-api-source-bundle", index.get("provenance_kind") == "source-bundle-v1" and index.get("source_sha256") is None and functions.get("app.main.scalar", {}).get("may_create_region") is False)
    sentinel = output / "package.pyz"
    sentinel.write_bytes(b"original package artifact\n")
    original = digest(sentinel)
    rejected = run("package-python", [binary / "cgx2c", "emit-python", "--package", package, "--out", sentinel, "--force"])
    check("package-python-rejected", normal(rejected, 1) and "CGE006" in rejected["stderr"])
    check("package-python-preserved", digest(sentinel) == original)
    # A real rustc wrapper mutates a checked input during compilation. The
    # resulting binary is never published over the existing output.
    stale_wrapper = output / "rustc-stale-input"
    stale_wrapper.write_text('#!/usr/bin/env python3\nimport os,sys\nfrom pathlib import Path\np=Path(' + repr(str(package / "lib.cg")) + ')\np.write_text(p.read_text()+"# changed after validation\\n")\nos.execvp("rustc", ["rustc", "-C", "link-arg=-Wl,--threads=2", *sys.argv[1:]])\n')
    stale_wrapper.chmod(0o700)
    sentinel = output / "package-stale-native"
    sentinel.write_bytes(b"original native artifact\n")
    original = digest(sentinel)
    stale = run("package-stale-build", [binary / "cgx2c", "build", "--package", package, "--out", sentinel, "--force", "--rustc", stale_wrapper])
    check("package-stale-rejected", normal(stale, 1) and "CGP018" in stale["stderr"])
    check("package-stale-preserved", digest(sentinel) == original)
    if args.regressions:
        regression_root = output / "regressions"
        regression_root.mkdir()
        specification = json.loads((ROOT / "plans/release-completion/s2b-ordinary-memory/regressions.json").read_text())
        for row in specification["checks"]:
            argv = [a.replace("${SOURCE_ROOT}", str(ROOT)).replace("${BINARY_DIR}", str(binary)).replace("${OUTPUT_DIR}", str(regression_root)) for a in row["argv"]]
            record = run("regression-" + row["name"], argv, suite=True)
            check("regression-" + row["name"], normal(record, row["expected_exit"]))
            if row["name"]=="owned-result-original":
                report_path=regression_root / "owned-result-original/owned-result-original.json"
                try:
                    original=json.loads(report_path.read_text())
                except (OSError,json.JSONDecodeError):
                    original={}
                check("historical-failure-is-only-the-unchanged-shadow-negative",historical_shadow_failure(original))
    after = {p.name: digest(p) for p in artifact_paths}
    check("frozen-artifacts-unchanged", before == after)
    check("fixture-inputs-unchanged", fixture_before == {p.name:digest(p) for p in (HERE / "fixtures").glob("*.cg")})
    report = {"schema_version": 1, "runner_sha256": hashlib.sha256(runner_source).hexdigest(), "ordinary_memory_complete": False,
              "passed": sum(c["passed"] for c in assertions), "failed": sum(not c["passed"] for c in assertions),
              "checks": assertions, "commands": records, "frozen_before": before, "frozen_after": after,
              "fixture_sha256": {p.name: digest(p) for p in fixture_dir.glob("*.cg")},
              "package_input_sha256": {p.name: digest(p) for p in package_snapshot.iterdir()},
              "package_after_fault_sha256": {p.name:digest(p) for p in package.iterdir()},
              "limits": "180s wall / 120s CPU / 4GiB address space / 512MiB files / 2MiB per output stream; process-group cleanup",
              "missing_required_rows": ["general aggregate ownership source lowering and v3 authentication", "source copy_owned/promote and failed-owner continuation", "complete aggregate ownership package integration and exact static dependency source diagnostics", "complete source/package fault, cleanup, and parity acceptance", "external leak-enabled assurance and independent audit"]}
    (output / "product.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"passed": report["passed"], "failed": report["failed"], "ordinary_memory_complete": False}))
    return int(report["failed"] != 0)

if __name__ == "__main__":
    raise SystemExit(main())
