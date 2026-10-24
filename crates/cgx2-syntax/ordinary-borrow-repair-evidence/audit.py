#!/usr/bin/env python3
"""Audit actual external standalone runs and frozen/source byte identities."""
import hashlib
import json
import os
from pathlib import Path
import resource
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE.parent / "owned-result-runner-evidence"))
from supervisor import supervise, evidence


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def limits():
    for kind, value in [(resource.RLIMIT_AS, 4 * 1024**3), (resource.RLIMIT_CPU, 120),
                        (resource.RLIMIT_FSIZE, 512 * 1024**2), (resource.RLIMIT_NOFILE, 128)]:
        resource.setrlimit(kind, (value, value))


def main():
    frozen, output, *attempts = map(lambda p: Path(p).resolve(), sys.argv[1:])
    output.mkdir(exist_ok=False)
    manifest = json.loads((frozen / "manifest.json").read_text())
    report = dict(commands=[], assertions=[], standalone=[], frozen_manifest_sha256=digest(frozen / "manifest.json"),
                  bounds=dict(wall_seconds=180, cpu_seconds=120, address_space_bytes=4 * 1024**3,
                              file_bytes=512 * 1024**2, output_bytes_per_stream=2 * 1024**2))
    def save():
        report["passed"] = sum(x["passed"] for x in report["assertions"])
        report["failed"] = sum(not x["passed"] for x in report["assertions"])
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    def check(label, condition):
        report["assertions"].append(dict(name=label, passed=bool(condition)))
        save()
    def run(label, argv, cwd):
        row = dict(name=label, argv=list(map(str, argv)), cwd=str(cwd), status="started")
        report["commands"].append(row)
        save()
        result = supervise(row["argv"], cwd=str(cwd), env=dict(os.environ, RUSTUP_TOOLCHAIN="1.98.1"),
                           limits=limits, cap=2 * 1024**2, timeout=180)
        row.update(evidence(result), status="complete")
        save()
        return row
    def normal(row, exit=0):
        return row.get("reason") == "exit" and row.get("exit") == exit and row.get("signal") is None
    def frozen_unchanged(phase):
        for group in ("artifact_sha256", "dependency_sha256"):
            for name, expected in manifest[group].items():
                check(phase + ":" + name, digest(frozen / name) == expected)
        for name, expected in manifest["source_sha256"].items():
            check(phase + ":source:" + name, digest(ROOT / name) == expected)
        for name, info in manifest["toolchain"].items():
            check(phase + ":toolchain:" + name, digest(Path(info["path"])) == info["sha256"])
    frozen_unchanged("before")
    rustc = manifest["toolchain"]["rustc"]["path"]
    for attempt in attempts:
        product = json.loads((attempt / "report.json").read_text())
        builds = [r for r in product["commands"] if r["name"].endswith(":standalone-build")]
        for build in builds:
            name = build["name"].removesuffix(":standalone-build")
            label = attempt.name + ":" + name
            emitted = next(r for r in product["commands"] if r["name"] == name + ":emit")
            execution = next(r for r in product["commands"] if r["name"] == name + ":standalone")
            source = Path(emitted["argv"][emitted["argv"].index("--out") + 1]).resolve()
            cwd = Path(build["cwd"]).resolve()
            executable = Path(execution["argv"][0]).resolve()
            check(label + ":actual-build-and-run", normal(build) and normal(execution))
            # Sandbox writable roots can have empty synthetic .git guard mounts.
            # An empty directory is not Git metadata; retain these observations
            # and additionally require Git's own discovery to return 128 below.
            metadata = [p / ".git" for p in (cwd, *cwd.parents) if (p / ".git").exists()]
            report.setdefault("git_metadata_observations", {})[str(cwd)] = [
                dict(path=str(p), empty_directory=p.is_dir() and not any(p.iterdir())) for p in metadata]
            check(label + ":outside-every-repository", all(p.is_dir() and not any(p.iterdir()) for p in metadata))
            check(label + ":same-external-run-cwd", Path(execution["cwd"]).resolve() == cwd)
            result = run(label + ":git-root", ["git", "rev-parse", "--show-toplevel"], cwd)
            check(label + ":git-no-repository", normal(result, 128) and "not a git repository" in result["stderr"])
            wrapper = Path(build["argv"][0])
            check(label + ":unaugmented-rustc", wrapper.read_text() == '#!/bin/sh\nexec rustc -C link-arg=-Wl,--threads=2 "$@"\n' and "--extern" not in build["argv"] and "-L" not in build["argv"])
            depfile = output / (attempt.name + "-" + name + ".d")
            result = run(label + ":dep-info", [rustc, "--edition=2021", "--emit=dep-info", source, "-o", depfile], cwd)
            check(label + ":dep-info-success", normal(result))
            content = depfile.read_text() if depfile.is_file() else ""
            dependency_lines = [line for line in content.splitlines() if ": " in line]
            check(label + ":only-emitted-source-input", bool(dependency_lines) and all(line.split(": ", 1)[1] == str(source) for line in dependency_lines))
            linked = run(label + ":ldd", ["ldd", executable], cwd)
            check(label + ":no-cgx2-or-foreign-shared-library", normal(linked) and all(token not in linked["stdout"] for token in ["libcgx2", "libpython", "libtorch"]))
            report["standalone"].append(dict(name=label, cwd=str(cwd), source=str(source), source_sha256=digest(source),
                        executable=str(executable), executable_sha256=digest(executable),
                        compiler_path=rustc, compiler_sha256=digest(Path(rustc)), depfile_sha256=digest(depfile)))
            save()
    check("all-nine-admission-standalones-audited", sum(row["name"].startswith("borrow-nominal-admission-") for row in report["standalone"]) == 9)
    frozen_unchanged("after")
    print(json.dumps(dict(passed=report["passed"], failed=report["failed"], standalone=len(report["standalone"]))))
    return bool(report["failed"])


if __name__ == "__main__":
    sys.exit(main())
