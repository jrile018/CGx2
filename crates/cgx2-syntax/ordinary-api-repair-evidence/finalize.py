#!/usr/bin/env python3
"""Verify frozen inputs, archive replay artifacts, and retain their identities."""
import gzip
import hashlib
import json
from pathlib import Path
import shutil

from command import ROOT, HERE


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    evidence = HERE / "evidence"
    frozen = ROOT / "target/frozen-ordinary-api-final"
    manifest = json.loads((frozen / "manifest.json").read_text())
    for name, sha in manifest["artifact_sha256"].items():
        assert digest(frozen / name) == sha, name
    for name, sha in manifest["source_sha256"].items():
        assert digest(ROOT / name) == sha, name
    assert digest(Path(manifest["rustc_path"])) == manifest["rustc_sha256"]
    with gzip.open(evidence / "frozen-manifest.json.gz", "xt") as stream:
        json.dump(manifest, stream, indent=2)
    retained = ROOT / "target/retained-ordinary-api-replays"
    retained.mkdir(exist_ok=False)
    archived = []
    for name in ["replay-visibility", "replay-specialization-external"]:
        for path in sorted((evidence / name).rglob("*")):
            if not path.is_file():
                continue
            data = path.read_bytes()
            binary = data.startswith((b"\x7fELF", b"!<arch>\n"))
            if not binary and path.suffix != ".rs":
                continue
            relative = path.relative_to(evidence)
            dest = retained / relative
            dest.parent.mkdir(parents=True, exist_ok=True)
            row = dict(original=str(relative), sha256=hashlib.sha256(data).hexdigest(),
                       bytes=len(data), retained_path=str(dest))
            if not binary:
                compressed = path.with_suffix(path.suffix + ".gz")
                with gzip.open(compressed, "xb") as stream:
                    stream.write(data)
                assert gzip.decompress(compressed.read_bytes()) == data
                row["portable_source_archive"] = str(compressed.relative_to(evidence))
            shutil.move(path, dest)
            assert digest(dest) == row["sha256"]
            archived.append(row)
    scripts = [HERE / name for name in ["command.py", "freeze.py", "replay.py", "run.py", "finalize.py"]]
    scripts += [ROOT / "crates/cgx2-api-index/tests/verify_visibility_cli.py",
                ROOT / "crates/cgx2-sema/tests/source_specialization/run.py"]
    report = dict(frozen_hashes_unchanged=True, compiler_hash_unchanged=True,
                  frozen_source_inputs_unchanged=len(manifest["source_sha256"]),
                  script_sha256={str(p.relative_to(ROOT)): digest(p) for p in scripts},
                  archived=archived)
    (evidence / "final-verification.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k: report[k] for k in
                      ["frozen_hashes_unchanged", "compiler_hash_unchanged", "frozen_source_inputs_unchanged"]}))
    print("Archived artifacts:", len(archived))


if __name__ == "__main__":
    main()
