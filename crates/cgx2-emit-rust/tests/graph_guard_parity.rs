use cgx2_cli::run_checked_source_with_failure;
use cgx2_emit_rust::emit_rust;
use cgx2_sema::check_module;
use cgx2_syntax::parse_module;
use std::{fs, path::PathBuf, process::Command};

fn emit(source: &str) -> String {
    let module = parse_module(source).unwrap();
    let checked = check_module(&module).unwrap();
    emit_rust(&module, &checked).unwrap()
}

#[test]
fn whole_module_profile_reaches_scalar_callees_branches_and_loops() {
    for source in [
        include_str!("graph_guard_parity/nested-zero.cg"),
        include_str!("graph_guard_parity/outer-zero.cg"),
        include_str!("graph_guard_parity/callee-zero.cg"),
        include_str!("graph_guard_parity/caller-zero.cg"),
        include_str!("graph_guard_parity/branch-loop-zero.cg"),
    ] {
        let generated = emit(source);
        assert!(generated.contains("const CGX2_GRAPH_PROFILE: bool = true;"));
        assert_eq!(
            generated.matches("session.try_no_gc(").count(),
            source.matches("no_gc(").count()
        );
        assert!(!generated.contains("session.no_gc("));
    }
    for source in [
        include_str!("graph_guard_parity/legacy-exact.cg"),
        include_str!("graph_guard_parity/legacy-one-under.cg"),
        include_str!("graph_guard_parity/legacy-nested-zero.cg"),
    ] {
        let generated = emit(source);
        assert!(generated.contains("const CGX2_GRAPH_PROFILE: bool = false;"));
        assert_eq!(
            generated.matches("session.no_gc(").count(),
            source.matches("no_gc(").count()
        );
        assert!(!generated.contains("session.try_no_gc("));
    }
}

#[test]
fn reviewed_zero_budget_reproducer_retains_source_error_and_cleanup() {
    let source = include_str!("graph_guard_parity/nested-zero.cg");
    let failure = run_checked_source_with_failure(source, &[])
        .err()
        .unwrap()
        .remove(0);
    assert_eq!(failure.error.code, "CGR013");
    assert!(failure.error.message.contains("BudgetExceeded"));
    let span = failure.error.span.unwrap();
    assert_eq!((span.start, span.end), (179, 251));
    assert_eq!(failure.rollback_preserved, Some(true));
    let operation = failure.operation_failure.unwrap();
    assert_eq!(operation.runtime_root_registrations, Some(1));
    assert_eq!(operation.rooted_handles, Some(1));
    assert_eq!(
        failure.final_unwind.unwrap().runtime_root_registrations,
        Some(0)
    );
    assert!(failure.cleanup_errors.is_empty());
    assert_eq!(failure.report.unwrap().totals.collection_count, 0);
}

fn checked_output(command: &mut Command) -> std::process::Output {
    let output = command.output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn synthetic_guard_reserve_faults_use_real_generated_code_and_runtime_hooks() {
    // Synthetic reserve failures, not real resource exhaustion or leak assurance.
    let directory =
        std::env::temp_dir().join(format!("cgx2-native-guard-faults-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let runtime = directory.join("libcgx2_runtime.rlib");
    checked_output(
        Command::new("rustc")
            .args([
                "--edition=2021",
                "--crate-name=cgx2_runtime",
                "--crate-type=rlib",
                "--cfg",
                "feature=\"fault-injection\"",
            ])
            .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../cgx2-runtime/src/lib.rs"))
            .arg("-o")
            .arg(&runtime),
    );
    let source_text = include_str!("graph_guard_parity/deep-exact.cg");
    let module = parse_module(source_text).unwrap();
    let mut spans = Vec::new();
    cgx2_syntax::visit_stmts(
        &module.functions[0].body,
        &mut |statement| {
            if matches!(statement.kind, cgx2_syntax::StmtKind::NoGc { .. }) {
                spans.push((statement.span.start, statement.span.end));
            }
        },
        &mut |_| {},
    );
    let mut generated = emit(source_text);
    generated.push_str(&format!(
        "\nconst EXPECTED_GUARD_SPANS: &[(usize, usize)] = &{spans:?};\n"
    ));
    generated.push_str(include_str!("graph_guard_parity/fault_harness.rs.txt"));
    let source = directory.join("generated.rs");
    fs::write(&source, generated).unwrap();
    for optimized in [false, true] {
        let binary = directory.join(if optimized { "release" } else { "debug" });
        let mut compiler = Command::new("rustc");
        compiler
            .args([
                "--edition=2024",
                "--test",
                "--cfg",
                "feature=\"fault-injection\"",
            ])
            .arg(&source)
            .arg("--extern")
            .arg(format!("cgx2_runtime={}", runtime.display()))
            .arg("-o")
            .arg(&binary)
            .env("CGX2_TEST_FAIL_RESERVE", "1");
        if optimized {
            compiler.arg("-O");
        }
        checked_output(&mut compiler);
        let output = checked_output(Command::new(&binary).arg("--nocapture"));
        assert!(String::from_utf8_lossy(&output.stdout).contains("3 GuardBudgets failures"));
        println!(
            "synthetic generated runtime (optimized={optimized}):\n{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
    fs::remove_dir_all(directory).unwrap();
}
