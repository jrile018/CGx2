use cgx2_sema::check_module;
use cgx2_syntax::parse_module;

fn check(source: &str) -> Result<(), Vec<cgx2_sema::TypeDiagnostic>> {
    check_module(&parse_module(source).unwrap()).map(|_| ())
}

fn rejects(source: &str, code: &str, text: &str) {
    let errors = check(source).unwrap_err();
    let error = errors
        .iter()
        .find(|error| error.code == code)
        .unwrap_or_else(|| panic!("expected {code} for {source:?}, got {errors:?}"));
    assert_eq!(&source[error.span.start..error.span.end], text);
}

#[test]
fn accepts_bounded_same_region_graphs() {
    for source in [
        include_str!("../../cgx2-cli/tests/fixtures/object_references.cg"),
        include_str!("../../cgx2-cli/tests/fixtures/object_cycle.cg"),
        include_str!("../../cgx2-cli/tests/fixtures/object_nested_no_gc.cg"),
    ] {
        check(source).unwrap();
    }
}

#[test]
fn rejects_unbounded_shapes_slots_and_cross_region_edges() {
    let prefix =
        "let r = memory.region(mode: manual, limit: 1024)\nlet a = memory.alloc_object(r, 8, 1)\n";
    rejects(
        &format!("{prefix}let found = memory.reference(a, 1)\n"),
        "CGM002",
        "1",
    );
    rejects(
        &format!("{prefix}let b = memory.alloc_object(r, 8, 257)\n"),
        "CGM002",
        "memory.alloc_object(r, 8, 257)",
    );
    rejects(
        &format!("{prefix}let b = memory.alloc_object(r, 1048577, 1)\n"),
        "CGM002",
        "memory.alloc_object(r, 1048577, 1)",
    );
    rejects(
        &format!("{prefix}let b = memory.alloc_object(r, 8, a)\n"),
        "CGM002",
        "a",
    );
    rejects(
        &format!("{prefix}let i = memory.owned_bytes(1)\nmemory.clear_reference(a, i)\n"),
        "CGM002",
        "i",
    );
    rejects(
        &format!("{prefix}let s = memory.alloc_bytes(r, 8)\nmemory.set_reference(s, 0, a)\n"),
        "CGM002",
        "s",
    );
    rejects(
        &format!("{prefix}let s = memory.alloc_bytes(r, 8)\nmemory.set_reference(a, 0, s)\n"),
        "CGM002",
        "s",
    );
    rejects(&format!("{prefix}let other = memory.region(mode: manual, limit: 1024)\nlet b = memory.alloc_object(other, 8, 0)\nmemory.set_reference(a, 0, b)\n"), "CGM002", "b");
}

#[test]
fn rejects_dropped_or_escaping_handles_and_hidden_collect() {
    let prefix =
        "let r = memory.region(mode: manual, limit: 1024)\nlet a = memory.alloc_object(r, 8, 1)\n";
    rejects(
        &format!("{prefix}memory.drop(a)\nlet found = memory.reference(a, 0)\n"),
        "CGO001",
        "memory.reference(a, 0)",
    );
    rejects(
        &format!("{prefix}memory.close(r)\n"),
        "CGO003",
        "memory.close(r)",
    );
    rejects(&format!("{prefix}let found = memory.reference(a, 0)\nmemory.drop(a)\nmemory.close(r)\nmemory.use_reference(found)\n"), "CGM002", "memory.use_reference(found)");
    rejects(&format!("{prefix}other(a)\n"), "CGM001", "other");
    rejects("fn f() -> Unit effects(Collect) caps(MemoryCollect):\n let r = memory.region(mode: manual, limit: 1024)\n memory.collect(r)\n memory.close(r)\nfn main() -> Unit effects(Collect) caps(MemoryCollect):\n no_gc(budget: 100):\n  f()\n", "CGO004", "f()");
}
