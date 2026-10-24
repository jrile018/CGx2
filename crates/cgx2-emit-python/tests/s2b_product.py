#!/usr/bin/env python3
"""Bounded emitter repair product checks; no timing or external dependencies."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    options = parser.parse_args()
    binary = options.binary_dir.resolve() / "cgx2"
    fixtures = Path(__file__).resolve().parent / "fixtures/s2b-repair"
    records = []
    with tempfile.TemporaryDirectory(prefix="s2bf-products-", dir=os.environ["TMPDIR"]) as tmp:
        scratch = Path(tmp).resolve()

        def run(label, args, expected=None, code=None):
            result = subprocess.run([str(a) for a in args], cwd=scratch, capture_output=True)
            record = {"case": label, "command": [str(a) for a in args],
                      "exit": result.returncode, "stdout": result.stdout.decode(),
                      "stderr": result.stderr.decode()}
            records.append(record)
            if code:
                assert result.returncode != 0 and code in record["stderr"], record
                assert "panicked at" not in record["stderr"], record
            else:
                assert result.returncode == 0, record
                if expected is not None:
                    assert result.stdout == expected, record
            return result

        cases = {"local-option": ("fn size<T>(value: T) -> i64:\n    let found = get([value], 0)\n    length([value])\nfn main() -> i64:\n    size(1) + size(true)\n", b"2\n")}
        for name in ["local-types", "strings", "contracts"]:
            expected = 'line\n\r\t\\"é界💡\n'.encode() if name == "strings" else b"2\n"
            cases[name] = ((fixtures / f"{name}.cg").read_text(), expected)
        cases["reverse"] = (cases["local-types"][0].replace("size(1) + size(true)", "size(true) + size(1)"), b"2\n")
        package = scratch / "package"
        (package / "src").mkdir(parents=True)
        (package / "cgx2.toml").write_text('schema_version = 1\nname = "repair"\nentry = "src/main.cg"\nsources = ["src/main.cg", "src/local.cg"]\n')
        (package / "src/local.cg").write_text("module repair.local\n" + cases["local-option"][0].split("fn main()")[0].replace("fn size", "pub fn size"))
        (package / "src/main.cg").write_text("module repair.main\nimport repair.local\nfn main() -> i64:\n    repair.local.size(1) + repair.local.size(true)\n")
        for name, (source, expected) in [*cases.items(), ("package", (None, b"2\n"))]:
            path = package if source is None else scratch / f"{name}.cg"
            if source is not None:
                path.write_text(source)
            run(name + "/check", [binary, "check", path])
            run(name + "/run", [binary, "run", path], expected)
            for release in [False, True]:
                native = scratch / f"{name}-native-{release}"
                run(name + f"/build/{release}", [binary, "build", path, "--out", native] + (["--release"] if release else []))
                run(name + f"/native/{release}", [native], expected)
            rust = scratch / f"{name}.rs"
            run(name + "/emit/rust", [binary, "emit", path, "--target", "rust", "--out", rust])
            for release in [False, True]:
                native = scratch / f"{name}-standalone-{release}"
                run(name + f"/rustc/{release}", ["rustc", "--edition=2021", rust, "-o", native] + (["-O"] if release else []))
                run(name + f"/standalone/{release}", [native], expected)
            python = scratch / (f"{name}.pyz" if source is None else f"{name}.py")
            run(name + "/emit/python", [binary, "emit", path, "--target", "python", "--out", python])
            run(name + "/python", ["python3", python], expected)

        negatives = {
            "direct": 'fn main() -> String:\n    concat("\\x41", "")\n',
            "generic": 'fn bad<T>(value: T) -> i64:\n    let xs = [["\\u0041"]]\n    length([value])\nfn main() -> i64:\n    bad(true)\n',
            "requires": 'fn bad<T>(value: i64) -> i64:\n    requires length("\\101") == 1\n    length([value])\nfn main() -> i64:\n    bad<Bool>(1)\n',
            "ensures": 'fn bad<T>(value: i64) -> i64:\n    ensures length("\\q") == result\n    length([value])\nfn main() -> i64:\n    bad<i64>(1)\n',
        }
        for name, source in negatives.items():
            path = scratch / f"invalid-{name}.cg"
            path.write_text(source)
            run(name + "/source-check", [binary, "check", path])
            run(name + "/source-run", [binary, "run", path], code="CGR009")
            for action, flags, code in [
                ("emit", ["--target", "rust"], "CGRS003"),
                ("emit", ["--target", "python"], "CGE003"),
                ("build", [], "CGRS003"),
                ("build", ["--release"], "CGRS003"),
            ]:
                suffix = ".py" if flags == ["--target", "python"] else ".rs" if action == "emit" else ""
                output = scratch / ("preserved-artifact" + suffix)
                output.write_bytes(b"preserve-existing-artifact\n")
                run(name + "/reject/" + str(flags), [binary, action, path, *flags, "--out", output, "--force"], code=code)
                assert output.read_bytes() == b"preserve-existing-artifact\n"
        # Imported invalid literals retain their defining source file and byte span.
        imported = package / "src/local.cg"
        imported.write_text('module repair.local\npub fn size<T>(value: T) -> i64:\n    let bad = ["\\x41"]\n    length([value])\n')
        for target, code in [("rust", "CGRS003"), ("python", "CGE003")]:
            output = scratch / ("package-preserved.pyz" if target == "python" else "package-preserved.rs")
            output.write_bytes(b"preserve-package\n")
            result = run("package/reject/" + target, [binary, "emit", package, "--target", target, "--out", output, "--force"], code=code)
            assert b"local.cg" in result.stderr
            assert output.read_bytes() == b"preserve-package\n"

    versions = {name: subprocess.check_output(args, text=True).strip() for name, args in
                [("rustc", ["rustc", "--version"]), ("cargo", ["cargo", "--version"]), ("python", ["python3", "--version"])]}
    result = {"status": "passed", "versions": versions, "checks": len(records), "records": records,
              "fixture_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(fixtures.glob("*.cg"))}}
    options.output.resolve().parent.mkdir(parents=True, exist_ok=True)
    options.output.write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n")
    print(f"{len(records)} product checks passed")


if __name__ == "__main__":
    main()
