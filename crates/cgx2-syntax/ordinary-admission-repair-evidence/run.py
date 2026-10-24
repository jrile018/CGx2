#!/usr/bin/env python3
"""Core-admission source/package parity; incremental bounded evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE.parent / "owned-result-runner-evidence"))
from supervisor import supervise, evidence


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def limits():
    resource.setrlimit(resource.RLIMIT_AS, (4 * 1024**3, 4 * 1024**3))
    resource.setrlimit(resource.RLIMIT_CPU, (120, 120))
    resource.setrlimit(resource.RLIMIT_FSIZE, (512 * 1024**2, 512 * 1024**2))


def region(body, declaration="effects(ManagedAlloc)"):
    return f"fn main() -> i64 {declaration}:\n    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n{body}\n        Result.Err(error):\n            -1\n"


def fixtures():
    original = ROOT / "plans/release-completion/s2b-ordinary-memory-review/repros"
    valid = {
        "user-borrow": ((original / "user-borrow.cg").read_text(), "42\n"),
        "legacy-guard": ((original / "legacy-guard-panic.cg").read_text(), "42\n"),
        "nested-assignment": (region("            let mut n = 40\n            no_gc(budget: 4294967296):\n                no_gc(budget: 1048576):\n                    if true:\n                        n += 2\n                    else:\n                        n += 0\n                    return n\n            0", "effects(ManagedAlloc, Mutate)"), "42\n"),
        "user-err-local": ("record Box:\n    text: String\nvariant Message:\n    Err(Box)\n" + region("            match memory.try_in(borrow(r), Message.Err(Box(text: \"secret\"))):\n                Result.Ok(value):\n                    match value:\n                        Message.Err(box):\n                            length(box.text)\n                Result.Err(error):\n                    0"), "6\n"),
        "copy": (region("            match memory.try_in(borrow(r), [42]):\n                Result.Ok(xs):\n                    match memory.copy_to(memory.default_region(), xs):\n                        Result.Ok(copy):\n                            length(copy) + 41\n                        Result.Err(error):\n                            -3\n                Result.Err(error):\n                    -2"), "42\n"),
        "session-shadow": ("module managed\nfn ref(value: String) -> String effects() caps():\n    value\nfn collect(value: i64) -> i64 effects() caps():\n    value\nfn main() -> i64 effects(ManagedAlloc) caps():\n    match memory.try_session():\n        Result.Ok(session):\n            42\n        Result.Err(error):\n            -1\n", "42\n"),
    }
    shadow = "module managed\npub fn ref(value: String) -> String effects() caps():\n    value\npub fn collect(value: i64) -> i64 effects() caps():\n    value + 1\n"
    app = "module app\nimport managed\n" + region("            memory.collect(borrow(r))\n            length(managed.ref(\"x\")) + managed.collect(40)", "effects(ManagedAlloc, Collect) caps(MemoryCollect)")
    packages = {
        "shadow-package": ({"managed.cg": shadow, "main.cg": app}, "42\n"),
        "borrow-package": ({"util.cg": "module util\npub fn borrow(value: i64) -> i64 effects():\n    value + 1\n", "main.cg": "module app\nimport util\n" + region("            match memory.try_in(borrow(r), [util.borrow(41)]):\n                Result.Ok(xs):\n                    length(xs) + util.borrow(40)\n                Result.Err(error):\n                    -2")}, "42\n"),
    }
    user_effects = "module managed\npub fn ref(value: i64) -> i64 effects(ReadArgs) caps(ArgsRead):\n    arg_count()\npub fn collect() -> i64 effects(ReadArgs) caps(ArgsRead):\n    arg_count()\n"
    user_calls = "module app\nimport managed\n" + region("            managed.ref(0) + managed.collect() + 42", "effects(ManagedAlloc, ReadArgs) caps(ArgsRead)")
    packages["user-effects-package"] = ({"managed.cg": user_effects, "main.cg": user_calls}, "42\n")
    leak = (original / "user-err-region-escape.cg").read_text().replace("secret", "秘密")
    invalid = {
        "user-err-escape": (leak, [("CGT095", "return text\n")]),
        "field-err-escape": (leak.replace("variant Message:\n    Err(String)", "record Box:\n    text: String\nvariant Message:\n    Err(Box)").replace('Message.Err("秘密")', 'Message.Err(Box(text: "秘密"))').replace("Message.Err(text):", "Message.Err(box):").replace("return text", "return box.text"), [("CGT095", "return box.text\n")]),
        "builtin-result-heap-error": (leak.replace("variant Message:\n    Err(String)\n", "").replace('Message.Err("秘密")', 'Result<i64, String>.Err("秘密")').replace("Message.Err(text):\n                            return text", "Result.Err(text):\n                            return text\n                        Result.Ok(n):\n                            return \"ok\""), [("CGT095", "return text\n")]),
        "region-effects": ("module managed\nfn ref(x: i64) -> i64 effects():\n    x\n" + region("            42", "effects() caps()"), [("CGT012", "memory.try_region(mode: memory.Mode.Manual, limit: 4096)")]),
    }
    for name, operation in [
        ("guard-effects", "memory.try_no_gc(budget: 4096)"),
        ("session-effects", "memory.try_session()"),
        ("try-in-effects", "memory.try_in(memory.default_region(), 42)"),
        ("copy-effects", "memory.copy_to(memory.default_region(), 42)"),
    ]:
        text = f"module managed\nfn ref(x: i64) -> i64 effects():\n    x\nfn main() -> i64 effects() caps():\n    match {operation}:\n        Result.Ok(value):\n            42\n        Result.Err(error):\n            -1\n"
        invalid[name] = (text, [("CGT012", operation)])
    invalid_packages = {}
    # The shadowed module is intentionally type-incompatible with template calls.
    # Requirements must come solely from compiler operations, through hidden().
    for name, signature, expected in [
        ("collect-effect", "effects(ManagedAlloc) caps(MemoryCollect)", [("CGT012", "hidden()")]),
        ("collect-capability", "effects(Collect) caps()", [("CGT013", "hidden()")]),
        ("region-transitive", "effects() caps()", [("CGT012", "hidden()")]),
    ]:
        operation = ("    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n            42\n        Result.Err(e):\n            -1\n" if name == "region-transitive" else "    memory.collect(memory.default_region())\n    42\n")
        text = f"module app\nimport managed\nfn hidden() -> i64:\n{operation}fn main() -> i64 {signature}:\n    hidden()\n"
        invalid_packages[name] = ({"managed.cg": shadow, "main.cg": text}, expected)
    invalid_packages["user-effects-retained"] = (
        {"managed.cg": user_effects, "main.cg": user_calls.replace("effects(ManagedAlloc, ReadArgs) caps(ArgsRead)", "effects(ManagedAlloc) caps()")},
        [("CGT012", "managed.ref(0)"), ("CGT013", "managed.ref(0)"),
         ("CGT012", "managed.collect()"), ("CGT013", "managed.collect()")])
    return valid, packages, invalid, invalid_packages


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    binary, output = args.binary_dir.resolve(), args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=False)
    (output / "runner-snapshot.py").write_bytes(Path(__file__).read_bytes())
    fixture_dir = output / "fixtures"
    fixture_dir.mkdir()
    outside = output / "outside"
    outside.mkdir()
    env = dict(os.environ, RUSTUP_TOOLCHAIN="1.98.1", PYTHONDONTWRITEBYTECODE="1")
    wrapper = output / "rustc-two-threads"
    wrapper.write_text('#!/bin/sh\nexec rustc -C link-arg=-Wl,--threads=2 "$@"\n')
    wrapper.chmod(0o700)
    artifacts = [p for p in binary.iterdir() if p.is_file()]
    before = {p.name: digest(p) for p in artifacts}
    report = {"schema_version": 1, "commands": [], "assertions": [],
              "bounds": {"wall_seconds": 180, "cpu_seconds": 120,
                         "address_space_bytes": 4 * 1024**3,
                         "file_size_bytes": 512 * 1024**2,
                         "retained_bytes_per_stream": 2 * 1024**2,
                         "aggregate_isolation": False},
              "frozen_before": before, "runner_sha256": digest(Path(__file__)),
              "checkpoint_complete": False, "release_complete": False}

    def save():
        report["passed"] = sum(a["passed"] for a in report["assertions"])
        report["failed"] = sum(not a["passed"] for a in report["assertions"])
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")

    def check(label, passed):
        report["assertions"].append({"name": label, "passed": bool(passed)})
        save()

    def run(label, argv, cwd=ROOT):
        result = supervise([str(a) for a in argv], cwd=str(cwd), env=env,
                           limits=limits, cap=2 * 1024**2, timeout=180)
        record = dict(name=label, argv=[str(a) for a in argv], cwd=str(cwd), **evidence(result))
        report["commands"].append(record)
        save()
        return record

    def normal(record, expected=0):
        return record["reason"] == "exit" and record["exit"] == expected and record.get("signal") is None

    def compiler_input(source):
        return ["--package", source] if source.is_dir() else [source]

    def materialize(name, text):
        if isinstance(text, dict):
            directory = fixture_dir / name
            directory.mkdir()
            manifest = 'schema_version = 1\nname = "admission"\nentry = "main.cg"\nsources = ' + json.dumps(sorted(text)) + '\n'
            for file, source in dict(text, **{"cgx2.toml": manifest}).items():
                (directory / file).write_text(source)
            return directory
        path = fixture_dir / (name + ".cg")
        path.write_text(text)
        return path

    valid, packages, invalid, invalid_packages = fixtures()
    for name, (text, expected) in dict(valid, **packages).items():
        source = materialize(name, text)
        print("valid", name, flush=True)
        r = run(name + ":check", [binary / "cgx2", "check", source])
        check(name + ":check", normal(r))
        r = run(name + ":interpreter", [binary / "cgx2", "run", source])
        check(name + ":interpreter", normal(r) and r["stdout"] == expected)
        for mode in ["debug", "release", "standalone"]:
            artifact = output / (name + "-" + mode)
            if mode == "standalone":
                rust = outside / (name + ".rs")
                r = run(name + ":emit", [binary / "cgx2c", "emit-rust", *compiler_input(source), "--out", rust])
                check(name + ":emit", normal(r))
                command = [wrapper, "--edition=2021", rust, "-o", artifact]
            else:
                command = [binary / "cgx2c", "build", *compiler_input(source), "--out", artifact, "--rustc", wrapper]
                if mode == "release":
                    command.append("--release")
            r = run(name + ":" + mode + "-build", command, outside)
            check(name + ":" + mode + "-build", normal(r))
            if normal(r):
                r = run(name + ":" + mode, [artifact], outside)
                check(name + ":" + mode, normal(r) and r["stdout"] == expected)
        # Unsupported Python must preserve an existing artifact on every valid input.
        sentinel = output / (name + ".py")
        sentinel.write_bytes(b"existing Python artifact\n")
        old = digest(sentinel)
        r = run(name + ":python", [binary / "cgx2c", "emit-python", *compiler_input(source), "--out", sentinel, "--force"])
        check(name + ":python", normal(r, 1) and "CGE006" in r["stderr"])
        check(name + ":python-preserved", digest(sentinel) == old)

    for name, (text, expected) in dict(invalid, **invalid_packages).items():
        source = materialize(name, text)
        original = text["main.cg"] if isinstance(text, dict) else text
        diagnostic = []
        for code, needle in expected:
            # hidden() appears in the declaration too; the final call is the site.
            index = original.rfind(needle) if needle == "hidden()" else original.index(needle)
            start = len(original[:index].encode())
            diagnostic.append((code, {"start": start, "end": start + len(needle.encode())}))
        print("invalid", name, flush=True)
        for mode, command in [
            ("check", [binary / "cgx2", "--diagnostic-format=json", "check", source]),
            ("interpreter", [binary / "cgx2", "--diagnostic-format=json", "run", source]),
        ]:
            r = run(name + ":" + mode, command)
            try:
                raw = r["stdout"] or r["stderr"]
                try:
                    rows = json.loads(raw)
                except json.JSONDecodeError:
                    rows = [json.loads(line) for line in raw.splitlines() if line]
                if isinstance(rows, dict):
                    rows = rows.get("diagnostics", [rows])
                actual = [(e.get("code"), e.get("span")) for e in rows]
            except (ValueError, TypeError):
                actual = None
            check(name + ":" + mode + "-exact", normal(r, 1) and actual == diagnostic)
        for mode in ["debug", "release", "standalone"]:
            sentinel = output / (name + "-" + mode)
            sentinel.write_bytes(b"existing rejected artifact\n")
            old = digest(sentinel)
            if mode == "standalone":
                command = [binary / "cgx2c", "emit-rust", *compiler_input(source), "--out", sentinel, "--force"]
            else:
                command = [binary / "cgx2c", "build", *compiler_input(source), "--out", sentinel, "--force", "--rustc", wrapper]
                if mode == "release":
                    command.append("--release")
            r = run(name + ":" + mode, command)
            # cgx2c preserves the first semantic failure in its human format;
            # cgx2 above reports every diagnostic as JSON Lines.
            check(name + ":" + mode + "-reject", normal(r, 1) and diagnostic[0][0] in r["stderr"])
            check(name + ":" + mode + "-preserved", digest(sentinel) == old)
    report["source_sha256"] = {str(p.relative_to(output)): digest(p) for p in fixture_dir.rglob("*") if p.is_file()}
    report["frozen_after"] = {p.name: digest(p) for p in artifacts}
    check("frozen-unchanged", report["frozen_after"] == before)
    print(json.dumps({k: report[k] for k in ("passed", "failed")}))
    return bool(report["failed"])


if __name__ == "__main__":
    sys.exit(main())
