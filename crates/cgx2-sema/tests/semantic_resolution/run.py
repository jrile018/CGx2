#!/usr/bin/env python3
"""Actual product regressions; retain failed cross-owner gates in the report."""
import argparse
import json
import os
from pathlib import Path
import resource
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
NAMES = ("length", "show", "parse_i64", "array", "list", "get", "append",
         "concat", "updated", "slice")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    options = parser.parse_args()
    binary = options.binary.resolve()
    records = []

    with tempfile.TemporaryDirectory(prefix="resolution-", dir=os.environ["TMPDIR"]) as tmp:
        scratch = Path(tmp).resolve()

        def run(label, command, expected=None, error=None, artifact=None, expansion=False,
                cross_lane=None, native_compiler=False):
            def limits():
                if not native_compiler:
                    resource.setrlimit(resource.RLIMIT_AS, (512 * 1024**2, 512 * 1024**2))
                cpu = 10 if expansion else 15
                resource.setrlimit(resource.RLIMIT_CPU, (cpu, cpu))

            command = [str(x) for x in command]
            before = artifact.read_bytes() if artifact else None
            try:
                result = subprocess.run(command, cwd=scratch, capture_output=True, text=True,
                                        timeout=20, preexec_fn=limits)
                passed = ((result.returncode == 0 and
                           (expected is None or result.stdout == expected)) if error is None
                          else result.returncode > 0 and error in result.stderr)
                if artifact:
                    passed = passed and artifact.read_bytes() == before
                record = dict(name=label, command=command, exit=result.returncode,
                              stdout=result.stdout, stderr=result.stderr[-16000:], passed=passed)
            except (subprocess.TimeoutExpired, OSError) as failure:
                record = dict(name=label, command=command, passed=False, failure=str(failure))
            if artifact:
                record["artifact_preserved"] = artifact.read_bytes() == before
            if cross_lane:
                record["cross_lane_if_failed"] = cross_lane
            records.append(record)
            return record["passed"]

        positives = [("original", HERE / "constructor-shadow.cg", "42\n", True),
                     ("qualified-binder", HERE / "qualified-binder.cg", "42\n", False),
                     ("imported-package", HERE / "package", "63\n", False)]
        for name in NAMES:
            for qualified in (False, True):
                label = f'{name}-{"qualified" if qualified else "local"}'
                source = scratch / (label + ".cg")
                prefix = "module model\n" if qualified else ""
                source.write_text(f"{prefix}record {name}:\n    value: i64\n"
                                  f"fn main() -> i64:\n    let x = {name}(value: 42)\n"
                                  "    x.value\n")
                positives.append((label, source, "42\n", not qualified))

        for label, source, expected, collision in positives:
            run(label + ":check", [binary, "check", source])
            run(label + ":run", [binary, "run", source], expected)
            for release in (False, True):
                profile = "release" if release else "debug"
                executable = scratch / (label + "-" + profile)
                command = [binary, "build", source, "--out", executable]
                if release:
                    command.append("--release")
                cross_lane = ("crates/cgx2-emit-rust/src/data.rs: expression() dispatches "
                              "intrinsic spelling despite checked record identity") if collision else None
                if run(label + ":build-" + profile, command, cross_lane=cross_lane,
                       native_compiler=True):
                    run(label + ":native-" + profile, [executable], expected)
            emitted = scratch / (label + (".pyz" if source.is_dir() else ".py"))
            if run(label + ":emit-python", [binary, "emit", source, "--target", "python",
                                            "--out", emitted]):
                run(label + ":python", ["python3", emitted], expected)
            rust = scratch / (label + ".rs")
            if run(label + ":emit-rust", [binary, "emit", source, "--target", "rust", "--out", rust]):
                executable = scratch / (label + "-rust")
                if run(label + ":rustc", ["rustc", "--edition=2021", "--crate-name", "fixture",
                                         rust, "-o", executable], cross_lane=cross_lane,
                       native_compiler=True):
                    run(label + ":emitted-rust", [executable], expected)

        for fixture in ("binder-constructor.cg", "binder-variant.cg", "binder-nested.cg",
                        "invalid-package"):
            source = HERE / fixture
            for action in ("check", "run"):
                run(fixture + ":" + action, [binary, action, source], error="CGT002")
            for target in ("rust", "python", "native-debug", "native-release"):
                artifact = scratch / ("sentinel-" + target)
                artifact.write_bytes(b"keep existing artifact\n")
                if target.startswith("native"):
                    command = [binary, "build", source, "--out", artifact, "--force"]
                    if target.endswith("release"):
                        command.append("--release")
                else:
                    command = [binary, "emit", source, "--target", target, "--out", artifact, "--force"]
                run(fixture + ":reject-" + target, command, error="CGT002", artifact=artifact,
                    native_compiler=target.startswith("native"))

        run("type-expansion-14", [binary, "check", HERE / "type-expansion.cg"],
            error="CGT030", expansion=True)

    report = dict(schema_version=1, lane="semantic-resolution", release_complete=False,
                  subprocess_limits=dict(address_space_bytes=512 * 1024**2, cpu_seconds=15,
                                         expansion_cpu_seconds=10, wall_seconds=20,
                                         native_compiler_address_space_bytes=None),
                  passed=sum(r["passed"] for r in records),
                  failed=sum(not r["passed"] for r in records), records=records)
    options.output.parent.mkdir(parents=True, exist_ok=True)
    options.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k: report[k] for k in ("passed", "failed")}))
    return int(report["failed"] > 0)


if __name__ == "__main__":
    raise SystemExit(main())
