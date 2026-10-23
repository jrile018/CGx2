#!/usr/bin/env python3
"""Run acceptance against an operator-trusted, explicitly pinned Z3 or CVC5.

No installation, download, PATH fixture, or finite result can satisfy this gate.
Exit 0 = accepted, 1 = failed, 3 = blocked/unrun (missing executable/pin).
"""
import argparse
import hashlib
import json
import pathlib
import shutil
import subprocess
import sys


def digest(path):
    with open(path, "rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verifier", required=True, type=pathlib.Path)
    parser.add_argument("--solver", help="path to trusted Z3/CVC5; otherwise discover installed solver")
    parser.add_argument("--sha256", help="previously trusted solver SHA-256; never inferred as trust")
    parser.add_argument("--output", required=True, type=pathlib.Path)
    args = parser.parse_args()
    # Do not replace previous evidence, even after an interrupted run.
    args.output.mkdir(parents=True, exist_ok=False)
    solver = args.solver or shutil.which("z3") or shutil.which("cvc5")
    report = {"schema_version": 1, "profile": "scalar_v2", "state": "blocked/unrun",
              "timeout_ms_per_solver_process": 2000, "max_cases": 100000,
              "input_limit_bytes": 65536, "solver": solver,
              "trusted_solver_sha256": args.sha256,
              "trust": "operator must independently trust the supplied executable and pin; probes are not an attestation",
              "accumulator_source": "blocked by shared CGT012 Mutate effect admission; library-only expansion tests",
              "cases": []}

    def stop(reason, code):
        report["reason"] = reason
        write_json(args.output / "acceptance.json", report)
        print(json.dumps({"state": report["state"], "reason": reason, "report": str(args.output / "acceptance.json")}))
        return code

    if not solver or not pathlib.Path(solver).is_file():
        return stop("trusted Z3/CVC5 executable unavailable", 3)
    if pathlib.Path(solver).name not in ("z3", "cvc5"):
        return stop("solver must be explicitly identified as z3 or cvc5", 3)
    if not args.sha256 or len(args.sha256) != 64 or any(c not in "0123456789abcdefABCDEF" for c in args.sha256):
        return stop("operator-trusted SHA-256 pin required before execution", 3)
    report["solver_sha256"] = digest(solver)
    if report["solver_sha256"] != args.sha256.lower():
        return stop("solver digest does not match trusted pin; not executed", 3)
    if not args.verifier.is_file():
        return stop("built verifier executable unavailable", 3)
    report["verifier_sha256"] = digest(args.verifier)
    report["verifier"] = str(args.verifier.resolve())
    examples = pathlib.Path(__file__).resolve().parents[1] / "examples" / "scalar-v2"
    cases = [
        ("valid", ["x:0:3"], False),
        ("false", ["x:0:3"], True),
        ("mul-overflow", ["x:9223372036854775807:9223372036854775807"], True),
        ("div-zero", ["x:0:1"], True),
        ("div-min", ["x:-9223372036854775808:-9223372036854775808", "y:-1:-1"], True),
        ("div-truncate", ["x:-7:-7"], False),
        ("inactive", ["x:-1:1"], False),
        ("shadow", ["x:-2:2"], False),
    ]
    failed = False
    for name, domains, violation in cases:
        source = examples / (name + ".cg")
        command = [str(args.verifier.resolve()), str(source), "--format", "json",
                   "--max-cases", "100000", "--timeout-ms", "2000", "--solver", solver,
                   "--trusted-solver-sha256", args.sha256.lower()]
        for domain in domains:
            command.extend(["--domain", domain])
        entry = {"name": name, "source_sha256": digest(source), "command": command, "state": "failed"}
        try:
            process = subprocess.run(command, capture_output=True, text=True, timeout=60, check=False)
            (args.output / (name + ".stdout.json")).write_text(process.stdout)
            (args.output / (name + ".stderr.txt")).write_text(process.stderr)
            result = json.loads(process.stdout)
            solver_report = result.get("solver", {})
            script = result["smtlib"]
            (args.output / (name + ".smt2")).write_text(script)
            script_hash = hashlib.sha256(script.encode()).hexdigest()
            entry["smt_sha256"] = script_hash
            entry["version"] = solver_report.get("version")
            entry["statuses"] = solver_report.get("statuses")
            # Finite counterexamples do not substitute for live witness validation.
            expected = ["unsat", "sat", "sat" if name == "false" else "unsat",
                        "sat" if violation and name != "false" else "unsat"]
            valid = (process.returncode == 0 and not process.stderr
                     and all(solver_report.get(key) is True for key in
                             ("pin_valid", "identity_valid", "executable_stable", "fixtures_valid"))
                     and solver_report.get("executable_sha256") == args.sha256.lower()
                     and solver_report.get("script_sha256") == script_hash
                     and result["bounded_solver_result"].get("source_parity") is True
                     and entry["statuses"] == expected)
            if violation:
                valid = valid and solver_report.get("response") == "validated_counterexample"
                valid = valid and any(w.get("validation") == "validated" for w in solver_report.get("witness_attempts", []))
            else:
                valid = valid and result["proof"].get("state") == "proven"
            if valid:
                entry["state"] = "accepted"
            else:
                entry["reason"] = "live obligations, identity, probes, source parity, proof or witness gate failed; inspect transcript"
        except (OSError, subprocess.TimeoutExpired, ValueError, KeyError) as error:
            entry["reason"] = str(error)
        failed = failed or entry["state"] != "accepted"
        report["cases"].append(entry)
    if digest(solver) != report["solver_sha256"]:
        failed = True
        report["reason"] = "solver executable changed during acceptance"
    report["state"] = "failed" if failed else "accepted_for_operator_trusted_executable"
    write_json(args.output / "acceptance.json", report)
    print(json.dumps({"state": report["state"], "report": str(args.output / "acceptance.json")}))
    return int(failed)


if __name__ == "__main__":
    sys.exit(main())
