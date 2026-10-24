#!/usr/bin/env python3
"""Bounded executable ordinary-owned slice; full memory checkpoint remains open."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import signal
import shlex
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
RUNTIME = ["lib.rs", "budget.rs", "heap.rs", "graph.rs", "scope.rs", "fault.rs", "owned.rs", "region.rs", "report.rs"]
CAP = 1024 * 1024


def limits():
    resource.setrlimit(resource.RLIMIT_AS, (2 * 1024**3, 2 * 1024**3))
    resource.setrlimit(resource.RLIMIT_CPU, (60, 60))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    binaries = args.binary_dir.resolve()
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    records = []
    fixtures = HERE / "fixtures"
    report = {"schema_version": 1, "checkpoint_complete": False, "release_complete": False,
              "scope": "first executable local ordinary-owned function slice", "commands": records,
              "fixture_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(fixtures.glob("*.cg"))}}
    try:
        with tempfile.TemporaryDirectory(prefix="ordinary-owned-", dir=os.environ["TMPDIR"]) as temporary:
            work = Path(temporary)

            def run(label, command, rc=0, code=None):
                command = [str(c) for c in command]
                stdout, stderr = work / "stdout", work / "stderr"
                with stdout.open("wb") as out, stderr.open("wb") as err:
                    child = subprocess.Popen(command, cwd=work, env={**os.environ, "CGX2_OWNED_EVIDENCE": "1"},
                                             stdout=out, stderr=err, start_new_session=True, preexec_fn=limits)
                    try:
                        actual = child.wait(timeout=120)
                    except subprocess.TimeoutExpired:
                        os.killpg(child.pid, signal.SIGKILL)
                        child.wait()
                        raise AssertionError(f"{label}: process exceeded 120 seconds")
                assert stdout.stat().st_size <= CAP and stderr.stat().st_size <= CAP, label
                out, err = stdout.read_text(), stderr.read_text()
                passed = actual == rc and (code is None or code in err)
                records.append({"label": label, "command": command, "exit": actual,
                                "stdout": out, "stderr": err, "passed": passed})
                assert passed, (label, actual, out, err)
                return out, err

            def assertion(label, condition):
                records.append({"label": label, "passed": bool(condition)})
                assert condition, label

            def evidence(stderr):
                return json.loads(next(line.split(" ", 1)[1] for line in stderr.splitlines() if line.startswith("CGX2_OWNED_EVIDENCE ")))

            compiler, _ = run("pinned-rustc", ["rustup", "which", "rustc"])
            wrapper = work / "rustc-bounded-linker"
            wrapper.write_text('#!/bin/sh\nexec ' + shlex.quote(compiler.strip()) + ' -Clink-arg=-Wl,--threads=2 "$@"\n')
            wrapper.chmod(0o700)
            report["compiler"] = {"path": compiler.strip(), "linker_threads": 2}
            # Independently stage, hash and compile the complete reviewed runtime inventory.
            runtime = work / "runtime"
            runtime.mkdir()
            inventory = {}
            for name in RUNTIME:
                contents = (ROOT / "crates/cgx2-runtime/src" / name).read_bytes()
                (runtime / name).write_bytes(contents)
                inventory[name] = hashlib.sha256(contents).hexdigest()
            report["runtime_inventory_sha256"] = inventory
            rlib = work / "libcgx2_runtime.rlib"
            run("standalone-runtime", [wrapper, "--edition=2021", "--crate-name", "cgx2_runtime", "--crate-type", "rlib", runtime / "lib.rs", "-o", rlib])
            positive = {"functions": ("37\n", 4, 18), "mapping": ("22\n", 3, 16), "copied-record": ("R(x: 3, yes: true)\n", 1, 9), "exclusive-result": ("42\n", 1, 8),
                        "repeated": ("99\n", 100, 8), "arithmetic-failure": (None, 2, 18),
                        "constructor-order": (None, 0, 0)}
            for name, (expected, allocations, peak) in positive.items():
                source = fixtures / f"{name}.cg"
                run(name + ":check", [binaries / "cgx2", "check", source])
                out, err = run(name + ":interpreter", [binaries / "cgx2", "run", source],
                               rc=0 if expected else 1, code=None if expected else "CGR080")
                accounting = evidence(err)
                assertion(name + ":source-storage", accounting["allocations"] == allocations
                          and accounting["releases"] == allocations and accounting["live_bytes"] == 0
                          and accounting["allocated_bytes"] == accounting["released_bytes"]
                          and accounting["peak_live_bytes"] == peak)
                if expected:
                    assertion(name + ":value", out == expected)
                else:
                    snippet = "p.x / n" if name == "arithmetic-failure" else "1 / 0"
                    start = source.read_bytes().index(snippet.encode())
                    assertion(name + ":failure-span", f"{start}..{start + len(snippet)}" in err)
                for release in [False, True]:
                    binary = work / (name + ("-release" if release else "-debug"))
                    run(binary.name + ":build", [binaries / "cgx2", "build", source, "--out", binary, "--rustc", wrapper] + (["--release"] if release else []))
                    out, err = run(binary.name + ":execute", [binary], rc=0 if expected else 1, code=None if expected else "CGR080")
                    assertion(binary.name + ":parity", evidence(err) == accounting and (out == expected if expected else f"{start}..{start + len(snippet)}" in err))
                emitted, binary = work / (name + ".rs"), work / (name + "-standalone")
                run(name + ":emit-rust", [binaries / "cgx2", "emit", source, "--target", "rust", "--out", emitted])
                run(name + ":standalone-compile", [wrapper, "--edition=2024", emitted, "--extern", f"cgx2_runtime={rlib}", "-o", binary])
                out, err = run(name + ":standalone-run", [binary], rc=0 if expected else 1, code=None if expected else "CGR080")
                assertion(name + ":standalone-parity", evidence(err) == accounting and (out == expected if expected else f"{start}..{start + len(snippet)}" in err))
                python = work / (name + ".py")
                python.write_bytes(b"previous artifact\n")
                run(name + ":python-rejection", [binaries / "cgx2", "emit", source, "--target", "python", "--out", python, "--force"], rc=1, code="CGE006")
                assertion(name + ":python-preserved", python.read_bytes() == b"previous artifact\n")

            for name, expected in json.loads((HERE / "expected.json").read_text()).items():
                source = fixtures / f"{name}.cg"
                _, err = run(name + ":reject", [binaries / "cgx2", "check", source, "--diagnostic-format=json"], rc=1, code=expected["code"])
                diagnostic = json.loads(err.splitlines()[0])
                assertion(name + ":exact-utf8-span", diagnostic["code"] == expected["code"] and diagnostic["span"] == expected["span"])
                run(name + ":reject-run", [binaries / "cgx2", "run", source], rc=1, code=expected["code"])
                for target in ["rust", "python", "binary"]:
                    artifact = work / (name + "." + target)
                    artifact.write_bytes(b"previous artifact\n")
                    command = [binaries / "cgx2", "build", source] if target == "binary" else [binaries / "cgx2", "emit", source, "--target", target]
                    run(name + ":reject-" + target, command + ["--out", artifact, "--force"], rc=1, code=expected["code"])
                    assertion(name + ":preserved-" + target, artifact.read_bytes() == b"previous artifact\n")

            source = fixtures / "functions.cg"
            run("api-reject", [binaries / "cgx2", "api", source, "--json"], rc=1, code="CGT080")
            run("effects-reject", [binaries / "cgx2-effect-index", source, "--syntax-only"], rc=1, code="CGI006")
            proof, _ = run("proof-unknown", [binaries / "cgx2-verify", source, "--format", "json"])
            proof = json.loads(proof)
            assertion("no-false-proof", proof["verdict"]["kind"] == "unknown" and proof["proof"]["state"] == "unsupported")
            policy = work / "policy.json"
            policy.write_text(json.dumps({"schema_version": 1, "allowed_effects": [], "allow_unknown": False}))
            run("security-reject", [binaries / "cgx2-security", "audit", source, "--policy", policy], rc=2, code="CGI006")
            notebook = work / "owned.cgnb"
            notebook.write_text("#!cgnb 1\n%% code id=owned\n" + source.read_text().replace("module owned_demo\n", ""))
            run("notebook-reject", [binaries / "cgx2-notebook", "run", notebook, "--out", work / "out.cgnb"], rc=1, code="CGNK026")
            package = work / "package"
            package.mkdir()
            (package / "cgx2.toml").write_text('schema_version = 1\nname = "owned_demo"\nentry = "main.cg"\nsources = ["main.cg"]\n')
            (package / "main.cg").write_text(source.read_text())
            run("package-reject", [binaries / "cgx2", "check", package], rc=1, code="CGT080")
    finally:
        report["passed"] = sum(r["passed"] for r in records)
        report["failed"] = sum(not r["passed"] for r in records)
        output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"ordinary-owned: {report['passed']} passed; checkpoint_complete=false; release_complete=false")


if __name__ == "__main__":
    main()
