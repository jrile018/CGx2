use cgx2_sema::check_module;
use cgx2_syntax::{parse_module, ExprKind, StmtKind};

fn function(body: &str) -> String {
    format!(
        "fn main() -> Unit effects(ManagedAlloc, Mutate, Collect) caps(MemoryCollect):\n{}",
        body.lines()
            .map(|line| format!("    {line}\n"))
            .collect::<String>()
    )
}

fn both_profiles(body: &str, error: Option<&str>) {
    for source in [body.to_owned(), function(body)] {
        let result = check_module(&parse_module(&source).unwrap());
        match error {
            None => {
                result.unwrap_or_else(|errors| panic!("{source}\n{errors:?}"));
            }
            Some(code) => {
                let errors = result.unwrap_err();
                assert!(
                    errors.iter().any(|error| error.code == code),
                    "{source}\n{errors:?}"
                );
            }
        }
    }
}

#[test]
fn independent_arm_names_nested_lifetimes_and_outer_environment() {
    both_profiles("let r = memory.region(mode: manual, limit: 1024)\nlet x = memory.owned_bytes(1)\nif true:\n    let b = memory.borrow(x)\n    memory.end_borrow(b)\n    if false:\n        let t = memory.alloc_bytes(r, 1)\n        memory.drop(t)\n    else:\n        let t = memory.owned_bytes(2)\n        memory.drop(t)\nelse:\n    let t = memory.owned_bytes(3)\n    memory.drop(t)\nlet t = memory.owned_bytes(4)\nmemory.drop(t)\nmemory.drop(x)\nif 1 < 2:\n    memory.close(r)\nelse:\n    memory.close(r)\n", None);
}

#[test]
fn branch_locals_require_explicit_release_on_every_successful_path() {
    for body in [
        "let t = memory.owned_bytes(1)",
        "let t = memory.alloc_bytes(r, 1)",
        "let t = memory.borrow(x)",
        "let t = memory.borrow_mut(x)",
        "let t = memory.owned_bytes(1)\n    if true:\n        memory.drop(t)\n    else:\n        memory.use(t)",
    ] {
        both_profiles(&format!("let r = memory.region(mode: manual, limit: 256)\nlet x = memory.owned_bytes(1)\nif false:\n    {body}\nelse:\n    memory.use(x)\nmemory.drop(x)\nmemory.close(r)\n"), Some("CGM001"));
    }
}

#[test]
fn unsupported_inactive_declarations_shadowing_and_escapes_stay_rejected() {
    for body in [
        "let t = memory.region(mode: manual, limit: 8)",
        "let t = memory.alloc_object(r, 1, 1)",
        "let t = memory.reference(obj, 0)",
        "let t = 1",
        "let x = memory.owned_bytes(1)\n    memory.drop(x)",
        "let mut t = memory.owned_bytes(1)",
        "let t: i64 = memory.owned_bytes(1)",
        "return",
    ] {
        both_profiles(&format!("let r = memory.region(mode: manual, limit: 256)\nlet obj = memory.alloc_object(r, 1, 1)\nlet x = memory.owned_bytes(1)\nif false:\n    {body}\nelse:\n    memory.use(x)\nmemory.drop(x)\nmemory.drop(obj)\nmemory.close(r)\n"), Some("CGM001"));
    }
    both_profiles("if true:\n    let t = memory.owned_bytes(1)\n    memory.drop(t)\nelse:\n    let t = memory.owned_bytes(1)\n    memory.drop(t)\nmemory.use(t)\n", Some("CGM002"));
}

#[test]
fn possible_path_transitions_and_inactive_operations_are_checked() {
    for branch in [
        "memory.drop(x)",
        "let t = memory.move(x)\n    memory.drop(t)",
    ] {
        both_profiles(&format!("let x = memory.owned_bytes(1)\nif false:\n    {branch}\nelse:\n    memory.use(x)\nmemory.use(x)\nmemory.drop(x)\n"), Some("CGO001"));
    }
    both_profiles("let r = memory.region(mode: manual, limit: 256)\nif false:\n    memory.close(r)\nelse:\n    memory.collect(r)\n", Some("CGO005"));
    both_profiles("let x = memory.owned_bytes(1)\nlet b = memory.borrow(x)\nif false:\n    memory.end_borrow(b)\nelse:\n    memory.use(x)\nmemory.drop(x)\n", Some("CGO005"));
    both_profiles("let r = memory.region(mode: manual, limit: 256)\nno_gc(budget: 128):\n    if false:\n        memory.collect(r)\n    else:\n        let t = memory.owned_bytes(1)\n        memory.drop(t)\nmemory.close(r)\n", Some("CGO004"));
}

