#!/usr/bin/env python3
"""Extend the read-only freeze helper with the canonical runtime probe library."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    out = Path(os.environ["TMPDIR"]) / "verification"
    out.mkdir(exist_ok=False)
    source = Path(os.environ["CARGO_TARGET_DIR"]) / "debug"
    subprocess.run([sys.executable, str(HERE.parent / "managed-core-repair-evidence/freeze.py"),
                    "--binary-dir", str(source), "--output-dir", str(out / "frozen"),
                    "--manifest", str(out / "frozen.json")], check=True)
    runtime = source / "libcgx2_runtime.rlib"
    shutil.copy2(runtime, out / "frozen" / runtime.name)
    manifest = json.loads((out / "frozen.json").read_text())
    manifest["probe_library_sha256"][runtime.name] = digest(runtime)
    manifest["dependency_sha256"] = {p.name: digest(p) for p in sorted((source / "deps").glob("*.rlib"))}
    manifest["runtime_source_sha256"] = {str(p.relative_to(ROOT)): digest(p)
        for p in sorted((ROOT / "crates/cgx2-runtime/src").glob("**/*.rs"))}
    manifest["toolchain"] = {}
    for tool in ("rustc", "cargo"):
        actual = subprocess.check_output(["rustup", "which", tool], text=True).strip()
        manifest["toolchain"][tool] = {"path": actual, "sha256": digest(Path(actual)),
            "version": subprocess.check_output([tool, "-vV"], text=True)}
    inputs = {ROOT / "Cargo.toml", ROOT / "Cargo.lock"}
    inputs.update(ROOT.glob("rust-toolchain*"))
    for pattern in ("*/Cargo.toml", "*/build.rs", "*/src/**/*.rs"):
        inputs.update((ROOT / "crates").glob(pattern))
    manifest["compiler_source_sha256"] = {str(p.relative_to(ROOT)): digest(p) for p in sorted(inputs) if p.is_file()}
    manifest["reviewed_base"] = "7bb7a480119826693db67ed503caaf7f23f6abb2"
    (out / "frozen.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps({"binaries": len(manifest["binary_sha256"]),
                      "probe_libraries": len(manifest["probe_library_sha256"]), "manifest": str(out / "frozen.json")}))


if __name__ == "__main__":
    main()
