"""Export one hash-checked, public-only syntax-pilot trial workspace."""

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath

from syntax_packet import PACKET, validate as validate_packet


ROOT = Path(__file__).resolve().parent
PUBLIC_TOOLS = ("syntax_tool.py", "syntax_normalize.py")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def _file(path):
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"missing or symlinked file: {path}")
    return path.read_bytes()


def _strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for item in value.values():
            yield from _strings(item)
    elif isinstance(value, list):
        for item in value:
            yield from _strings(item)


def _safe_packet(packet):
    if packet.is_symlink() or not packet.is_dir():
        raise ValueError("packet directory must be real")
    for path in packet.rglob("*"):
        if path.is_symlink():
            raise ValueError(f"packet contains symlink: {path}")


def _starter(packet, task, variant, hashes):
    task_id = task["id"]
    if not isinstance(task_id, str) or not task_id.replace("_", "").isalnum():
        raise ValueError("unsafe task ID")
    name = task["starter_variants"][variant]
    expected = f"candidate/{variant}/{task_id}.cg"
    if not isinstance(name, str) or PurePosixPath(name).is_absolute() or name != expected:
        raise ValueError("starter path escapes its public variant")
    contents = _file(packet / name)
    if digest(contents) != hashes.get(name):
        raise ValueError("starter frozen SHA-256 mismatch")
    if b"TODO()" not in contents:
        raise ValueError("public starter has no TODO() placeholder")
    return contents


def export(task_id, variant, out, packet=PACKET):
    if variant not in ("indent", "brace"):
        raise ValueError("variant must be indent or brace")
    packet = Path(packet)
    out = Path(out)
    _safe_packet(packet)
    manifest_bytes = _file(packet / "packet.json")
    manifest = json.loads(manifest_bytes)
    lock = json.loads(_file(packet / "packet-hashes.json"))
    if digest(manifest_bytes) != lock["sha256"].get("packet.json"):
        raise ValueError("packet frozen SHA-256 mismatch")
    task = next((item for item in manifest["tasks"] if item["id"] == task_id), None)
    if task is None:
        raise ValueError("unknown task ID")
    starter = _starter(packet, task, variant, lock["sha256"])
    packet_check = validate_packet(packet)
    if packet_check["findings"]:
        raise ValueError("packet contains invalid or leaking candidate material")
    prompt = {
        "schema_version": 1,
        "task_id": task_id,
        "variant": variant,
        "prompt": manifest["common_prompt"],
        "public_api": task["public_api"],
        "expected_behavior": task["expected_behavior"],
        "example_args": task["example_args"],
        "allowed_tools": manifest["allowed_tools"],
        "context_budget_tokens": manifest["context_budget_tokens"],
        "token_budget": manifest["token_budget"],
        "time_budget_seconds": manifest["time_budget_seconds"],
    }
    contract = {
        "tool": "python3 syntax_tool.py {emit-rust,build} SOURCE.cg --variant VARIANT --compiler CGX2C --out OUTPUT",
        "compiler": "cgx2c; operator supplies the same pinned binary to both variants",
        "final_submission": "solution.cg",
        "acceptance": "The operator grades the final source outside this workspace; hidden outputs and tests are not exposed here.",
    }
    files = {
        "prompt.json": json.dumps(prompt, sort_keys=True, indent=2).encode() + b"\n",
        "starter.cg": starter,
        "acceptance-interface.json": json.dumps(contract, sort_keys=True, indent=2).encode() + b"\n",
    }
    for tool in PUBLIC_TOOLS:
        files[tool] = _file(ROOT / tool)
    private = [_file(path) for path in (packet / "private").rglob("*") if path.is_file()]
    for name, contents in files.items():
        decoded_strings = list(_strings(json.loads(contents))) if name.endswith(".json") else []
        if (b"private/reference" in contents or b"private/oracle" in contents
                or any(secret and (secret in contents or
                    any(secret.decode("utf-8") in value for value in decoded_strings))
                    for secret in private)):
            raise ValueError(f"private material leaked into export: {name}")
    if out.exists() or out.is_symlink():
        raise ValueError("export directory already exists")
    out.mkdir(parents=True, exist_ok=False)
    for name, contents in files.items():
        (out / name).write_bytes(contents)
    export_manifest = {
        "schema_version": 1,
        "packet_sha256": digest(manifest_bytes),
        "task_id": task_id,
        "variant": variant,
        "files_sha256": {name: digest(contents) for name, contents in sorted(files.items())},
    }
    (out / "export-manifest.json").write_text(
        json.dumps(export_manifest, sort_keys=True, indent=2) + "\n")
    verify_export(out)
    return export_manifest


def verify_export(out):
    out = Path(out)
    if out.is_symlink() or not out.is_dir():
        raise ValueError("export directory must be real")
    manifest = json.loads(_file(out / "export-manifest.json"))
    expected = set(manifest["files_sha256"]) | {"export-manifest.json"}
    actual = {path.name for path in out.iterdir()}
    if actual != expected:
        raise ValueError("export file set changed")
    for name, expected_sha in manifest["files_sha256"].items():
        if "/" in name or name in (".", ".."):
            raise ValueError("unsafe export filename")
        if digest(_file(out / name)) != expected_sha:
            raise ValueError(f"export SHA-256 mismatch: {name}")
    return manifest


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--task", required=True)
    parser.add_argument("--variant", choices=("indent", "brace"), required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    print(json.dumps(export(args.task, args.variant, args.out), sort_keys=True, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
