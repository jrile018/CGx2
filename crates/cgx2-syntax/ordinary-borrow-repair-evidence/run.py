#!/usr/bin/env python3
"""Nominal borrow fixtures through the unchanged admission parity engine."""
import importlib.util
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
path = HERE.parent / "ordinary-admission-repair-evidence/run.py"
spec = importlib.util.spec_from_file_location("admission_parity", path)
engine = importlib.util.module_from_spec(spec)
spec.loader.exec_module(engine)


def region(body):
    return engine.region(body)


def fixtures():
    nominal = "record borrow:\n    value: i64\n"
    generic = "record borrow<T>:\n    value: T\n"
    valid = {
        "retained-local": ((ROOT / "plans/release-completion/s2b-ordinary-memory-review/repros/core-nominal/record-borrow.cg").read_text(), "42\n"),
        "qualified-local": ("module app\n" + nominal + region("            let x = app.borrow(value: 42)\n            x.value"), "42\n"),
        "generic-heap": (generic + region("            let x = borrow<String>(value: \"秘密\")\n            length(x.value) + 40"), "42\n"),
        "inferred-generic": (generic + region("            let x = borrow(value: \"秘密\")\n            length(x.value) + 40"), "42\n"),
        "nested-heap": (generic + "record Box:\n    text: String\n" + region("            let x = borrow<Box>(value: Box(text: \"秘密\"))\n            let box = x.value\n            length(box.text) + 40"), "42\n"),
        "nested-borrow": (generic + region("            let x = borrow<borrow<i64>>(value: borrow<i64>(value: 42))\n            let inner = x.value\n            inner.value"), "42\n"),
        "user-function": ("fn borrow(value: i64) -> i64 effects():\n    value + 1\n" + region("            borrow(41)"), "42\n"),
        "genuine-capability": (region("            match memory.try_in(borrow(r), [42]):\n                Result.Ok(xs):\n                    length(xs) + 41\n                Result.Err(error):\n                    -2"), "42\n"),
    }
    library = "module util\npub record borrow<T>:\n    value: T\n"
    def package(body, lib=library):
        return {"util.cg": lib, "main.cg": "module app\nimport util\n" + region(body)}
    packages = {
        "imported-simple": (package("            match memory.try_in(borrow(r), util.borrow(value: 42)):\n                Result.Ok(x):\n                    x.value\n                Result.Err(error):\n                    -2", "module util\npub record borrow:\n    value: i64\n"), "42\n"),
        "imported-inferred": (package("            match memory.try_in(borrow(r), util.borrow(value: \"秘密\")):\n                Result.Ok(x):\n                    length(x.value) + 40\n                Result.Err(error):\n                    -2"), "42\n"),
        "imported-record": (package("            let x = util.borrow<i64>(value: 42)\n            x.value"), "42\n"),
        "contextual-imported": (package("            match memory.try_in(borrow(r), util.borrow<String>(value: \"秘密\")):\n                Result.Ok(x):\n                    length(x.value) + 40\n                Result.Err(error):\n                    -2"), "42\n"),
        "nested-imported": (package("            match memory.try_in(borrow(r), util.borrow<util.Box>(value: util.Box(text: \"秘密\"))):\n                Result.Ok(x):\n                    let box = x.value\n                    length(box.text) + 40\n                Result.Err(error):\n                    -2", library + "pub record Box:\n    text: String\n"), "42\n"),
        "imported-function": (package("            match memory.try_in(borrow(r), [util.borrow(41)]):\n                Result.Ok(xs):\n                    length(xs) + util.borrow(40)\n                Result.Err(error):\n                    -2", "module util\npub fn borrow(value: i64) -> i64 effects():\n    value + 1\n"), "42\n"),
    }
    leak = "record borrow:\n    value: String\nfn leak<R: memory.RegionName>(r: shared<memory.Region<R>>) -> String effects(ManagedAlloc):\n    match memory.try_in(r, borrow(value: \"秘密\")):\n        Result.Ok(x):\n            return x.value\n        Result.Err(error):\n            return \"failed\"\n" + region("            42")
    escape = "fn main() -> String effects(ManagedAlloc):\n    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n            match memory.try_in(borrow(r), PAYLOAD):\n                Result.Ok(x):\n                    return RETURNED\n                Result.Err(error):\n                    return \"allocation failed\"\n        Result.Err(error):\n            return \"region failed\"\n"
    invalid = {
        "local-field-type": (nominal + region("            let x = borrow(value: true)\n            x.value"), [("CGT003", "true")]),
        "generic-field-type": (generic + region("            let x = borrow<i64>(value: true)\n            x.value"), [("CGT003", "true")]),
        "nominal-region-escape": (leak, [("CGT095", "return x.value\n")]),
        "real-region-escape": (escape.replace("PAYLOAD", '"秘密"').replace("RETURNED", "x"), [("CGT095", "return x\n")]),
        "real-borrow-scalar": (region("            let x = borrow(42)\n            42"), [("CGT095", "borrow(42)")]),
        "record-hides-capability": (nominal + region("            let x = borrow(value: r)\n            x.value"), [("CGT095", "borrow(value: r)")]),
        "record-forges-capability": ((nominal + region("            memory.collect(borrow(value: 42))\n            42")).replace("effects(ManagedAlloc)", "effects(ManagedAlloc, Collect) caps(MemoryCollect)"), [("CGT003", "borrow(value: 42)"), ("CGT003", "borrow(value: 42)")]),
        "function-forges-capability": (("fn borrow(value: i64) -> i64 effects():\n    value\n" + region("            memory.collect(borrow(42))\n            42")).replace("effects(ManagedAlloc)", "effects(ManagedAlloc, Collect) caps(MemoryCollect)"), [("CGT095", "borrow(42)")]),
    }
    imported_escape = "module app\nimport util\n" + escape.replace("PAYLOAD", 'util.borrow<String>(value: "秘密")').replace("RETURNED", "x.value")
    invalid_packages = {
        "imported-field-type": (package("            let x = util.borrow<i64>(value: true)\n            x.value"), [("CGT003", "true")]),
        "imported-region-escape": ({"util.cg": library, "main.cg": imported_escape}, [("CGT095", "return x.value\n")]),
        "imported-cross-region": (package("            let text = \"秘密\"\n            match memory.try_in(borrow(r), util.borrow<String>(value: text)):\n                Result.Ok(x):\n                    length(x.value)\n                Result.Err(error):\n                    -2"), [("CGT095", "util.borrow<String>(value: text)")]),
    }
    return valid, packages, invalid, invalid_packages


if __name__ == "__main__":
    engine.fixtures = fixtures
    sys.exit(engine.main())
