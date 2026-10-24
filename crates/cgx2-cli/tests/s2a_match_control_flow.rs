use cgx2_cli::{Value, run_checked_source};
use std::{fs, process::Command};

#[test]
fn match_flow_interpretation_preserves_values_and_rejects_bad_inference() {
    for source in [
        include_str!("../../cgx2-sema/tests/match_flow/nontail.cg"),
        include_str!("../../cgx2-sema/tests/match_flow/nontail_value.cg"),
        include_str!("../../cgx2-sema/tests/match_flow/all_return.cg"),
        include_str!("../../cgx2-sema/tests/match_flow/loop.cg"),
        include_str!("../../cgx2-sema/tests/match_flow/partial.cg"),
    ] {
        assert_eq!(
            run_checked_source(source, &[]).unwrap().0.value,
            Value::I64(42)
        );
    }
    let errors = run_checked_source(
        include_str!("../../cgx2-sema/tests/match_flow/inferred.cg"),
        &[],
    )
    .unwrap_err();
    assert!(errors.iter().any(|d| d.code == "CGT003"));
    assert!(!errors.iter().any(|d| d.code.starts_with("CGR")));
}

#[test]
fn product_check_run_and_emit_reject_before_publishing() {
    let source = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../cgx2-sema/tests/match_flow/inferred.cg"
    );
    let dir = std::env::temp_dir().join(format!("cgx2-match-cli-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let output = dir.join("bad.rs");
    for args in [
        vec!["check", source],
        vec!["run", source],
        vec![
            "emit",
            source,
            "--target",
            "rust",
            "--out",
            output.to_str().unwrap(),
        ],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_cgx2"))
            .args(args)
            .output()
            .unwrap();
        assert!(!result.status.success());
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains("CGT003"), "{stderr}");
        assert!(!output.exists());
    }
    fs::remove_dir_all(dir).unwrap();
}
