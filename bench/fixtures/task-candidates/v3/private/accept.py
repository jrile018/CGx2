"""Private correctness oracle for Wave 13 candidates.

Reference smoke: python3 bench/fixtures/task-candidates/v3/private/accept.py
Scored submission: add --task TASK --language cgx2|rust --candidate-workspace DIR.
Only a submitted workspace is scored; bundled starters and references are smoke
fixtures. Keep this file and reference overlays out of model-visible workspaces.
"""

import argparse
import json
import os
from pathlib import Path
import re
import resource
import shutil
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
REPO = ROOT.parents[3]
LANGUAGES = ("cgx2", "rust")
COMPILE_SECONDS = 90
RUN_SECONDS = 10
MAX_OUTPUT_BYTES = 1_000_000
# The tasks require only module calls and arithmetic. Forbid source-level file
# and process access, especially compile-time includes of private fixtures.
FORBIDDEN_SOURCE = re.compile(
    r"\b(?:include|include_str|include_bytes|std::fs|std::process|"
    r"read_to_string|File::|Command::|private)\b|\.\.[/\\]|/home/|/tmp/",
    re.IGNORECASE,
)

SPECS = {
    "quote-credit-v3": {
        "module": "quote.pricing", "function": "quote", "rust_module": "pricing",
        "cases": [
            ((7, 120, 18, 34), 824),
            ((0, 9, 0, 0), 0),
            ((2, 100, 5, 20), 185),
            ((3, 17, -4, 6), 41),
            ((-7, 13, 9, -2), -80),
            ((1, 0, -5, 8), -13),
            ((9223372036854775807, 1, 1, 0), "overflow"),
            ((9223372036854775807, 2, -9223372036854775807, 0), "overflow"),
        ],
        "wrong_case": ((7, 120, 18, 34), 892),
    },
    "grid-quote-v3": {
        "module": "grid.quote", "function": "total", "rust_module": "quote",
        "cases": [
            ((10,), 2810), ((0,), 0), ((1,), 281),
            ((7,), 1967), ((-3,), -843),
            ((9223372036854775807,), "overflow"),
        ],
        "wrong_case": ((10,), 3560),
        "group_rates": {"generation": 28, "transmission": 49,
                        "distribution": 83, "market": 121},
    },
}


def validate_workspace(task, language, candidate_workspace):
    """Require a distinct, exact task/language starter shape and plain source."""
    if task not in SPECS or language not in LANGUAGES:
        raise ValueError("unknown task or language")
    supplied = Path(candidate_workspace)
    if supplied.is_symlink() or not supplied.is_dir():
        raise ValueError("candidate workspace must be a real directory")
    workspace = supplied.resolve(strict=True)
    if workspace.is_relative_to(ROOT):
        raise ValueError("bundled starters and references cannot be scored")
    expected = ROOT / "tasks" / task / language
    expected_files = {p.relative_to(expected) for p in expected.rglob("*") if p.is_file()}
    actual_files = set()
    for path in workspace.rglob("*"):
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise ValueError("symlinks and special files are forbidden")
        if path.is_file():
            actual_files.add(path.relative_to(workspace))
    if actual_files != expected_files:
        raise ValueError("workspace file set differs from requested task/language starter")
    cards = json.loads((ROOT / "task-cards.json").read_text(encoding="utf-8"))["cards"]
    card = next(card for card in cards if card["task_id"] == task)
    allowed = {Path(name).relative_to(language) for name in card["allowed_files"]
               if Path(name).parts[0] == language}
    if not allowed <= expected_files:
        raise ValueError("task card editable files differ from starter")
    suffix = ".cg" if language == "cgx2" else ".rs"
    for relative in actual_files:
        path = workspace / relative
        if relative not in allowed and path.read_bytes() != (expected / relative).read_bytes():
            raise ValueError(f"file outside editable set was changed: {relative}")
        if path.suffix == suffix:
            if FORBIDDEN_SOURCE.search(path.read_text(encoding="utf-8")):
                raise ValueError(f"forbidden source-level fixture/file access: {relative}")
        elif relative not in (Path("cgx2.toml"), Path("README.md")):
            raise ValueError(f"unexpected language file: {relative}")
    return workspace


def invoke(command, *, cwd=None, seconds=COMPILE_SECONDS):
    def limits():
        resource.setrlimit(resource.RLIMIT_CPU, (seconds, seconds + 1))
        resource.setrlimit(resource.RLIMIT_FSIZE, (64 * 1024 * 1024,) * 2)
    env = {key: value for key, value in os.environ.items()
           if key not in ("PYTHONPATH", "RUSTFLAGS", "RUSTC_WRAPPER")}
    # File-backed capture keeps a noisy submission from growing parent memory.
    # RLIMIT_FSIZE also bounds each captured stream at 64 MiB at the OS level.
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        completed = subprocess.run(command, cwd=cwd, env=env, stdout=stdout,
                                   stderr=stderr, timeout=seconds + 2,
                                   check=False, preexec_fn=limits)
        stdout.seek(0)
        stderr.seek(0)
        out = stdout.read(MAX_OUTPUT_BYTES + 1)
        err = stderr.read(MAX_OUTPUT_BYTES + 1)
    if len(out) > MAX_OUTPUT_BYTES or len(err) > MAX_OUTPUT_BYTES:
        raise AssertionError("command exceeded captured-output limit")
    return subprocess.CompletedProcess(command, completed.returncode,
                                       out.decode("utf-8", errors="replace"),
                                       err.decode("utf-8", errors="replace"))


