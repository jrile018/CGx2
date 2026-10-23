use cgx2_sema::check_module;
use cgx2_syntax::parse_module;

fn both(body: &str, valid: bool) {
    let wrapped = format!(
        "fn main() -> Unit effects(ManagedAlloc, Mutate, Collect) caps(MemoryCollect):\n{}",
        body.lines()
            .map(|s| format!("    {s}\n"))
            .collect::<String>()
    );
    for source in [body.to_owned(), wrapped] {
        let result = check_module(&parse_module(&source).unwrap());
        assert_eq!(result.is_ok(), valid, "{source}\n{result:?}");
    }
}

#[test]
fn ranges_scopes_conversions_and_borrows() {
    both("let r = memory.region(mode: manual, limit: 4096)\nlet x = memory.owned_bytes(1)\nfor i in range(-2, 3):\n    let b = memory.borrow(x)\n    memory.end_borrow(b)\n    for j in range(i, 3):\n        if j >= i:\n            let t = memory.owned_bytes(j)\n            let m = memory.move(t)\n            let p = memory.promote_owned(r, m)\n            let c = memory.copy_to_owned(p)\n            memory.drop(c)\n            memory.drop(p)\n        else:\n            memory.use(x)\nfor i in range(0, 0):\n    memory.use(x)\nmemory.drop(x)\nmemory.close(r)\n", true);
}

#[test]
fn restricted_endpoints_and_immutable_indices() {
    for endpoints in [
        "true, 2",
        "0, false",
        "0",
        "0, 2, 1",
        "start: 0, end: 2",
        "0 + 1, 2",
        "unknown(), 2",
        "0, 9223372036854775808",
        "0, x",
    ] {
        both(&format!("let x = memory.owned_bytes(1)\nfor i in range({endpoints}):\n    memory.use(x)\nmemory.drop(x)\n"), false);
    }
    for body in [
        "i = 2",
        "let i = memory.owned_bytes(1)",
        "return",
        "for i in range(0, 1):\n        memory.use(x)",
    ] {
        both(&format!("let x = memory.owned_bytes(1)\nfor i in range(0, 0):\n    {body}\nmemory.drop(x)\n"), false);
    }
    both(
        "let i = memory.owned_bytes(1)\nfor i in range(0, 0):\n    memory.use(i)\nmemory.drop(i)\n",
        false,
    );
    both("for i in range(0, 1):\n    let t = memory.owned_bytes(1)\n    memory.drop(t)\nlet x = memory.owned_bytes(i)\nmemory.drop(x)\n", false);
}

#[test]
fn zero_trip_backedge_and_all_paths_are_checked() {
    for body in [
        "memory.drop(x)",
        "let t = memory.move(x)\n    memory.drop(t)",
        "let t = memory.promote_owned(r, x)\n    memory.drop(t)",
        "memory.close(r)",
        "let b = memory.borrow(x)",
        "let t = memory.owned_bytes(1)",
        "if false:\n        memory.drop(x)\n    else:\n        memory.use(x)",
        "let rr = memory.region(mode: manual, limit: 1)",
        "let o = memory.alloc_object(r, 0, 0)",
    ] {
        both(&format!("let r = memory.region(mode: manual, limit: 1024)\nlet x = memory.owned_bytes(1)\nfor i in range(0, 0):\n    {body}\nmemory.drop(x)\nmemory.close(r)\n"), false);
    }
    both("let r = memory.region(mode: manual, limit: 1024)\nno_gc(budget: 1024):\n    for i in range(0, 0):\n        if false:\n            memory.collect(r)\n        else:\n            memory.collect(r)\nmemory.close(r)\n", false);
    both("let r = memory.region(mode: manual, limit: 1024)\nlet o = memory.alloc_object(r, 0, 1)\nfor i in range(0, 0):\n    memory.clear_reference(o, 0)\nmemory.drop(o)\nmemory.close(r)\n", false);
}

#[test]
fn local_call_walkers_keep_scope_recursion_collection_and_closure_rules() {
    let source = "fn leaf(n: i64) -> Unit effects(ManagedAlloc, Mutate):\n    let x = memory.owned_bytes(n)\n    memory.drop(x)\nfn main() -> Unit effects(ManagedAlloc, Mutate):\n    for i in range(0, 3):\n        for j in range(i, 3):\n            if j >= i:\n                leaf(j)\n            else:\n                leaf(i)\n";
    check_module(&parse_module(source).unwrap()).unwrap();
    for bad in [
        source.replace("leaf(j)", "leaf(k)"),
        source.replace("range(i, 3)", "range(n, 3)"),
    ] {
        assert!(parse_module(&bad).is_err() || check_module(&parse_module(&bad).unwrap()).is_err());
    }
    let hidden = "fn collect_now() -> Unit effects(ManagedAlloc, Mutate, Collect) caps(MemoryCollect):\n    let r = memory.region(mode: manual, limit: 1024)\n    for j in range(0, 0):\n        memory.collect(r)\n    memory.close(r)\nfn main() -> Unit effects(ManagedAlloc, Mutate, Collect) caps(MemoryCollect):\n    no_gc(budget: 1024):\n        for i in range(0, 0):\n            if false:\n                collect_now()\n            else:\n                collect_now()\n";
    assert!(check_module(&parse_module(hidden).unwrap())
        .unwrap_err()
        .iter()
        .any(|e| e.code == "CGO004"));
    let recursive = source.replace("leaf(j)", "main()");
    assert!(check_module(&parse_module(&recursive).unwrap())
        .unwrap_err()
        .iter()
        .any(|e| e.message.contains("recursive")));
    let closed = "fn main() -> Unit effects(ManagedAlloc, Mutate):\n    let r = memory.region(mode: manual, limit: 1024)\n    for i in range(0, 0):\n        memory.close(r)\n";
    let errors = check_module(&parse_module(closed).unwrap()).unwrap_err();
    assert!(errors.iter().any(|e| e.message.contains("must be closed")));
}

#[test]
fn outer_borrows_and_local_names_cannot_escape_or_be_consumed() {
    both("let x = memory.owned_bytes(1)\nlet b = memory.borrow(x)\nfor i in range(0, 0):\n    memory.end_borrow(b)\nmemory.end_borrow(b)\nmemory.drop(x)\n", false);
    both("let x = memory.owned_bytes(1)\nfor i in range(0, 1):\n    let b = memory.borrow(x)\n    memory.end_borrow(b)\nmemory.end_borrow(b)\nmemory.drop(x)\n", false);
    both("let x = memory.owned_bytes(1)\nfor i in range(0, 1):\n    let t = memory.owned_bytes(1)\n    memory.drop(t)\nmemory.use(t)\nmemory.drop(x)\n", false);
    let source = "fn leaf(n: i64) -> Unit effects(ManagedAlloc, Mutate):\n    for n in range(0, 0):\n        let t = memory.owned_bytes(1)\n        memory.drop(t)\nfn main() -> Unit effects(ManagedAlloc, Mutate):\n    leaf(1)\n";
    assert!(check_module(&parse_module(source).unwrap()).is_err());
}
