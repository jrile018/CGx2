"""Test-only syntax comparison for the executable CGx2 scalar subset.

Brace files are candidate material, never input to the CGx2 compiler directly.
Only the narrowly specified block/statement rewrite below is accepted.
"""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parent
PACKET = ROOT / "fixtures/syntax-pilot/v1"
FUNCTION = re.compile(r"fn [A-Za-z_][A-Za-z_0-9]*\([^{};]*\) -> i64")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def normalize_brace(source):
    """Accept only the packet's one-level function braces and terminators."""
    if not source.endswith("\n") or "\t" in source:
        raise ValueError("brace source needs final newline and no tabs")
    lines = []
    in_function = False
    for raw in source.splitlines():
        if not raw:
            if in_function:
                raise ValueError("blank lines inside brace functions are unsupported")
            lines.append("")
        elif raw == "}":
            if not in_function:
                raise ValueError("unmatched closing brace")
            in_function = False
        elif raw.endswith(" {"):
            if in_function or not FUNCTION.fullmatch(raw[:-2]):
                raise ValueError("unsupported brace block")
            lines.append(raw[:-2] + ":")
            in_function = True
        else:
            prefix = "    " if in_function else ""
            if not raw.startswith(prefix) or raw.startswith("     "):
                raise ValueError("invalid brace indentation")
            statement = raw[len(prefix):]
            if not statement.endswith(";") or any(c in statement for c in "{}"):
                raise ValueError("unsupported brace statement")
            lines.append(prefix + statement[:-1])
    if in_function:
        raise ValueError("unclosed function")
    while lines and lines[-1] == "":
        lines.pop()
    return "\n".join(lines) + "\n"


def _run(argv, timeout=30):
    return subprocess.run(argv, capture_output=True, timeout=timeout, check=False)


def _load(packet):
    manifest = json.loads((packet / "packet.json").read_text())
    oracle = json.loads((packet / "private/oracle.json").read_text())
    if manifest.get("schema_version") != 1 or oracle.get("schema_version") != 1:
        raise ValueError("unsupported packet version")
    return manifest, oracle


def _starter_shape(source):
    """A public starter contains only API headers and unresolved placeholders."""
    lines = [line.strip() for line in source.splitlines() if line.strip()]
    return "TODO()" in lines and all(
        line == "TODO()" or (line.startswith("fn ") and line.endswith(":"))
        for line in lines
    )


def _headers(source):
    return [line.strip() for line in source.splitlines()
            if line.strip().startswith("fn ") and line.strip().endswith(":")]


