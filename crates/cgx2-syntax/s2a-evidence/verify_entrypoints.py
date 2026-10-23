"""Reproduce S2a product entry/package evidence; no packages or network needed."""
import argparse
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
parser = argparse.ArgumentParser()
parser.add_argument("--bin-dir", type=Path, required=True)
parser.add_argument("--out-dir", type=Path, required=True)
args = parser.parse_args()
binary = (args.bin_dir / "cgx2").resolve()
out = args.out_dir.resolve()
out.mkdir(parents=True, exist_ok=True)
fixture = ROOT / "crates/cgx2-syntax/s2a-evidence/fixtures"
observations = []


def run(command, expected_code=0, expected_stdout=None, stderr_code=None):
    result = subprocess.run([str(x) for x in command], cwd=ROOT,
                            text=True, capture_output=True, timeout=120)
    assert (result.returncode == 0) == (expected_code == 0), result.stderr
    if expected_stdout is not None:
        assert result.stdout.strip() == expected_stdout, result.stdout
    if stderr_code is not None:
        assert stderr_code in result.stderr, result.stderr
    observations.append({"command": [str(x) for x in command],
                         "exit_code": result.returncode,
                         "stdout": result.stdout, "stderr": result.stderr})
    return result


for name in ["variants.cg", "branches.cg", "package"]:
    run([binary, "run", fixture / name], expected_stdout="42")
for name in ["variants.cg", "package"]:
    executable = out / ("native-" + name.replace(".cg", ""))
    run([binary, "build", fixture / name, "--out", executable, "--force"])
    run([executable], expected_stdout="42")
unsupported = out / "unsupported.py"
assert not unsupported.exists(), "Use a fresh --out-dir for rejection evidence"
run([binary, "emit", fixture / "variants.cg", "--target", "python",
     "--out", unsupported], expected_code=1, stderr_code="CGE006")
assert not unsupported.exists()
syntax_api = run([binary, "api", "--syntax-only", "--json",
                  fixture / "generic-contract.cg"])
api = json.loads(syntax_api.stdout)
assert api["schema_version"] == 6
assert api["semantics_status"] == "syntax-only"
assert len(api["functions"][0]["loop_contracts"]) == 2
assert api["variants"][0]["type_parameters"][0]["constraints"] == ["Eq", "Show"]
run([binary, "--diagnostic-format=json", "check",
     fixture / "generic-contract.cg"], expected_code=1, stderr_code="CGT032")
report = {"schema_version": 1, "cases": len(observations), "passed": True,
          "environment": {key: os.environ.get(key) for key in
                          ["RUSTUP_TOOLCHAIN", "CARGO_BUILD_JOBS",
                           "CARGO_TARGET_DIR", "TMPDIR", "LIBRARY_PATH"]},
          "observations": observations}
(out / "entrypoints.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({"cases": len(observations), "passed": True}))
