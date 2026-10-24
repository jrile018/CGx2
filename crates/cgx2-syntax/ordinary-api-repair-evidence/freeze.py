#!/usr/bin/env python3
"""Freeze owned API consumers and canonical libraries after all Cargo checks."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    output = Path(sys.argv[1]).resolve()
    output.mkdir()
    source = ROOT / "target/release-ordinary-api-visibility/debug"
    artifacts = {}
    for name in ["cgx2", "cgx2-native", "cgx2c", "libcgx2_api_index.rlib", "libcgx2_cli.rlib"]:
        before = digest(source / name)
        shutil.copy2(source / name, output / name)
        assert before == digest(output / name) == digest(source / name)
        artifacts[name] = before
    compiler = Path(subprocess.check_output(["rustup", "which", "rustc"], text=True).strip())
    inputs = [ROOT / "Cargo.toml", ROOT / "Cargo.lock"]
    inputs += [p for p in (ROOT / "crates").rglob("*") if p.is_file()
               and p.suffix in (".rs", ".cg", ".toml")]
    manifest = dict(binary_directory=str(output), artifact_sha256=artifacts,
                    rustc_path=str(compiler), rustc_sha256=digest(compiler),
                    rustc_vv=subprocess.check_output(["rustc", "-vV"], text=True),
                    cargo_vv=subprocess.check_output(["cargo", "-vV"], text=True),
                    source_sha256={str(p.relative_to(ROOT)): digest(p) for p in sorted(inputs)},
                    probe_libraries="canonical top-level libraries; no external library probe run")
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps({"artifacts": artifacts, "source_inputs": len(inputs)}))


if __name__ == "__main__":
    main()