def validate(packet=PACKET, compiler=None):
    packet = Path(packet).resolve()
    manifest, oracle = _load(packet)
    findings = []
    cases = []
    hash_lock = json.loads((packet / "packet-hashes.json").read_text())
    tasks = manifest.get("tasks", [])
    if (not isinstance(tasks, list) or not tasks or
            any(not isinstance(t, dict) or not isinstance(t.get("id"), str) or
                not re.fullmatch(r"[a-z][a-z0-9_]*", t["id"]) for t in tasks) or
            len({t["id"] for t in tasks}) != len(tasks) or
            set(oracle.get("cases", {})) != {t["id"] for t in tasks}):
        raise ValueError("task and private oracle IDs differ")
    if (manifest.get("context_budget_tokens", 0) <= 0 or
            manifest.get("time_budget_seconds", 0) <= 0 or
            manifest.get("token_budget", 0) <= 0):
        raise ValueError("shared budgets must be positive")
    if manifest.get("allowed_tools") != ["syntax-pilot emit-rust", "syntax-pilot build"]:
        raise ValueError("paired variants require the same normalized compiler tools")

    expected_sources = {f"{variant}/{task['id']}.cg" for task in tasks
                        for variant in ("indent", "brace")}
    hash_paths = {"packet.json", "private/oracle.json"}
    hash_paths.update(f"candidate/{name}" for name in expected_sources)
    hash_paths.update(f"private/reference/{name}" for name in expected_sources)
    if (hash_lock.get("schema_version") != 1 or
            set(hash_lock.get("sha256", {})) != hash_paths or
            any(sha((packet / name).read_bytes()) != expected
                for name, expected in hash_lock["sha256"].items())):
        raise ValueError("packet or private reference differs from frozen SHA-256")
    candidate = packet / "candidate"
    reference = packet / "private/reference"
    for root, label in ((candidate, "candidate"), (reference, "private reference")):
        actual_sources = {str(path.relative_to(root)) for path in root.rglob("*")
                          if path.is_file() or path.is_symlink()}
        if (actual_sources != expected_sources or root.is_symlink() or
                any(path.is_symlink() for path in root.rglob("*"))):
            raise ValueError(f"{label} tree has unexpected files or symlinks")

    with tempfile.TemporaryDirectory(prefix="cgx2-syntax-") as tmp:
        for task in tasks:
            name = task["id"]
            starter_paths = {variant: f"candidate/{variant}/{name}.cg"
                             for variant in ("indent", "brace")}
            reference_paths = {variant: f"private/reference/{variant}/{name}.cg"
                               for variant in ("indent", "brace")}
            if (task.get("starter_variants") != starter_paths or
                    task.get("reference_variants") != reference_paths or
                    not isinstance(task.get("public_api"), str) or not task["public_api"] or
                    not isinstance(task.get("expected_behavior"), str) or
                    not task["expected_behavior"] or
                    not isinstance(task.get("example_args"), list) or
                    any(not isinstance(arg, str) for arg in task["example_args"])):
                raise ValueError(f"invalid paired task contract: {name}")
            starter_indent = (packet / starter_paths["indent"]).read_text()
            starter_brace = (packet / starter_paths["brace"]).read_text()
            reference_indent = (packet / reference_paths["indent"]).read_text()
            reference_brace = (packet / reference_paths["brace"]).read_text()
            normalized_starter = normalize_brace(starter_brace)
            normalized_reference = normalize_brace(reference_brace)
            starter_parity = normalized_starter == starter_indent
            reference_parity = normalized_reference == reference_indent
            if not starter_parity:
                findings.append(f"{name}:starter_syntax_mismatch")
            if not reference_parity:
                findings.append(f"{name}:reference_syntax_mismatch")
            if (not _starter_shape(starter_indent) or
                    "TODO()" in reference_indent or
                    starter_indent == reference_indent or
                    _headers(starter_indent) != _headers(reference_indent)):
                findings.append(f"{name}:starter_leaks_or_changes_reference_api")
            row = {"task_id": name,
                   "indent_source_bytes": len(starter_indent.encode()),
                   "brace_source_bytes": len(starter_brace.encode()),
                   "normalized_sha256": sha(normalized_starter.encode()),
                   "indent_sha256": sha(starter_indent.encode()),
                   "same_semantic_units": starter_parity and reference_parity,
                   "starter_parity": starter_parity,
                   "private_reference_parity": reference_parity,
                   "compiler_check": "unrun"}
            if compiler is not None and reference_parity:
                outputs = []
                for variant, content in (("indent", reference_indent),
                                         ("brace", normalized_reference)):
                    source_path = Path(tmp) / f"{name}-{variant}.cg"
                    source_path.write_text(content)
                    rust_path = Path(tmp) / f"{name}-{variant}.rs"
                    emitted = _run([str(compiler), "emit-rust", str(source_path),
                                    "--out", str(rust_path)])
                    if emitted.returncode:
                        findings.append(f"{name}:{variant}_emit_failed")
                        break
                    binary = Path(tmp) / f"{name}-{variant}"
                    built = _run([str(compiler), "build", str(source_path),
                                  "--out", str(binary)], timeout=120)
                    if built.returncode:
                        findings.append(f"{name}:{variant}_build_failed")
                        break
                    run = _run([str(binary), *task["example_args"]])
                    expected = oracle["cases"][name]
                    accepted = (run.returncode == expected["exit_status"] and
                                run.stdout.decode("utf-8", errors="replace") == expected["stdout"] and
                                (not expected.get("stderr_contains") or
                                 expected["stderr_contains"].encode() in run.stderr))
                    outputs.append((sha(rust_path.read_bytes()), run.returncode,
                                    run.stdout, accepted))
                if len(outputs) == 2:
                    row["compiler_check"] = ("pass" if outputs[0] == outputs[1] and
                                             outputs[0][3] else "fail")
                    if row["compiler_check"] == "fail":
                        findings.append(f"{name}:compiler_or_acceptance_mismatch")
            cases.append(row)
    return {"status": "invalid" if findings else ("pass" if compiler else "unrun"),
            "case_count": len(cases), "findings": findings, "cases": cases,
            "shared_context_budget_tokens": manifest["context_budget_tokens"],
            "shared_token_budget": manifest["token_budget"],
            "shared_time_budget_seconds": manifest["time_budget_seconds"],
            "independent_human_review": "unrun"}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--packet", type=Path, default=PACKET)
    parser.add_argument("--compiler", type=Path)
    args = parser.parse_args(argv)
    result = validate(args.packet, args.compiler)
    print(json.dumps(result, indent=2, sort_keys=True))
    return 1 if result["status"] == "invalid" else 0


if __name__ == "__main__":
    raise SystemExit(main())
