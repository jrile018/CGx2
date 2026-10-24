#!/usr/bin/env python3
"""Focused frozen-binary public API assertions with incremental evidence."""
import argparse
import hashlib
import json
from pathlib import Path
import resource
import sys

from command import ROOT, HERE, environment, supervisor


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def limits():
    for kind, value in [(resource.RLIMIT_CPU, 30), (resource.RLIMIT_AS, 2 * 1024**3),
                        (resource.RLIMIT_FSIZE, 16 * 1024**2), (resource.RLIMIT_NOFILE, 128)]:
        resource.setrlimit(kind, (value, value))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary_dir.resolve() / "cgx2"
    manifest = json.loads((args.binary_dir / "manifest.json").read_text())
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=False)
    work = Path(environment()["TMPDIR"]) / ("product-" + output.name)
    work.mkdir(exist_ok=False)
    report = dict(schema_version=1, assertions=[], commands=[], failures=[], cwd=str(work),
                  manifest_sha256=digest(args.binary_dir / "manifest.json"),
                  limits=dict(wall_seconds=35, cpu_seconds=30, address_bytes=2 * 1024**3,
                              file_bytes=16 * 1024**2, pipe_bytes_per_stream=1024**2))

    def save():
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")

    def check(label, condition):
        report["assertions"].append(dict(label=label, passed=bool(condition)))
        save()
        assert condition, label

    def run(label, argv, expected=0):
        command = [str(binary), *map(str, argv)]
        result = supervisor.supervise(command, cwd=work, env=environment(), limits=limits,
                                      cap=1024**2, timeout=35)
        report["commands"].append(dict(label=label, argv=command, **supervisor.evidence(result)))
        save()
        check(label + ":exit", result["reason"] == "exit" and result["exit"] == expected)
        return result

    def api(label, path):
        result = run(label, ["api", "--json", path])
        check(label + ":stderr", not result["stderr"])
        return json.loads(result["stdout"])

    def fixture(name, text):
        destination = work / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(text)
        retained = output / "sources" / name
        retained.parent.mkdir(parents=True, exist_ok=True)
        retained.write_text(text)
        return destination

    def case(name, action):
        try:
            action()
        except Exception as error:
            report["failures"].append(dict(case=name, error=repr(error)))
            save()

    def unchanged():
        for name, expected in manifest["artifact_sha256"].items():
            check("frozen:" + name, digest(args.binary_dir / name) == expected)

    case("before-hashes", unchanged)
    source = (HERE / "fixtures/public.cg").read_text()
    ordinary = fixture("ordinary.cg", source)
    scalar = fixture("scalar.cg", source[:source.index("fn private_helper")]
                     + "fn main() -> i64:\n    exported()\n")

    def compare_file():
        index = api("ordinary-json", ordinary)
        baseline = api("scalar-json", scalar)
        for key in ["module", "functions", "records", "variants"]:
            check("equivalent-declarations:" + key, index[key] == baseline[key])
        check("checked-ordinary", index["semantics_status"] == "checked-ordinary-memory-v1")
        check("unknown-absent-summary", index["absent_summary"] == "unknown")
        check("original-public-generics", [f["name"] for f in index["functions"]] == ["exported", "id", "unused"])
        check("original-generic-type", index["functions"][1]["return_type"] == "T")
        check("constraints", index["functions"][1]["type_parameters"][0]["constraints"] == ["Eq", "Show"])
        for label, path in [("ordinary-text", ordinary), ("scalar-text", scalar)]:
            text = run(label, ["api", path])["stdout"].decode()
            report[label] = text
            check(label + ":no-private", all(s not in text for s in ["private_helper", "Secret", "main", "$cgx2$"]))
        # Public source declarations remain distinct from concrete instantiation
        # summaries, whose absence is explicitly unknown in this versioned view.
        baseline_text = "\n".join(line for line in report["scalar-text"].splitlines()
                                  if not line.startswith("instantiate "))
        check("equivalent-human-declarations", report["ordinary-text"].replace("checked-ordinary-memory-v1", "checked").rstrip() == baseline_text)
        effects = json.loads(run("whole-program-effects", ["memory-effects", ordinary])["stdout"])
        functions = {f["name"]: f for f in effects["functions"]}
        check("compatible-memory-schema", effects["schema_version"] == 2)
        for name in ["private_helper", "main"]:
            check("private-transitive-effects:" + name, functions[name]["memory_effects_v2"]["may_enter_guard"] is True)
        check("no-private-json", all(s not in json.dumps(index) for s in ["private_helper", "Secret", "id<i64>", "$cgx2$"]))

    case("file", compare_file)

    def packages():
        values = []
        for label in ["ordinary", "scalar"]:
            library = source[:source.index("fn private_helper")]
            main = "module app.main\nimport app.api\nfn main() -> i64"
            if label == "ordinary":
                library = source.replace("fn main()", "fn internal_entry()")
                main += " effects(ManagedAlloc):\n    match memory.try_no_gc(budget: 4096):\n        Result.Ok(guard):\n            app.api.exported()\n        Result.Err(error):\n            -1\n"
            else:
                main += ":\n    app.api.exported()\n"
            root = fixture(label + "-package/cgx2.toml", 'schema_version = 1\nname = "api"\nentry = "main.cg"\nsources = ["main.cg", "lib.cg"]\n').parent
            fixture(label + "-package/lib.cg", library)
            fixture(label + "-package/main.cg", main)
            values.append(api(label + "-package-json", root))
            text = run(label + "-package-text", ["api", root])["stdout"].decode()
            check(label + "-package:no-private-text", "private_helper" not in text and "internal_entry" not in text)
            check(label + "-package:source-path", "source lib.cg" in text)
        index, baseline = values
        for left, right in zip(index["modules"], baseline["modules"]):
            check("package:path:" + left["source"], left["source"] == right["source"])
            for key in ["module", "functions", "records", "variants"]:
                default = [] if key != "module" else None
                check("package:" + left["source"] + ":" + key, left["api"].get(key, default) == right["api"].get(key, default))
        check("package-manifest", index["package"]["manifest_sha256"] == digest(work / "ordinary-package/cgx2.toml"))
        check("package-entry", index["entry"] == index["package"]["entry"] == "main.cg")
        check("package-sources", len(index["package"]["source_hashes"]) == 2)
        check("package:qualified-types", index["modules"][0]["api"]["records"][0]["identity"] == "app.api.Visible")
        effects = json.loads(run("package-memory-effects", ["memory-effects", work / "ordinary-package"])["stdout"])
        check("package:private-effects", any(f["name"] == "app.api.private_helper" for f in effects["functions"]))

    case("package", packages)

    def negatives():
        invalid = source + "\nfn invalid() -> i64:\n    true\n"
        bad = fixture("invalid.cg", invalid)
        for label, path in [("file", bad), ("package", work / "ordinary-package")]:
            if label == "package":
                # Invalid entry retains available exact source attribution;
                # dependency-file attribution is a separately deferred defect.
                entry = "module app.main\nimport app.api\n" + invalid.replace("module app.api\n", "")
                fixture("ordinary-package/main.cg", entry)
            result = run(label + ":invalid", ["--diagnostic-format=json", "api", "--json", path], 1)
            check(label + ":invalid-no-output", not result["stdout"])
            diagnostics = [json.loads(line) for line in result["stderr"].splitlines()]
            check(label + ":invalid-code", any(d["code"] == "CGT003" and d["message"] == "expected i64, found Bool" for d in diagnostics))
            original = invalid if label == "file" else entry
            start = original.rindex("true")
            check(label + ":invalid-span", any(d.get("span") == {"start": start, "end": start + 4} for d in diagnostics))
            check(label + ":invalid-file", any(d["file"].endswith("invalid.cg" if label == "file" else "main.cg") for d in diagnostics))
        syntax = json.loads(run("explicit-syntax-only", ["api", "--syntax-only", "--json", bad])["stdout"])
        check("syntax-only-remains-unchecked", syntax["semantics_status"] == "syntax-only")
        uninstantiated = fixture("invalid-generic.cg", source + "\nfn invalid<T>(x: T) -> i64:\n    true\n")
        result = run("invalid-uninstantiated-generic", ["api", "--json", uninstantiated], 1)
        check("generic-invalid-diagnostic", b"CGT003" in result["stderr"] and not result["stdout"])

    case("negative", negatives)

    def repro():
        path = fixture("om08.cg", (HERE / "fixtures/om08.cg").read_text())
        index = api("om08-json", path)
        check("om08-public-record", index["records"][0]["name"] == "PublicBox")
        check("om08-only-public-function", [f["name"] for f in index["functions"]] == ["exported"])
        text = run("om08-text", ["api", path])["stdout"].decode()
        check("om08-human", "PublicBox" in text and "secret" not in text and "main" not in text)
        region = (ROOT / "crates/cgx2-syntax/ordinary-memory-evidence/fixtures/region.cg").read_text().replace("fn build", "pub fn build", 1)
        path = fixture("region.cg", region)
        index = api("region-json", path)
        check("region-signature", "memory.Value<R, List<i64>>" in index["functions"][0]["return_type"])
        text = run("region-text", ["api", path])["stdout"].decode()
        check("region-human", "R: memory.RegionName" in text and "memory.Value<R, List<i64>>" in text)

    case("reproductions", repro)
    case("after-hashes", unchanged)
    report["passed"] = sum(r["passed"] for r in report["assertions"])
    report["failed"] = len(report["failures"])
    save()
    print(json.dumps({k: report[k] for k in ["passed", "failed", "failures"]}))
    return bool(report["failures"])


if __name__ == "__main__":
    sys.exit(main())
