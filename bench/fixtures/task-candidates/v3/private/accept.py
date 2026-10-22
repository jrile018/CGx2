"""Private, independently executable correctness oracle for Wave 13 candidates.

Run from the repository root after `cargo build --offline -p cgx2c`:
  python3 bench/fixtures/task-candidates/v3/private/accept.py --task quote-credit-v3
  python3 bench/fixtures/task-candidates/v3/private/accept.py --task grid-quote-v3
The starter must build and produce a wrong value; the private reference overlay
must pass every value and checked-overflow case in both languages.
"""

import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
REPO = ROOT.parents[3]
SPECS = {
    "quote-credit-v3": {
        "module": "quote.pricing", "function": "quote", "rust_module": "pricing",
        "cases": [
            ((7, 120, 18, 34), 824),
            ((0, 9, 0, 0), 0),
            ((2, 100, 5, 20), 185),
            ((9223372036854775807, 1, 1, 0), "overflow"),
        ],
        "wrong_case": ((7, 120, 18, 34), 892),
    },
    "grid-quote-v3": {
        "module": "grid.quote", "function": "total", "rust_module": "quote",
        "cases": [((10,), 2810), ((0,), 0), ((1,), 281),
                  ((9223372036854775807,), "overflow")],
        "wrong_case": ((10,), 3560),
        "group_cases": [(name, rate * 10) for name, rate in
                        (("generation", 28), ("transmission", 49),
                         ("distribution", 83), ("market", 121))],
    },
}


def invoke(command, cwd=None):
    return subprocess.run(command, cwd=cwd, text=True, capture_output=True,
                          timeout=90, check=False)


def run_case(task, language, variant, args, compiler, group=None):
    spec = SPECS[task]
    source = ROOT / "tasks" / task / language
    with tempfile.TemporaryDirectory(prefix="cgx2-candidate-") as directory:
        root = Path(directory) / "starter"
        shutil.copytree(source, root)
        if variant == "reference":
            overlay = ROOT / "private" / task / language
            for item in overlay.iterdir():
                destination = root / ("src" if language == "cgx2" else "") / item.name
                shutil.copyfile(item, destination)
        module = f"grid.{group}" if group else spec["module"]
        function = "charge" if group else spec["function"]
        call = f"{module}.{function}({', '.join(map(str, args))})"
        if language == "cgx2":
            (root / "src/main.cg").write_text(
                f"module {module.split('.')[0]}.main\nimport {module}\n"
                f"fn main() -> i64:\n    {call}\n", encoding="utf-8")
            binary = Path(directory) / "program"
            build = invoke([str(compiler), "build", "--package", str(root),
                            "--out", str(binary)])
        else:
            rust_module = group or spec["rust_module"]
            call = f"{rust_module}::{function}({', '.join(map(str, args))})"
            main = root / "main.rs"
            contents = main.read_text(encoding="utf-8")
            prefix = contents[:contents.index("fn main()")]
            main.write_text(prefix + f'fn main() {{ println!("{{}}", {call}); }}\n', encoding="utf-8")
            binary = Path(directory) / "program"
            build = invoke(["rustc", "--edition=2024", str(main), "-o", str(binary)])
        if build.returncode:
            raise AssertionError(f"{task}/{language}/{variant} failed to build:\n{build.stderr}")
        return invoke([str(binary)])


def check(task, compiler):
    spec = SPECS[task]
    for language in ("cgx2", "rust"):
        args, wrong = spec["wrong_case"]
        observed = run_case(task, language, "starter", args, compiler)
        assert observed.returncode == 0 and observed.stdout.strip() == str(wrong), (
            task, language, "negative fixture must be a behavioral failure", observed)
        for args, expected in spec["cases"]:
            observed = run_case(task, language, "reference", args, compiler)
            if expected == "overflow":
                assert observed.returncode != 0 and "overflow" in observed.stderr.lower(), (
                    task, language, args, observed)
            else:
                assert observed.returncode == 0 and observed.stdout.strip() == str(expected), (
                    task, language, args, observed)
        for group, expected in spec.get("group_cases", []):
            observed = run_case(task, language, "reference", (10,), compiler, group=group)
            assert observed.returncode == 0 and observed.stdout.strip() == str(expected), (
                task, language, group, observed)
        print(f"{task}/{language}: negative behavior and {len(spec['cases'])} reference cases passed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--task", required=True, choices=sorted(SPECS))
    parser.add_argument("--cgx2c", type=Path, default=REPO / "target/debug/cgx2c")
    args = parser.parse_args()
    if not args.cgx2c.is_file():
        parser.error(f"build cgx2c first: {args.cgx2c}")
    check(args.task, args.cgx2c.resolve())


if __name__ == "__main__":
    main()
