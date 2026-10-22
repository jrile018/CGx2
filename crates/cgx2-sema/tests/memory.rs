use cgx2_sema::check_module;
use cgx2_syntax::parse_module;

fn check(source: &str) -> Result<(), Vec<cgx2_sema::TypeDiagnostic>> {
    check_module(&parse_module(source).unwrap()).map(|_| ())
}

fn error(source: &str, code: &str, needle: &str) {
    let errors = check(source).unwrap_err();
    let found = errors
        .iter()
        .find(|d| d.code == code)
        .unwrap_or_else(|| panic!("{errors:?}"));
    assert_eq!(&source[found.span.start..found.span.end], needle);
}

#[test]
fn checks_bounded_memory_script() {
    check("let scratch = memory.region(mode: manual, limit: 4096)\nlet buffer = memory.owned_bytes(128)\nlet moved = memory.move(buffer)\nno_gc(budget: 256):\n    let item = memory.alloc_bytes(scratch, 64)\nmemory.collect(scratch)\nmemory.drop(item)\nmemory.close(scratch)\n").unwrap();
}

#[test]
fn ownership_failures_retain_source_spans() {
    error(
        "let b = memory.owned_bytes(1)\nlet c = memory.move(b)\nb\n",
        "CGO001",
        "b",
    );
    error("let r = memory.region(mode: auto, limit: 64)\nlet x = memory.alloc_bytes(r, 1)\nmemory.close(r)\n", "CGO003", "memory.close(r)");
    error(
        "let b = memory.owned_bytes(1)\nlet a = memory.borrow(b)\nlet z = memory.borrow_mut(b)\n",
        "CGO002",
        "memory.borrow_mut(b)",
    );
    error("let r = memory.region(mode: manual, limit: 64)\nno_gc(budget: 4):\n    no_gc(budget: 2):\n        memory.collect(r)\n", "CGO004", "memory.collect(r)");
    error(
        "let café = memory.owned_bytes(1)\nlet moved = memory.move(café)\ncafé\n",
        "CGO001",
        "café",
    );
}

#[test]
fn rejects_unmodeled_values_and_calls() {
    error(
        "let r = memory.region(mode: manual, limit: 4)\nlet x = r\n",
        "CGM001",
        "r",
    );
    error(
        "let b = memory.owned_bytes(4)\nother(b)\n",
        "CGM001",
        "other",
    );
    error(
        "let b = memory.owned_bytes(4)\nfor x in b:\n    x\n",
        "CGM001",
        "for x in b:\n    x\n",
    );
    error("let x = memory.unknown(4)\n", "CGM001", "memory.unknown(4)");
    error(
        "fn f():\n    memory.owned_bytes(4)\n",
        "CGM001",
        "memory.owned_bytes(4)\n",
    );
    error("let x = memory.move(1)\n", "CGM002", "1");
}

#[test]
fn validates_literal_budgets_and_named_intrinsics() {
    error(
        "no_gc(budget: value):\n    memory.owned_bytes(1)\n",
        "CGM002",
        "value",
    );
    error(
        "no_gc(budget: 18446744073709551616):\n    memory.owned_bytes(1)\n",
        "CGM002",
        "18446744073709551616",
    );
    error(
        "let r = memory.region(mode: variable, limit: 4)\n",
        "CGM002",
        "variable",
    );
    error(
        "let r = memory.region(mode: manual, limit: x)\n",
        "CGM002",
        "x",
    );
    error(
        "let r = memory.region(mode: manual, limit: 18446744073709551616)\n",
        "CGM002",
        "18446744073709551616",
    );
    error(
        "let r = memory.region(mode: manual, wrong: 4)\n",
        "CGM002",
        "mode: manual",
    );
    error(
        "let r = memory.region(mode: manual, limit: 4)\nlet x = memory.alloc_bytes(4, 1)\n",
        "CGM002",
        "4",
    );
}
