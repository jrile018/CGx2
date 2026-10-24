#!/usr/bin/env python3
"""Check exported raw bytes, frozen executables/libraries and source provenance."""
import gzip
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    exports = json.loads((HERE / "exports.json").read_text())
    for name, row in exports.items():
        data = (HERE / name).read_bytes()
        assert digest(data) == row["stored_sha256"], name
        original = gzip.decompress(data) if row["gzip"] else data
        assert digest(original) == row["original_sha256"], name
    frozen = json.loads((HERE / "frozen.json").read_text())
    for name, sha in dict(frozen["binary_sha256"], **frozen["probe_library_sha256"]).items():
        assert digest((Path(frozen["binary_directory"]) / name).read_bytes()) == sha, name
    inputs = json.loads((HERE / "compiler-inputs.json").read_text())
    for name, sha in inputs.items():
        assert digest((ROOT / name).read_bytes()) == sha, name
    print(json.dumps({"exports_verified": len(exports), "frozen_artifacts_verified": 11,
                      "compiler_inputs_verified": len(inputs)}))


if __name__ == "__main__":
    main()