def run_case(task, language, variant, args, compiler, *, group=None, candidate_workspace=None):
    spec = SPECS[task]
    source = candidate_workspace if variant == "candidate" else ROOT / "tasks" / task / language
    with tempfile.TemporaryDirectory(prefix="cgx2-candidate-") as directory:
        root = Path(directory) / "workspace"
        shutil.copytree(source, root)
        if variant == "reference":
            for item in (ROOT / "private" / task / language).iterdir():
                destination = root / ("src" if language == "cgx2" else "") / item.name
                shutil.copyfile(item, destination)
        module = f"grid.{group}" if group else spec["module"]
        function = "charge" if group else spec["function"]
        binary = Path(directory) / "program"
        if language == "cgx2":
            # The frozen scalar parser does not accept a leading minus in a
            # call argument; an equivalent checked subtraction is supported.
            cg_args = ", ".join(f"(0 - {-value})" if value < 0 else str(value)
                                for value in args)
            call = f"{module}.{function}({cg_args})"
            (root / "src/main.cg").write_text(
                f"module {module.split('.')[0]}.main\nimport {module}\n"
                f"fn main() -> i64:\n    {call}\n", encoding="utf-8")
            build = invoke([str(compiler), "build", "--package", str(root),
                            "--out", str(binary)])
        else:
            rust_module = group or spec["rust_module"]
            call = f"{rust_module}::{function}({', '.join(map(str, args))})"
            main = root / "main.rs"
            contents = main.read_text(encoding="utf-8")
            prefix = contents[:contents.index("fn main()")]
            main.write_text(prefix + f'fn main() {{ println!("{{}}", {call}); }}\n', encoding="utf-8")
            build = invoke(["rustc", "--edition=2024", str(main), "-o", str(binary)])
        if build.returncode:
            raise AssertionError(f"{task}/{language}/{variant} failed to build:\n{build.stderr}")
        return invoke([str(binary)], cwd=root, seconds=RUN_SECONDS)


def normalized_output(observed):
    """Permit one result line with surrounding whitespace and CRLF only."""
    lines = observed.stdout.replace("\r\n", "\n").strip().splitlines()
    return lines[0].strip() if len(lines) == 1 else None


def require(condition, detail):
    if not condition:
        raise AssertionError(detail)


def check(task, compiler, *, candidate_workspace=None, language=None):
    if candidate_workspace is not None:
        if language is None:
            raise ValueError("--language is required for submitted workspaces")
        candidate_workspace = validate_workspace(task, language, candidate_workspace)
        languages = (language,)
    else:
        languages = (language,) if language else LANGUAGES
    for current_language in languages:
        if candidate_workspace is None:
            args, wrong = SPECS[task]["wrong_case"]
            observed = run_case(task, current_language, "starter", args, compiler)
            require(observed.returncode == 0 and normalized_output(observed) == str(wrong),
                    (task, current_language, "negative starter fixture", observed))
        variant = "candidate" if candidate_workspace is not None else "reference"
        for args, expected in SPECS[task]["cases"]:
            observed = run_case(task, current_language, variant, args, compiler,
                                candidate_workspace=candidate_workspace)
            if expected == "overflow":
                require(observed.returncode != 0 and "overflow" in observed.stderr.lower(),
                        (task, current_language, args, observed))
            else:
                require(observed.returncode == 0 and normalized_output(observed) == str(expected),
                        (task, current_language, args, observed))
        for group, rate in SPECS[task].get("group_rates", {}).items():
            for units in (10, -3):
                observed = run_case(task, current_language, variant, (units,), compiler,
                                    group=group, candidate_workspace=candidate_workspace)
                require(observed.returncode == 0 and normalized_output(observed) == str(rate * units),
                        (task, current_language, group, units, observed))
        print(f"{task}/{current_language}: {variant} passed hidden acceptance")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--task", choices=SPECS, help="one candidate; omitted for both reference smokes")
    parser.add_argument("--language", choices=LANGUAGES)
    parser.add_argument("--candidate-workspace", type=Path)
    parser.add_argument("--cgx2c", type=Path, default=REPO / "target/debug/cgx2c")
    args = parser.parse_args(argv)
    if args.candidate_workspace is not None and (args.task is None or args.language is None):
        parser.error("submitted workspaces require --task and --language")
    if not args.cgx2c.is_file():
        parser.error(f"build cgx2c first: {args.cgx2c}")
    for task in (args.task,) if args.task else SPECS:
        check(task, args.cgx2c.resolve(), candidate_workspace=args.candidate_workspace,
              language=args.language)


if __name__ == "__main__":
    main()
