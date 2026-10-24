"""Reproduce S2b visibility product checks using absolute paths and a private cwd."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=False)
    fixtures = Path(__file__).resolve().parent / "fixtures"
    results = []
    failures = []

    def run(label, command, *, expected=None, error=None):
        command = [str(p) for p in command]
        completed = subprocess.run(command, cwd=output, capture_output=True,
                                   text=True, timeout=120)
        passed = (completed.returncode == 0 if error is None else
                  completed.returncode != 0 and error in completed.stderr
                  and not completed.stdout)
        if expected is not None:
            passed = passed and completed.stdout == expected
        record = {"label": label, "command": command, "cwd": str(output),
                  "exit_code": completed.returncode, "passed": passed,
                  "stdout": completed.stdout, "stderr": completed.stderr}
        results.append(record)
        assert passed, record
        return completed.stdout

    def check(label, condition):
        results.append({"label": label, "passed": bool(condition)})
        assert condition, label

    def case(label, action):
        try:
            action()
        except (AssertionError, subprocess.TimeoutExpired) as error:
            failures.append({"case": label, "error": str(error)})

    def api_case(name, source):
        text = run(name + ":text", [binary, "api", source])
        run(name + ":text-repeat", [binary, "api", source], expected=text)
        raw = run(name + ":json", [binary, "api", source, "--json"])
        run(name + ":json-repeat", [binary, "api", source, "--json"], expected=raw)
        check(name + ":private-names-absent", all(
            token not in text + raw for token in
            ["Secret", "private_id", "api.hidden", "dep_private", "dep_hidden", "app_hidden"]))
        document = json.loads(raw)
        apis = [m["api"] for m in document["modules"]] if source.is_dir() else [document]
        for api in apis:
            check(name + ":checked-schema7:" + api["module"],
                  api["schema_version"] == 7 and api["semantics_status"] == "checked")
        if name == "reviewed":
            check("reviewed:no-instantiations", not document["instantiations"])
            return
        instances = [i for api in apis for i in api["instantiations"]]
        check(name + ":instance-count", len(instances) == (5 if name == "source" else 6))
        identity = "api.Visible" if name == "source" else "dep.Value"
        complete = f"cgx2.core.Option<cgx2.core.Result<List<Array<{identity}>>, Bool>>"
        check(name + ":complete-nested-type", any(
            i["type_arguments"] == [complete] and i["parameters"] == [complete]
            and i["return_type"] == complete for i in instances))
        binder = "List<api.nested::U>" if name == "source" else "Array<app.wrap::U>"
        check(name + ":scoped-owner-binder", any(i["return_type"] == binder for i in instances))
        check(name + ":constraints", all(api["functions"][0]["type_parameters"][0]["constraints"] == ["Eq", "Show"]
                                         for api in apis if api["module"] in ["api", "dep"]))

    def syntax_case():
        source = fixtures / "s2bf-visibility.cg"
        text = run("syntax:text", [binary, "api", source, "--syntax-only"])
        raw = run("syntax:json", [binary, "api", source, "--syntax-only", "--json"])
        run("syntax:json-repeat", [binary, "api", source, "--syntax-only", "--json"], expected=raw)
        run("syntax:text-repeat", [binary, "api", source, "--syntax-only"], expected=text)
        api = json.loads(raw)
        check("syntax:unchecked-schema6", api["schema_version"] == 6
              and api["semantics_status"] == "syntax-only" and "instantiations" not in api
              and all(f["effects_status"] == "unchecked" and f["capabilities_status"] == "unchecked"
                      for f in api["functions"]) and "instantiate " not in text)

    def execute_case(name, source):
        run(name + ":check", [binary, "check", source])
        case(name + ":interpreter", lambda: run(name + ":interpreter", [binary, "run", source], expected="7\n"))
        for profile in ["debug", "release"]:
            executable = output / (name + "-" + profile)
            def native_case():
                run(name + ":build-" + profile, [binary, "build", source, "--out", executable]
                    + (["--release"] if profile == "release" else []))
                run(name + ":native-" + profile, [executable], expected="7\n")
            case(name + ":native-" + profile, native_case)
        emitted = output / (name + (".pyz" if source.is_dir() else ".py"))
        def python_case():
            run(name + ":emit-python", [binary, "emit", source, "--target", "python", "--out", emitted])
            run(name + ":python", ["python3", emitted], expected="7\n")
        case(name + ":python", python_case)
        rust = output / (name + ".rs")
        native = output / (name + "-rust")
        def rust_case():
            run(name + ":emit-rust", [binary, "emit", source, "--target", "rust", "--out", rust])
            run(name + ":compile-rust", ["rustc", "--edition=2021", rust, "-o", native])
            run(name + ":rust", [native], expected="7\n")
        case(name + ":rust", rust_case)

    def reject_case(name, source, error):
        for command in ["api", "check", "run"]:
            run(name + ":" + command, [binary, command, source], error=error)
        run(name + ":json", [binary, "api", source, "--json"], error=error)
        for target in ["native", "rust", "python"]:
            artifact = output / (name + "-reject-" + target)
            sentinel = b"previous reviewed artifact\n"
            artifact.write_bytes(sentinel)
            command = ([binary, "build", source] if target == "native" else
                       [binary, "emit", source, "--target", target])
            run(name + ":reject-" + target, command + ["--out", artifact, "--force"], error=error)
            check(name + ":preserved-" + target, artifact.read_bytes() == sentinel)

    try:
        for name, fixture in [("reviewed", "s2bf-private.cg"), ("source", "s2bf-visibility.cg"),
                              ("package", "s2bf-package")]:
            case(name + ":api", lambda name=name, fixture=fixture: api_case(name, fixtures / fixture))
        case("syntax", syntax_case)
        for name, fixture in [("source", "s2bf-visibility.cg"), ("package", "s2bf-package")]:
            case(name + ":execution", lambda name=name, fixture=fixture: execute_case(name, fixtures / fixture))
        for name, fixture in [("public-source", "s2bf-public.cg"), ("public-package", "s2bf-public-package"),
                              ("public-values", "s2bf-public-values.cg")]:
            case(name + ":execution", lambda name=name, fixture=fixture: execute_case(name, fixtures / fixture))
        case("invalid-public-signature", lambda: reject_case("invalid", fixtures / "s2bf-invalid.cg", "CGT022"))
        for name, expression, error in [("private-callee", "dep.dep_private<i64>(1)", "CGT009"),
                                        ("private-type", "dep.id<dep.DepSecret>(dep.DepSecret(number: 1))", "CGT022")]:
            package = output / name
            shutil.copytree(fixtures / "s2bf-package", package)
            (package / "src/main.cg").write_text("module app\nimport dep\npub fn main() -> i64:\n    let hidden = "
                                                 + expression + "\n    7\n")
            case(name, lambda name=name, package=package, error=error: reject_case(name, package, error))
    finally:
        report = {"kind": "local source/package/target conformance; no release acceptance",
                  "base": "b595724e1b086c604220cc5b165c141e65a0531f",
                  "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                  "fixtures_sha256": {str(p.relative_to(fixtures)): hashlib.sha256(p.read_bytes()).hexdigest()
                                      for p in sorted(fixtures.rglob("*")) if p.is_file() and any(
                                          part.startswith("s2bf") for part in p.relative_to(fixtures).parts)},
                  "environment": {k: os.environ.get(k) for k in ["RUSTUP_TOOLCHAIN", "CARGO_BUILD_JOBS",
                                    "CARGO_TARGET_DIR", "TMPDIR", "LIBRARY_PATH"]},
                  "results": results, "failures": failures}
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"{sum(r['passed'] for r in results)}/{len(results)} checks passed; {len(failures)} failed cases; {output / 'report.json'}")
    raise SystemExit(bool(failures))


if __name__ == "__main__":
    main()
