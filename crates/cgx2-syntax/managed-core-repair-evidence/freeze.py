#!/usr/bin/env python3
"""Copy the final existing binaries after Cargo checks finish; no compilation."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--binary-dir", type=Path, required=True)
    p.add_argument("--output-dir", type=Path, required=True)
    p.add_argument("--manifest", type=Path, required=True)
    args = p.parse_args()
    source = args.binary_dir.resolve()
    output = args.output_dir.resolve()
    output.mkdir()  # Never replace a previous frozen candidate.
    report = {"schema_version": 1, "source_directory": str(source), "binary_directory": str(output),
              "checks_after_cargo": True, "binary_sha256": {}, "probe_library_sha256": {}}
    for name in ["cgx2", "cgx2-native", "cgx2c", "cgx2-effect-index", "cgx2-security", "cgx2-verify", "cgx2-notebook"]:
        before = digest(source / name)
        shutil.copy2(source / name, output / name)
        assert digest(output / name) == before == digest(source / name)
        report["binary_sha256"][name] = before
    # These libraries support the interpreter fault probe and the temporary
    # historical Rust test. Standalone emitted programs use embedded runtime code.
    for name in ["cgx2_cli", "cgx2_sema", "cgx2_package", "cgx2_emit_rust"]:
        library = "lib" + name + ".rlib"
        (output / library).symlink_to(source / library)
        report["probe_library_sha256"][library] = digest(source / library)
    (output / "deps").symlink_to(source / "deps", target_is_directory=True)
    args.manifest.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
