#!/usr/bin/env python3
"""Bounded core repair checks; immutable attempts, no release/performance claim."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import shutil
import shlex
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE.parent / "owned-result-runner-evidence"))
from supervisor import supervise, evidence


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    binaries = args.binary_dir.resolve()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if (output / "product.json").exists():
        raise SystemExit("use a new output directory; previous attempts are immutable")
    records = []
    report = {"schema_version": 1, "checks": records, "release_complete": False,
              "managed_acceptance": "coordinator-review-required", "full_memory_complete": False,
              "bounds": {"wall_seconds": 180, "cpu_seconds_per_process": 120,
                         "address_space_bytes_per_process": 4 * 1024**3,
                         "file_bytes_per_process": 512 * 1024**2,
                         "retained_bytes_per_stream": 2 * 1024**2,
                         "aggregate_cgroup_isolation": False},
              "binary_sha256": {name: digest(binaries / name) for name in ["cgx2", "cgx2c"]},
              "source_sha256": {p.name: digest(p) for p in HERE.glob("*.cg")},
              "runner_sha256": digest(Path(__file__)),
              "runtime_sha256": {str(p.relative_to(ROOT)): digest(p) for p in sorted((ROOT / "crates/cgx2-runtime/src").rglob("*")) if p.is_file()}}
    env = {**os.environ, "RUSTUP_TOOLCHAIN": "1.98.1", "CGX2_MANAGED_EVIDENCE": "1"}

    def save():
        report.update(passed=sum(r["passed"] for r in records), failed=sum(not r["passed"] for r in records))
        (output / "product.json").write_text(json.dumps(report, indent=2) + "\n")

    def limits():
        resource.setrlimit(resource.RLIMIT_AS, (4 * 1024**3, 4 * 1024**3))
        resource.setrlimit(resource.RLIMIT_CPU, (120, 120))
        resource.setrlimit(resource.RLIMIT_FSIZE, (512 * 1024**2, 512 * 1024**2))

    def command(label, argv, cwd, rc=0, extra_env=None):
        result = supervise([str(a) for a in argv], cwd=cwd, env={**env, **(extra_env or {})},
                           limits=limits, cap=2 * 1024**2, timeout=180)
        record = {"label": label, "command": [str(a) for a in argv], "cwd": str(cwd),
                  "extra_env": extra_env or {}, **evidence(result)}
        record["passed"] = result["reason"] == "exit" and result["exit"] == rc
        records.append(record)
        save()
        assert record["passed"], record
        return record

    def check(label, condition):
        records.append({"label": label, "passed": bool(condition)})
        save()
        assert condition, label

    with tempfile.TemporaryDirectory(prefix="managed-auth-repair-", dir=os.environ["TMPDIR"]) as temp:
        work = Path(temp)
        rustc = command("pinned-rustc", ["rustup", "which", "rustc"], work)["stdout"].strip()
        report["rustc_identity"] = command("rustc-version", [rustc, "-vV"], work)["stdout"]
        wrapper = work / "rustc-bounded-linker"
        wrapper.write_text("#!/bin/sh\nexec " + shlex.quote(rustc) + ' -Clink-arg=-Wl,--threads=2 "$@"\n')
        wrapper.chmod(0o700)
        cli, compiler = binaries / "cgx2", binaries / "cgx2c"
        for name, expected in [("nominal-plain", "42\n"), ("nominal", "42\n"), ("recovery", "42\n"), ("nominal-package", "42\n")]:
            source = HERE / name if name.endswith("package") else HERE / (name + ".cg")
            check(name + ":interpreter", command(name + ":run", [cli, "run", source], work)["stdout"] == expected)
            emitted = work / (name + ".rs")
            command(name + ":emit", [cli, "emit", source, "--target", "rust", "--out", emitted], work)
            if source.is_file():
                check(name + ":source-hash", f"original source sha256: {digest(source)};" in emitted.read_text().splitlines()[0])
            else:
                index = json.loads(command(name + ":memory-index", [cli, "memory-effects", source], work)["stdout"])
                check(name + ":bundle-hash", index["source_sha256"] is None and index["provenance_kind"] == "source-bundle-v1" and index["inputs_sha256"] in emitted.read_text().splitlines()[0])
                report["package_sources_sha256"] = {p.name: digest(p) for p in source.iterdir() if p.is_file()}
            for release in [False, True]:
                artifact = work / f"{name}-{release}"
                built = command(artifact.name + ":build", [cli, "build", source, "--out", artifact, "--rustc", wrapper] + (["--release"] if release else []), work)
                if source.is_file():
                    check(artifact.name + ":metadata-source-hash", f"source_sha256={digest(source)}" in built["stdout"])
                else:
                    check(artifact.name + ":metadata-source-hash", all(digest(p) in built["stdout"] for p in source.iterdir() if p.is_file()))
                check(artifact.name + ":value", command(artifact.name + ":execute", [artifact], work)["stdout"] == expected)
                report.setdefault("artifact_sha256", {})[artifact.name] = digest(artifact)
            artifact = work / (name + "-standalone")
            command(name + ":standalone-compile", [wrapper, "--edition=2024", emitted, "-o", artifact], work)
            check(name + ":standalone-value", command(name + ":standalone-execute", [artifact], work)["stdout"] == expected)
            report["artifact_sha256"][emitted.name] = digest(emitted)
            report["artifact_sha256"][artifact.name] = digest(artifact)

        # Public interpreter fault API and native/standalone test hook, same UTF-8 bytes.
        probe = work / "entry-probe"
        command("entry-probe:compile", [wrapper, "--edition=2021", HERE / "entry-probe.rs", "--extern", f"cgx2_cli={binaries / 'libcgx2_cli.rlib'}", "-L", f"dependency={binaries / 'deps'}", "-o", probe], work)
        for name, code in [("entry-managed", "CGR090"), ("entry-hybrid", "CGR080")]:
            source = HERE / (name + ".cg")
            expected = command(name + ":interpreter-entry", [probe, source], work)["stdout"].strip()
            span = expected.split(":")[1]
            data = source.read_bytes()
            start = data.index(b"fn main")
            check(name + ":original-utf8-byte-span", expected == f"{code}:{start}..{len(data)}")
            for release in [False, True]:
                artifact = work / f"{name}-{release}"
                command(artifact.name + ":build", [compiler, "build", source, "--out", artifact, "--rustc", wrapper, "--test-fail-reserve", "1"] + (["--release"] if release else []), work)
                error = command(artifact.name + ":failure", [artifact], work, rc=1)
                check(artifact.name + ":exact-entry-diagnostic", error["stderr"].strip() == f"{code}: {'managed session entry failed' if code == 'CGR090' else 'session allocation failed'}: HostAllocation {{ site: SessionRegions }} at {span}")
                report["artifact_sha256"][artifact.name] = digest(artifact)
            emitted = work / (name + ".rs")
            artifact = work / (name + "-standalone")
            command(name + ":emit", [cli, "emit", source, "--target", "rust", "--out", emitted], work)
            command(name + ":standalone-compile", [wrapper, "--edition=2024", '--cfg=feature="fault-injection"', emitted, "-o", artifact], work, extra_env={"CGX2_TEST_FAIL_RESERVE": "1"})
            failure = command(name + ":standalone-failure", [artifact], work, rc=1)
            check(name + ":standalone-entry-span", f"{code}:" in failure["stderr"] and failure["stderr"].strip().endswith(" at " + span))
            report["artifact_sha256"][emitted.name] = digest(emitted)
            report["artifact_sha256"][artifact.name] = digest(artifact)

        source = HERE / "wrong-error.cg"
        rejected = command("wrong-error:check", [cli, "--diagnostic-format=json", "check", source], work, rc=1)
        diagnostics = [json.loads(line) for line in rejected["stderr"].splitlines()]
        data = source.read_bytes()
        start = data.index(b"managed.try_alloc(42)")
        check("wrong-error:code-and-span", any(d["code"] == "CGT003" and d["span"] == {"start": start, "end": start + len(b"managed.try_alloc(42)")} for d in diagnostics))
        for mode in ["debug", "release", "rust", "python"]:
            artifact = work / ("sentinel-" + mode)
            artifact.write_bytes(b"previous artifact\n")
            argv = [cli, "build", source, "--out", artifact, "--force", "--rustc", wrapper] + (["--release"] if mode == "release" else []) if mode in ["debug", "release"] else [cli, "emit", source, "--target", mode, "--out", artifact, "--force"]
            command("wrong-error:" + mode, argv, work, rc=1)
            check("wrong-error:" + mode + ":artifact-preserved", artifact.read_bytes() == b"previous artifact\n")

        # Mutate the original source during an actual rustc invocation. A successful
        # target compile cannot authorize replacing an artifact from stale input.
        for release in [False, True]:
            source = work / "drifting.cg"
            source.write_bytes((HERE / "nominal.cg").read_bytes())
            artifact = work / f"stale-{release}"
            artifact.write_bytes(b"previous artifact\n")
            mutator = work / "rustc-mutate-source"
            mutator.write_text(f"#!{sys.executable}\nimport os, pathlib, sys\np=pathlib.Path({str(source)!r})\nif any(a.endswith('generated.rs') for a in sys.argv[1:]): p.write_bytes(p.read_bytes()+b'# changed after checking\\n')\nos.execv({rustc!r}, [{rustc!r}, '-Clink-arg=-Wl,--threads=2', *sys.argv[1:]])\n")
            mutator.chmod(0o700)
            failed = command(f"stale-source-{release}:build", [compiler, "build", source, "--out", artifact, "--force", "--rustc", mutator] + (["--release"] if release else []), work, rc=1)
            check(f"stale-source-{release}:diagnostic", "CGXC009" in failed["stderr"])
            check(f"stale-source-{release}:artifact-preserved", artifact.read_bytes() == b"previous artifact\n")
        for release in [False, True]:
            package = work / f"drifting-package-{release}"
            shutil.copytree(HERE / "nominal-package", package)
            changed = package / "lib.cg"
            artifact = work / f"stale-package-{release}"
            artifact.write_bytes(b"previous artifact\n")
            mutator = work / "rustc-mutate-import"
            mutator.write_text(f"#!{sys.executable}\nimport os, pathlib, sys\np=pathlib.Path({str(changed)!r})\nif any(a.endswith('generated.rs') for a in sys.argv[1:]): p.write_bytes(p.read_bytes()+b'# changed imported source\\n')\nos.execv({rustc!r}, [{rustc!r}, '-Clink-arg=-Wl,--threads=2', *sys.argv[1:]])\n")
            mutator.chmod(0o700)
            failed = command(f"stale-package-{release}:build", [compiler, "build", "--package", package, "--out", artifact, "--force", "--rustc", mutator] + (["--release"] if release else []), work, rc=1)
            check(f"stale-package-{release}:diagnostic", "CGP018" in failed["stderr"])
            check(f"stale-package-{release}:artifact-preserved", artifact.read_bytes() == b"previous artifact\n")

        # Run the existing layout assertions from a private migrated copy. The
        # historical test itself remains untouched and its original failure is
        # retained separately. Only the obsolete constructor setup is replaced.
        historical = ROOT / "crates/cgx2-emit-rust/tests/s2a_record_identities.rs"
        original = historical.read_text()
        start = original.index("        let entry = package")
        end = original.index("        assert!(generated.contains", start)
        migrated = work / "record-identities-migrated.rs"
        migrated.write_text(original[:start] + "        let program = package.managed_program_v1().unwrap();\n" + original[end:])
        report["historical_layout_test_sha256"] = digest(historical)
        report["migrated_layout_test_sha256"] = digest(migrated)
        artifact = work / "record-identities-migrated"
        argv = [wrapper, "--test", "--edition=2021", migrated, "-L", f"dependency={binaries / 'deps'}", "-o", artifact]
        for crate in ["cgx2_package", "cgx2_sema", "cgx2_emit_rust"]:
            argv += ["--extern", f"{crate}={binaries / ('lib' + crate + '.rlib')}"]
        command("migrated-layout-test:compile", argv, work)
        command("migrated-layout-test:all-original-assertions", [artifact], ROOT / "crates/cgx2-emit-rust")
        save()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
