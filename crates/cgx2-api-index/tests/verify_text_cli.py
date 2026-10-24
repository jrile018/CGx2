"""Exercise the existing CLI with owned fixtures; writes local conformance evidence."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    fixtures = Path(__file__).resolve().parent / "fixtures"
    results = []

    def run(arguments, *, success=True, expected=None):
        command = [str(part) for part in arguments]
        result = subprocess.run(command, capture_output=True, text=True, timeout=60)
        results.append({"command": command, "exit": result.returncode,
                        "stdout": result.stdout, "stderr": result.stderr})
        assert (result.returncode == 0) == success, results[-1]
        if expected is not None:
            assert result.stdout == expected, results[-1]
        if not success:
            assert not result.stdout and "CGT" in result.stderr, results[-1]
        return result.stdout

    try:
        cases = [("text-checked", False, "text-checked"),
                 ("text-syntax-only", True, "text-syntax-only"),
                 ("text-contracts", False, "text-contracts-checked"),
                 ("text-contracts", True, "text-contracts-syntax-only")]
        for source, syntax_only, golden in cases:
            command = [binary, "api"] + (["--syntax-only"] if syntax_only else [])
            command += [fixtures / (source + ".cg")]
            expected = (fixtures / (golden + ".txt")).read_text()
            run(command, expected=expected)
            run(command, expected=expected)
            expected_json = (fixtures / (golden + ".json")).read_text() + "\n"
            run(command + ["--json"], expected=expected_json)
            run(command + ["--json"], expected=expected_json)

        runnable = fixtures / "text-runnable.cg"
        run([binary, "check", fixtures / "text-checked.cg"])
        run([binary, "check", runnable])
        run([binary, "run", runnable], expected="7\n")
        native = output_dir / "positive-native"
        assert not native.exists(), "Use a fresh output directory"
        run([binary, "build", runnable, "--out", native])
        run([native], expected="7\n")
        for target, suffix in [("rust", "rs"), ("python", "py")]:
            emitted = output_dir / ("positive." + suffix)
            run([binary, "emit", runnable, "--target", target, "--out", emitted])
            if target == "rust":
                executable = output_dir / "positive-rust"
                run(["rustc", "--edition=2021", emitted, "-o", executable])
                run([executable], expected="7\n")
            else:
                run(["python3", emitted], expected="7\n")

        unsupported = fixtures / "text-syntax-only.cg"
        for command in ["api", "check", "run"]:
            run([binary, command, unsupported], success=False)
        run([binary, "api", unsupported, "--json"], success=False)
        for target in ["rust", "python", "native"]:
            unpublished = output_dir / ("rejected-" + target)
            command = ([binary, "build", unsupported] if target == "native" else
                       [binary, "emit", unsupported, "--target", target])
            run(command + ["--out", unpublished], success=False)
            assert not unpublished.exists(), f"Published invalid target: {unpublished}"
    finally:
        report = {"evidence_kind": "local fixture conformance, not external acceptance",
                  "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                  "environment": {key: os.environ.get(key) for key in
                                  ["RUSTUP_TOOLCHAIN", "CARGO_BUILD_JOBS",
                                   "CARGO_TARGET_DIR", "TMPDIR", "LIBRARY_PATH"]},
                  "results": results}
        (output_dir / "cli-report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"Passed {len(results)} CLI/source/target checks; {output_dir / 'cli-report.json'}")


if __name__ == "__main__":
    main()