#[test]
fn only_exact_bounded_guards_are_admitted_in_both_paths() {
    for guard in [
        "true", "false", "1 == 2", "1 != 2", "1 < 2", "1 <= 2", "1 > 2", "1 >= 2",
    ] {
        both_profiles(&format!("if {guard}:\n    let t = memory.owned_bytes(1)\n    memory.drop(t)\nelse:\n    let t = memory.owned_bytes(1)\n    memory.drop(t)\n"), None);
    }
    for guard in [
        "1",
        "1 + 1 == 2",
        "true == false",
        "1 == 2 == 3",
        "memory.owned_bytes(1)",
        "unknown()",
        "1 < unknown()",
        "9223372036854775808 == 1",
    ] {
        both_profiles(&format!("if {guard}:\n    let t = memory.owned_bytes(1)\n    memory.drop(t)\nelse:\n    let t = memory.owned_bytes(1)\n    memory.drop(t)\n"), Some("CGM002"));
    }
    let body = "if 1 == 2:\n    let t = memory.owned_bytes(1)\n    memory.drop(t)\nelse:\n    let t = memory.owned_bytes(1)\n    memory.drop(t)\n";
    for source in [body.to_owned(), function(body)] {
        for mutation in 0..4 {
            let mut module = parse_module(&source).unwrap();
            let statements = if module.functions.is_empty() {
                &mut module.statements
            } else {
                &mut module.functions[0].body
            };
            let StmtKind::If { condition, .. } = &mut statements[0].kind else {
                panic!()
            };
            let ExprKind::Call { callee, arguments } = &mut condition.kind else {
                panic!()
            };
            match mutation {
                0 => {
                    arguments[0].name = Some("left".into());
                }
                1 => {
                    arguments.pop();
                }
                2 => {
                    arguments.push(arguments[0].clone());
                }
                _ => {
                    let ExprKind::Name(path) = &mut callee.kind else {
                        panic!()
                    };
                    path.segments.push("extra".into());
                }
            }
            assert!(check_module(&module)
                .unwrap_err()
                .iter()
                .any(|e| e.code == "CGM002"));
        }
    }
}

#[test]
fn conditional_call_graph_collection_and_region_closure_walkers_recurse() {
    let source = "fn collect_now() -> Unit effects(ManagedAlloc, Collect, Mutate) caps(MemoryCollect):\n    let r = memory.region(mode: manual, limit: 256)\n    if false:\n        memory.collect(r)\n    else:\n        memory.collect(r)\n    memory.close(r)\nfn bridge() -> Unit effects(ManagedAlloc, Collect, Mutate) caps(MemoryCollect):\n    if false:\n        collect_now()\n    else:\n        collect_now()\nfn main() -> Unit effects(ManagedAlloc, Collect, Mutate) caps(MemoryCollect):\n    no_gc(budget: 1024):\n        if false:\n            bridge()\n        else:\n            bridge()\n";
    assert!(check_module(&parse_module(source).unwrap())
        .unwrap_err()
        .iter()
        .any(|e| e.code == "CGO004"));
    let valid = source.replace("    no_gc(budget: 1024):\n        if false:\n            bridge()\n        else:\n            bridge()", "    if false:\n        bridge()\n    else:\n        bridge()");
    check_module(&parse_module(&valid).unwrap()).unwrap();
    let recursive = valid.replace("        collect_now()", "        bridge()");
    assert!(check_module(&parse_module(&recursive).unwrap())
        .unwrap_err()
        .iter()
        .any(|e| e.message.contains("recursive")));
}

#[test]
fn ancestor_temporary_can_be_explicitly_released_in_both_nested_arms() {
    both_profiles("if true:\n    let outer = memory.owned_bytes(1)\n    if false:\n        memory.drop(outer)\n    else:\n        memory.drop(outer)\nelse:\n    let outer = memory.owned_bytes(2)\n    memory.drop(outer)\n", None);
}
