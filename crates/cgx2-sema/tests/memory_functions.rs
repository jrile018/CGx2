use cgx2_sema::check_module;
use cgx2_syntax::parse_module;

fn check(source: &str) -> Result<(), Vec<cgx2_sema::TypeDiagnostic>> {
    check_module(&parse_module(source).unwrap()).map(|_| ())
}

fn has_error(source: &str, code: &str, text: &str) {
    let errors = check(source).unwrap_err();
    assert!(
        errors.iter().any(|error| {
            error.code == code
                && source[error.span.start..error.span.end].trim_end() == text.trim_end()
        }),
        "{errors:?}"
    );
}

const VALID: &str = "fn build() -> Unit effects(ManagedAlloc, Collect, Mutate) caps(MemoryCollect):\n    let r = memory.region(mode: manual, limit: 256)\n    let o = memory.owned_bytes(8)\n    let m = memory.promote_owned(r, o)\n    let copy = memory.copy_to_owned(m)\n    memory.drop(m)\n    memory.drop(copy)\n    memory.collect(r)\n    memory.close(r)\nfn main() -> Unit effects(ManagedAlloc, Collect, Mutate) caps(MemoryCollect):\n    build()\n";

#[test]
fn accepts_local_memory_functions_and_conversions() {
    check(VALID).unwrap();
}

#[test]
fn scalar_parameters_are_checked_at_calls_and_only_admitted_as_byte_sizes() {
    check("fn make(n: i64) -> Unit effects(ManagedAlloc):\n let value = memory.owned_bytes(n)\nfn relay(n: i64) -> Unit effects(ManagedAlloc):\n make(n)\nfn main() -> Unit effects(ManagedAlloc):\n relay(0 - 1)\n").unwrap();
    has_error("fn make(n: i64) -> Unit effects(ManagedAlloc):\n let value = memory.owned_bytes(n)\nfn main() -> Unit effects(ManagedAlloc):\n make()\n", "CGM002", "make()");
    has_error("fn make(n: i64) -> Unit effects(ManagedAlloc):\n let value = memory.owned_bytes(n)\nfn main() -> Unit effects(ManagedAlloc):\n make(true)\n", "CGM002", "true");
    has_error("fn make(n: i64) -> Unit effects(ManagedAlloc):\n let value = memory.owned_bytes(n)\nfn main() -> Unit effects(ManagedAlloc):\n make(9223372036854775808)\n", "CGM002", "9223372036854775808");
    has_error("fn make(n: i64) -> Unit effects(ManagedAlloc):\n let region = memory.region(mode: manual, limit: n)\n memory.close(region)\nfn main() -> Unit effects(ManagedAlloc):\n make(8)\n", "CGM002", "n");
    has_error("fn make(n: Bool) -> Unit effects(ManagedAlloc):\n let value = memory.owned_bytes(n)\nfn main() -> Unit effects(ManagedAlloc):\n make(8)\n", "CGM002", "n: Bool");
}

#[test]
fn rejects_cross_boundary_handles_and_dynamic_sizes() {
    has_error("fn build() -> Unit effects(ManagedAlloc):\n    let o = memory.owned_bytes(8)\n    return o\nfn main() -> Unit effects(ManagedAlloc):\n    build()\n", "CGM001", "return o");
    check("fn build(n: i64) -> Unit effects(ManagedAlloc):\n    let o = memory.owned_bytes(n)\nfn main() -> Unit effects(ManagedAlloc):\n    build(8)\n").unwrap();
    has_error("fn build() -> Unit effects(ManagedAlloc):\n    let o = memory.owned_bytes(n)\nfn main() -> Unit effects(ManagedAlloc):\n    build()\n", "CGM002", "n");
    has_error("fn build() -> Unit effects(Mutate):\n    memory.use(outer)\nfn main() -> Unit effects(ManagedAlloc, Mutate):\n    let outer = memory.owned_bytes(8)\n    build()\n", "CGM002", "outer");
    has_error("fn build() -> Unit effects(ManagedAlloc):\n    let o = memory.owned_bytes(8)\nfn main() -> Unit effects(ManagedAlloc):\n    build(o)\n", "CGM002", "build(o)");
    has_error("fn build() -> Unit effects(ManagedAlloc, Mutate):\n    let o = memory.owned_bytes(8)\n    let b = memory.borrow(o)\n    return b\nfn main() -> Unit effects(ManagedAlloc, Mutate):\n    build()\n", "CGM001", "return b");
    has_error("fn build() -> Unit effects(ManagedAlloc):\n    let o = memory.owned_bytes(8)\n    o\nfn main() -> Unit effects(ManagedAlloc):\n    build()\n", "CGM001", "o");
}

#[test]
fn rejects_recursion_and_collect_through_no_gc_call() {
    has_error("fn build() -> Unit effects(ManagedAlloc):\n    let o = memory.owned_bytes(8)\n    build()\nfn main() -> Unit effects(ManagedAlloc):\n    build()\n", "CGM001", "build()");
    has_error("fn build() -> Unit effects(ManagedAlloc, Collect) caps(MemoryCollect):\n    let r = memory.region(mode: manual, limit: 64)\n    memory.collect(r)\n    memory.close(r)\nfn main() -> Unit effects(ManagedAlloc, Collect) caps(MemoryCollect):\n    no_gc(budget: 64):\n        build()\n", "CGO004", "build()");
    has_error("fn collect_now() -> Unit effects(ManagedAlloc, Collect, Mutate) caps(MemoryCollect):\n    let r = memory.region(mode: manual, limit: 64)\n    memory.collect(r)\n    memory.close(r)\nfn bridge() -> Unit effects(ManagedAlloc, Collect, Mutate) caps(MemoryCollect):\n    collect_now()\nfn main() -> Unit effects(ManagedAlloc, Collect, Mutate) caps(MemoryCollect):\n    no_gc(budget: 64):\n        bridge()\n", "CGO004", "bridge()");
}

#[test]
fn requires_declared_memory_effects_and_capability() {
    has_error("fn build() -> Unit effects():\n    let o = memory.owned_bytes(8)\nfn main() -> Unit effects(ManagedAlloc):\n    build()\n", "CGT012", "memory.owned_bytes(8)");
    has_error("fn build() -> Unit effects(ManagedAlloc, Collect):\n    let r = memory.region(mode: manual, limit: 64)\n    memory.collect(r)\n    memory.close(r)\nfn main() -> Unit effects(ManagedAlloc, Collect) caps(MemoryCollect):\n    build()\n", "CGT013", "fn build() -> Unit effects(ManagedAlloc, Collect):\n    let r = memory.region(mode: manual, limit: 64)\n    memory.collect(r)\n    memory.close(r)\n");
    has_error("fn build() -> Unit effects(ManagedAlloc):\n    let o = memory.owned_bytes(8)\nfn main() -> Unit effects():\n    build()\n", "CGT012", "build()");
}
