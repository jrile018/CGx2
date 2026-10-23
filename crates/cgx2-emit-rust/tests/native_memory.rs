use cgx2_cli::run_checked_source;
use cgx2_emit_rust::emit_rust;
use cgx2_sema::check_module;
use cgx2_syntax::parse_module;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn native(source: &str, optimized: bool) -> std::process::Output {
    let module = parse_module(source).unwrap();
    let checked = check_module(&module).unwrap();
    let generated = emit_rust(&module, &checked).unwrap();
    assert!(generated.contains("cgx2_runtime::"));
    assert!(!generated.contains("cgx2_cli"));
    let dir = std::env::temp_dir().join(format!(
        "cgx2-native-memory-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&dir).unwrap();
    let runtime_source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../cgx2-runtime/src/lib.rs");
    let runtime = dir.join("libcgx2_runtime.rlib");
    let result = Command::new("rustc")
        .args([
            "--edition=2021",
            "--crate-name",
            "cgx2_runtime",
            "--crate-type=lib",
        ])
        .arg(runtime_source)
        .arg("-o")
        .arg(&runtime)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let emitted = dir.join("generated.rs");
    let binary = dir.join("program");
    fs::write(&emitted, generated).unwrap();
    let mut command = Command::new("rustc");
    command
        .arg("--edition=2024")
        .arg(&emitted)
        .arg("--extern")
        .arg(format!("cgx2_runtime={}", runtime.display()))
        .arg("-o")
        .arg(&binary);
    if optimized {
        command.arg("-O");
    }
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result = Command::new(&binary).output().unwrap();
    fs::remove_dir_all(dir).unwrap();
    result
}

fn compare(source: &str, expect_error: Option<&str>) {
    let cli = run_checked_source(source, &[]);
    assert_eq!(cli.is_err(), expect_error.is_some());
    for optimized in [false, true] {
        let output = native(source, optimized);
        assert_eq!(
            output.status.success(),
            expect_error.is_none(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&output.stderr);
        if let Some(message) = expect_error {
            assert!(stderr.contains(message), "{stderr}");
            assert!(cli.as_ref().unwrap_err()[0].message.contains(message));
            assert!(!stderr.contains("panicked"), "{stderr}");
        } else {
            assert!(stderr.is_empty(), "{stderr}");
        }
    }
}

#[test]
fn existing_fixture_and_conversion_match_cli() {
    compare(
        include_str!("../../cgx2-cli/tests/fixtures/memory.cg"),
        None,
    );
    compare(
        "let r = memory.region(mode: manual, limit: 256)\nlet owned = memory.owned_bytes(8)\nlet moved = memory.move(owned)\nlet managed = memory.promote_owned(r, moved)\nlet borrow = memory.borrow(managed)\nmemory.end_borrow(borrow)\nlet copy = memory.copy_to_owned(managed)\nmemory.drop(managed)\nmemory.collect(r)\nmemory.close(r)\nmemory.use(copy)\nmemory.drop(copy)\n",
        None,
    );
}

#[test]
fn auto_and_manual_collection_and_budget_failure_match_cli() {
    let base = |mode| {
        format!(
            "let r = memory.region(mode: {mode}, limit: 1048640)\nlet x = memory.alloc_bytes(r, 1048576)\nmemory.drop(x)\nlet y = memory.alloc_bytes(r, 1048576)\nmemory.use(y)\n"
        )
    };
    compare(&base("auto"), None);
    compare(&base("manual"), Some("memory allocation failed"));
    compare(
        "let r = memory.region(mode: manual, limit: 256)\nno_gc(budget: 95):\n    let x = memory.alloc_bytes(r, 32)\n",
        Some("memory allocation failed"),
    );
    compare(
        "let r = memory.region(mode: manual, limit: 256)\nno_gc(budget: 191):\n    let x = memory.alloc_bytes(r, 32)\n    no_gc(budget: 95):\n        let y = memory.alloc_bytes(r, 32)\n",
        Some("memory allocation failed"),
    );
}

#[test]
fn invalid_borrow_and_nonliteral_size_are_rejected_before_native_output() {
    for source in [
        "let r = memory.region(mode: manual, limit: 256)\nlet x = memory.alloc_bytes(r, 8)\nlet b = memory.borrow(x)\nmemory.drop(x)\n",
        "let r = memory.region(mode: manual, limit: 256)\nlet n = memory.owned_bytes(8)\nlet x = memory.alloc_bytes(r, n)\n",
    ] {
        let module = parse_module(source).unwrap();
        assert!(check_module(&module).is_err());
        assert!(run_checked_source(source, &[]).is_err());
    }
}

#[test]
fn conditional_scoped_names_and_nested_parameter_calls_match_cli() {
    for source in [
        include_str!("../../cgx2c/tests/fixtures/conditional_memory_success.cg"),
        include_str!("../../cgx2c/tests/fixtures/conditional_memory_functions.cg"),
    ] {
        compare(source, None);
    }
}

#[test]
fn all_parameter_comparisons_select_only_the_budgeted_allocation() {
    let mut source = String::new();
    let mut calls = String::new();
    let mut index = 0;
    for (op, a, b, truth) in [
        ("==", 1, 1, true),
        ("!=", 1, 1, false),
        ("<", 1, 2, true),
        ("<=", 2, 1, false),
        (">", 2, 1, true),
        (">=", 1, 2, false),
    ] {
        for (a, b, truth) in [
            (a, b, truth),
            (
                b,
                a,
                if matches!(op, "==" | "!=") {
                    truth
                } else {
                    !truth
                },
            ),
        ] {
            source.push_str(&format!("fn choose_{index}(a: i64, b: i64) -> Unit effects(ManagedAlloc, Mutate):\n    no_gc(budget: 40):\n        if a {op} b:\n            let temp = memory.owned_bytes({})\n            memory.drop(temp)\n        else:\n            let temp = memory.owned_bytes({})\n            memory.drop(temp)\n", if truth { 8 } else { 9 }, if truth { 9 } else { 8 }));
            calls.push_str(&format!("    choose_{index}({a}, {b})\n"));
            index += 1;
        }
    }
    source.push_str("fn main() -> Unit effects(ManagedAlloc, Mutate):\n");
    source.push_str(&calls);
    compare(&source, None);
}
