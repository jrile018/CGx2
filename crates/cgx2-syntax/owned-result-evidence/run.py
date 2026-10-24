#!/usr/bin/env python3
"""Bounded executable ordinary-owned slice; full memory checkpoint remains open."""
import argparse
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
from supervisor import evidence as command_evidence, supervise
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
    report = {"schema_version": 2, "checkpoint_complete": False, "release_complete": False,
              "scope": "recoverable owned Result and bounded no_gc", "commands": records,
              "limits": {"wall_seconds": 120, "cpu_seconds_per_process": 60,
                         "address_space_bytes_per_process": 2 * 1024**3,
                         "retained_bytes_per_stream": CAP, "aggregate_cgroup_isolation": False},
              "fixture_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(fixtures.glob("*.cg"))}}
    try:
        with tempfile.TemporaryDirectory(prefix="owned-result-", dir=os.environ["TMPDIR"]) as temporary:
            work = Path(temporary)

            def run(label, command, rc=0, code=None, extra_env=None):
                command = [str(c) for c in command]
                result = supervise(command, cwd=work,
                                   env={**os.environ, "CGX2_OWNED_EVIDENCE": "1", **(extra_env or {})},
                                   limits=limits, cap=CAP, timeout=120)
                record = {"label": label, "command": command, "expected_exit": rc,
                          "expected_diagnostic": code, **command_evidence(result)}
                actual, out, err = record["exit"], record["stdout"], record["stderr"]
                passed = result["reason"] == "exit" and actual == rc and (code is None or code in err)
                record["passed"] = passed
                records.append(record)
                assert passed, (label, result["reason"], actual, out, err)
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
            ordinary = fixtures / "ordinary-symbol.cg"
            run("ordinary-symbol:check", [binaries / "cgx2", "check", ordinary])
            out, err = run("ordinary-symbol:interpreter", [binaries / "cgx2", "run", ordinary])
            assertion("ordinary-symbol:resolved-user-symbol", out == "42\n" and "CGX2_OWNED_EVIDENCE" not in err)
            for release in [False, True]:
                binary = work / f"ordinary-symbol-{release}"
                run(binary.name + ":build", [binaries / "cgx2", "build", ordinary, "--out", binary, "--rustc", wrapper] + (["--release"] if release else []))
                out, _ = run(binary.name + ":execute", [binary])
                assertion(binary.name + ":value", out == "42\n")
            python = work / "ordinary-symbol.py"
            run("ordinary-symbol:python-emit", [binaries / "cgx2", "emit", ordinary, "--target", "python", "--out", python])
            out, _ = run("ordinary-symbol:python-run", ["python3", python])
            assertion("ordinary-symbol:python-value", out == "42\n")
            positive = json.loads((HERE / "positive.json").read_text())
            for name, spec in positive.items():
                expected, allocations, peak = spec["stdout"], spec["allocations"], spec["peak"]
                source = fixtures / f"{name}.cg"
                run(name + ":check", [binaries / "cgx2", "check", source])
                out, err = run(name + ":interpreter", [binaries / "cgx2", "run", source],
                               rc=0 if expected else 1, code=None if expected else "CGR080")
                accounting = evidence(err)
                assertion(name + ":source-storage", accounting["allocations"] == allocations
                          and accounting["releases"] == allocations and accounting["live_bytes"] == 0
                          and accounting["allocated_bytes"] == accounting["released_bytes"]
                          and accounting["peak_live_bytes"] == peak)
                assertion(name + ":result-guard-observation", all(accounting[k] == spec[k] for k in ["result_ok", "result_err", "guard_entries", "guard_failures"]) and accounting["guard_exits"] == accounting["guard_entries"] and accounting["collection_count"] == 0 and accounting["active_guards_at_exit"] == 0)
                assertion(name + ":actual-runtime-charges", accounting["charged_bytes"] == accounting["owned_charged_bytes"] + accounting["guard_charged_bytes"])
                if expected:
                    assertion(name + ":value", out == expected)
                else:
                    snippet = spec["failure_snippet"]
                    assertion(name + ":primary-failure", spec["failure_message"] in err)
                    start = source.read_bytes().index(snippet.encode())
                    end = spec.get("failure_end", start + len(snippet.encode()))
                    assertion(name + ":failure-span", f"{start}..{end}" in err)
                for release in [False, True]:
                    binary = work / (name + ("-release" if release else "-debug"))
                    run(binary.name + ":build", [binaries / "cgx2", "build", source, "--out", binary, "--rustc", wrapper] + (["--release"] if release else []))
                    out, err = run(binary.name + ":execute", [binary], rc=0 if expected else 1, code=None if expected else "CGR080")
                    assertion(binary.name + ":parity", evidence(err) == accounting and (out == expected if expected else f"{start}..{end}" in err))
                emitted, binary = work / (name + ".rs"), work / (name + "-standalone")
                run(name + ":emit-rust", [binaries / "cgx2", "emit", source, "--target", "rust", "--out", emitted])
                run(name + ":standalone-compile", [wrapper, "--edition=2024", emitted, "--extern", f"cgx2_runtime={rlib}", "-o", binary])
                out, err = run(name + ":standalone-run", [binary], rc=0 if expected else 1, code=None if expected else "CGR080")
                assertion(name + ":standalone-parity", evidence(err) == accounting and (out == expected if expected else f"{start}..{end}" in err))
                python = work / (name + ".py")
                python.write_bytes(b"previous artifact\n")
                run(name + ":python-rejection", [binaries / "cgx2", "emit", source, "--target", "python", "--out", python, "--force"], rc=1, code="CGE006")
                assertion(name + ":python-preserved", python.read_bytes() == b"previous artifact\n")

            fault_rlib = work / "libcgx2_runtime_fault.rlib"
            run("standalone-fault-runtime", [wrapper, "--edition=2021", "--crate-name", "cgx2_runtime", "--crate-type", "rlib", "--cfg", 'feature="fault-injection"', runtime / "lib.rs", "-o", fault_rlib])
            for index, wanted, entries in [(1, None, 0), (2, None, 0), (3, "3\n", 1)]:
                source = fixtures / "host-recovery.cg"
                for release in [False, True]:
                    binary = work / f"injected-{index}-{release}"
                    run(binary.name + ":build", [binaries / "cgx2", "build", source, "--out", binary, "--rustc", wrapper, "--test-fail-reserve", index] + (["--release"] if release else []))
                    out, err = run(binary.name + ":execute", [binary], rc=0 if wanted else 1, code=None if wanted else "CGR080")
                    observed = evidence(err)
                    assertion(binary.name + ":synthetic-host-outcome", (out == wanted if wanted else "HostAllocation" in err) and observed["allocations"] == observed["releases"] and observed["live_bytes"] == 0 and observed["guard_entries"] == entries and observed["guard_exits"] == entries and observed["collection_count"] == 0)
                standalone = work / f"fault-standalone-{index}"
                emitted = work / "host-recovery.rs"
                run(standalone.name + ":compile", [wrapper, "--edition=2024", "--cfg", 'feature="fault-injection"', emitted, "--extern", f"cgx2_runtime={fault_rlib}", "-o", standalone], extra_env={"CGX2_TEST_FAIL_RESERVE": str(index)})
                out, err = run(standalone.name + ":execute", [standalone], rc=0 if wanted else 1, code=None if wanted else "CGR080")
                assertion(standalone.name + ":fault-parity", evidence(err) == observed and (out == wanted if wanted else "HostAllocation" in err))
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

            source = fixtures / "success.cg"
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
    print(f"owned-result: {report['passed']} passed; checkpoint_complete=false; release_complete=false")


if __name__ == "__main__":
    main()
