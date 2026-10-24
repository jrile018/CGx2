#!/usr/bin/env python3
"""Persist exact successful and failed records without generated executables."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--scratch-dir", type=Path, required=True)
    p.add_argument("--outside-root", type=Path, required=True)
    args = p.parse_args()
    scratch = args.scratch_dir.resolve()
    out = HERE / "evidence"
    out.mkdir(exist_ok=False)
    manifest = {}

    def export(path, name, compressed=False):
        original = path.read_bytes()
        raw = gzip.decompress(original) if compressed else original
        encoded = gzip.compress(raw, mtime=0)
        target = out / (str(name) + ".gz")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(encoded)
        assert gzip.decompress(target.read_bytes()) == raw
        manifest[str(target.relative_to(out))] = {"bytes": len(raw), "sha256": sha(raw), "gzip_sha256": sha(encoded)}
        if compressed:
            manifest[str(target.relative_to(out))]["historical_input_sha256"] = sha(original)

    for directory in ["verification", "product", "product-retry", "parity", "standalone"]:
        for path in sorted((scratch / directory).rglob("*")):
            if path.is_file() and not path.is_symlink() and path.suffix in (".json", ".cg", ".py", ".rs", ".toml"):
                # The freeze contains actual executables/libraries, never export those.
                if directory == "verification" and path.parent != scratch / directory:
                    continue
                export(path, path.relative_to(scratch))
    export(scratch / "standalone-journal.json", "standalone-journal.json")
    historical = ROOT / "plans/release-completion/s2b-ordinary-memory-review/evidence"
    for name in ("standalone.log", "failed-standalone-scratch-audit.json", "standalone-retry-verification.json"):
        export(historical / (name + ".gz"), Path("historical-coordinator") / name, compressed=True)
    for path in sorted(args.outside_root.resolve().glob("ordinary-roots-*/**/*.rs")):
        export(path, Path("outside-source") / path.relative_to(args.outside_root.resolve()))
    quality = {}
    for path in sorted((HERE / "checks").glob("*.json")):
        record = json.loads(path.read_text())
        if record.get("status") == "running":
            continue
        row = {k: record.get(k) for k in ("exit", "signal", "reason", "limits", "argv")}
        counts = re.findall(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored", record.get("stdout", ""))
        if counts:
            row["rust_tests"] = {k: sum(int(v[i]) for v in counts) for i, k in enumerate(("passed", "failed", "ignored"))}
        quality[path.stem] = row
    frozen = json.loads((scratch / "verification/frozen.json").read_text())
    changed = [name for name, digest in frozen["compiler_source_sha256"].items() if sha((ROOT / name).read_bytes()) != digest]
    assert not changed, changed
    binaries = scratch / "verification/frozen"
    final_hashes = {name: sha((binaries / name).read_bytes()) for name in dict(frozen["binary_sha256"], **frozen["probe_library_sha256"])}
    assert final_hashes == dict(frozen["binary_sha256"], **frozen["probe_library_sha256"])
    assert all(sha((binaries / "deps" / name).read_bytes()) == digest for name, digest in frozen["dependency_sha256"].items())
    product = json.loads((scratch / "product-retry/product.json").read_text())
    first = json.loads((scratch / "product/product.json").read_text())
    assert first["frozen_before"] == first["frozen_after"] == product["frozen_before"] == product["frozen_after"]
    parity = json.loads((scratch / "parity/parity.json").read_text())
    standalone = json.loads((scratch / "standalone/standalone.json").read_text())
    files = [ROOT / "crates/cgx2-runtime/src/graph.rs", ROOT / "crates/cgx2-runtime/src/scope.rs",
             ROOT / "crates/cgx2-runtime/tests/ordinary_roots_repair.rs", ROOT / "crates/cgx2-sema/tests/ordinary_roots_repair.rs"]
    files.extend(HERE.glob("*.py"))
    files.extend(HERE.glob("fixtures/*.cg"))
    files.extend(path for path in (HERE / "checks").glob("*.json")
                 if json.loads(path.read_text()).get("status") != "running")
    summary = {"reviewed_base": frozen["reviewed_base"], "ordinary_memory_accepted": False,
        "quality": quality, "first_product": {k: first[k] for k in ("passed", "failed")},
        "product": {k: product[k] for k in ("passed", "failed")},
        "focused_parity": {k: parity[k] for k in ("passed", "failed")},
        "standalone": {k: standalone[k] for k in ("passed", "failed")},
        "other_lane_failures": parity["known_unrepaired"], "final_frozen_hashes": final_hashes,
        "compiler_sources_unchanged_since_freeze": not changed,
        "owned_file_sha256": {str(path.relative_to(ROOT)): sha(path.read_bytes()) for path in sorted(files)},
        "unrun": ["external leak-enabled assurance", "independent safety audit", "full workspace test suite", "release performance measurements"],
        "remaining": ["OM01", "OM02", "OM03", "OM04", "OM08", "general aggregate ownership",
                      "source copy_owned/promote failed-owner continuation", "complete authenticated ownership v3", "static dependency-file attribution"]}
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps({"exported_files": len(manifest), "product": summary["product"], "parity": summary["focused_parity"], "standalone": summary["standalone"]}))


if __name__ == "__main__":
    main()
