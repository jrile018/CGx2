#!/usr/bin/env python3
"""Preserve raw reports and compiler inputs, excluding rebuildable binaries."""
import gzip
import hashlib
import json
from pathlib import Path
import shutil

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    destination = HERE / "exports"
    destination.mkdir(exist_ok=False)
    manifest = {}
    attempts = ["product-attempt-1", "product-attempt-2", "product-attempt-3",
                "product-final", "product-extended-final", "product-complete", "unchanged-ordinary", "default-managed"]
    for attempt in attempts:
        source = ROOT / "target" / ("ordinary-core-admission-" + attempt)
        for path in sorted(source.rglob("*")):
            if not path.is_file() or path.suffix not in (".json", ".cg", ".toml", ".rs", ".py", ".gz"):
                continue
            relative = Path(attempt) / path.relative_to(source)
            compressed = path.suffix != ".gz"
            target = destination / (str(relative) + ".gz" if compressed else relative)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(gzip.compress(path.read_bytes(), mtime=0) if compressed else path.read_bytes())
            manifest[str(target.relative_to(HERE))] = {"original_sha256": digest(path),
                                                       "stored_sha256": digest(target),
                                                       "gzip": compressed}
    shutil.copy2(ROOT / "target/ordinary-core-admission-freeze/frozen.json", HERE / "frozen.json")
    inputs = {}
    for path in sorted((ROOT / "crates").rglob("*")):
        if path.is_file() and path.suffix in (".rs", ".toml"):
            inputs[str(path.relative_to(ROOT))] = digest(path)
    for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml"):
        path = ROOT / name
        if path.exists():
            inputs[name] = digest(path)
    toolchain = json.loads((HERE / "toolchain-path.json").read_text())["stdout"].strip()
    inputs[toolchain] = digest(Path(toolchain))
    (HERE / "compiler-inputs.json").write_text(json.dumps(inputs, indent=2) + "\n")
    (HERE / "exports.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps({"exports": len(manifest), "compiler_inputs": len(inputs)}))


if __name__ == "__main__":
    main()
