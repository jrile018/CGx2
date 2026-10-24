#!/usr/bin/env python3
"""Verify preserved archives, raw stream identities and actual frozen files."""
import base64
import gzip
import hashlib
import json
from pathlib import Path
import sys
import tarfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def streams(value):
    count = 0
    if isinstance(value, list):
        return sum(streams(row) for row in value)
    if not isinstance(value, dict):
        return count
    for name in ("stdout", "stderr"):
        if name + "_sha256" in value:
            data = base64.b64decode(value[name + "_base64"]) if name + "_base64" in value else value[name].encode()
            assert sha(data) == value[name + "_sha256"]
            assert len(data) == value[name + "_bytes"]
            count += 1
    return count + sum(streams(row) for row in value.values() if isinstance(row, (dict, list)))


def main():
    exports = json.loads((HERE / "exports.json").read_text())
    count = 0
    partial_json = []
    for item in exports["exports"]:
        path = HERE / item["stored"]
        assert sha(path.read_bytes()) == item["stored_sha256"]
        data = gzip.decompress(path.read_bytes())
        assert sha(data) == item["original_sha256"] and len(data) == item["original_bytes"]
        if item["original"].endswith(".json"):
            try:
                value = json.loads(data)
            except (ValueError, UnicodeDecodeError):
                partial_json.append(item["original"])
            else:
                count += streams(value)
    for item in exports["retained_executables"]:
        path = Path(item["original"])
        assert path.stat().st_size == item["original_bytes"] and sha(path.read_bytes()) == item["original_sha256"]
    for path in (HERE / "checks").glob("*.json"):
        count += streams(json.loads(path.read_text()))
    frozen = Path(exports["root"]) / "borrow-nominal-frozen-01"
    manifest = json.loads((frozen / "manifest.json").read_text())
    for group in ("artifact_sha256", "dependency_sha256"):
        for name, digest in manifest[group].items():
            assert sha((frozen / name).read_bytes()) == digest
    for name, digest in manifest["source_sha256"].items():
        assert sha((ROOT / name).read_bytes()) == digest
    for info in manifest["toolchain"].values():
        assert sha(Path(info["path"]).read_bytes()) == info["sha256"]
    recovery = json.loads((HERE / "tmp-recovery.json").read_text())
    path = HERE / exports["tmp_archive"]["stored"]
    assert sha(path.read_bytes()) == recovery["archive_sha256"] == exports["tmp_archive"]["sha256"]
    with tarfile.open(path, "r:gz") as archive:
        actual = {p.name: sha(archive.extractfile(p).read()) for p in archive if p.isfile()}
        assert actual == recovery["files"]
    print(json.dumps(dict(exports=len(exports["exports"]), streams=count,
                         frozen_artifacts=len(manifest["artifact_sha256"]),
                         dependencies=len(manifest["dependency_sha256"]), sources=len(manifest["source_sha256"]),
                         retained_executables=len(exports["retained_executables"]),
                         partial_json=partial_json, tmp_archive_files=len(actual))))


if __name__ == "__main__":
    main()
