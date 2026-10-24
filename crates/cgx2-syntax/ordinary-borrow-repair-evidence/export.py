#!/usr/bin/env python3
"""Archive source/log bytes and inventory retained executable artifacts."""
import gzip
import hashlib
import json
from pathlib import Path
import shutil
import sys

HERE = Path(__file__).resolve().parent


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    root = Path(sys.argv[1]).resolve()
    retained = HERE / "retained"
    retained.mkdir(exist_ok=False)
    report = dict(schema_version=1, root=str(root), exports=[], retained_executables=[], directories=[])
    paths = [root / "frozen.json", root / "borrow-nominal-frozen-01/manifest.json"]
    for directory in sorted(root.glob("borrow-nominal-*")):
        if directory.is_dir() and directory.name != "borrow-nominal-frozen-01":
            report["directories"].append(str(directory))
            paths.extend(p for p in directory.rglob("*") if p.is_file())
    for path in sorted(paths):
        with path.open("rb") as stream:
            magic = stream.read(4)
        data = path.read_bytes()
        item = dict(original=str(path), original_sha256=sha(data), original_bytes=len(data))
        if magic == b"\x7fELF":
            report["retained_executables"].append(item)
            continue
        destination = retained / (str(path.relative_to(root)) + ".gz")
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(gzip.compress(data, mtime=0))
        assert gzip.decompress(destination.read_bytes()) == data
        item.update(stored=str(destination.relative_to(HERE)), stored_sha256=sha(destination.read_bytes()))
        report["exports"].append(item)
    archive = root / "borrow-nominal-tmp-attempts.tar.gz"
    destination = retained / archive.name
    shutil.copyfile(archive, destination)
    assert sha(archive.read_bytes()) == sha(destination.read_bytes())
    report["tmp_archive"] = dict(stored=str(destination.relative_to(HERE)), sha256=sha(destination.read_bytes()))
    (HERE / "exports.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(dict(exports=len(report["exports"]), retained_executables=len(report["retained_executables"]))))


if __name__ == "__main__":
    main()
