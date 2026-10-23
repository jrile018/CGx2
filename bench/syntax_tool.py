"""Operator-provided compiler wrapper for either public syntax-pilot variant.

The wrapper contains no reference solutions or oracle. The operator must
provide it as the same tool in both isolated candidate environments.
"""

import argparse
from pathlib import Path
import subprocess
import sys
import tempfile

from syntax_normalize import normalize_brace


def canonical_source(source, variant):
    if variant == "brace":
        return normalize_brace(source)
    if variant == "indent":
        if not source.endswith("\n") or "\t" in source:
            raise ValueError("indent source needs final newline and no tabs")
        return source
    raise ValueError("variant must be indent or brace")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("emit-rust", "build"))
    parser.add_argument("source", type=Path)
    parser.add_argument("--variant", choices=("indent", "brace"), required=True)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.source.suffix != ".cg":
        parser.error("source must end in .cg")
    try:
        canonical = canonical_source(args.source.read_text(), args.variant)
    except (OSError, UnicodeError, ValueError) as error:
        parser.error(str(error))
    with tempfile.TemporaryDirectory(prefix="cgx2-syntax-tool-") as tmp:
        source_path = Path(tmp) / args.source.name
        source_path.write_text(canonical)
        try:
            completed = subprocess.run(
                [str(args.compiler), args.command, str(source_path),
                 "--out", str(args.out)],
                capture_output=True, timeout=120, check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as error:
            print(f"syntax-pilot tool failed: {error}", file=sys.stderr)
            return 1
        original = str(args.source)
        temporary = str(source_path)
        sys.stdout.buffer.write(completed.stdout.replace(temporary.encode(), original.encode()))
        sys.stderr.buffer.write(completed.stderr.replace(temporary.encode(), original.encode()))
        return completed.returncode


if __name__ == "__main__":
    raise SystemExit(main())
