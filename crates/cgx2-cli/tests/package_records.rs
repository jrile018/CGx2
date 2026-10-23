use cgx2_cli::run_native_package;
use std::path::Path;

#[test]
fn checked_package_interpreter_keeps_identity_and_defining_error_file() {
    let root = Path::new("../cgx2-package/fixtures/public-records");
    let value = run_native_package(root, &[]).unwrap();
    assert_eq!(
        value.value.to_string(),
        "demo.geometry.Point(self: 42, live: true)"
    );
    for (count, code) in [(2, "CGR006"), (3, "CGR005")] {
        let errors = run_native_package(root, &vec!["x".to_owned(); count]).unwrap_err();
        assert_eq!(errors[0].code, code);
        assert_eq!(errors[0].file, Path::new("src/main.cg"));
    }
    assert_eq!(
        run_native_package(
            Path::new("../cgx2-package/fixtures/public-record-collisions"),
            &[]
        )
        .unwrap()
        .value
        .to_string(),
        "42"
    );
    let errors = run_native_package(root, &["fail".to_owned()]).unwrap_err();
    assert_eq!(errors[0].code, "CGR005");
    assert_eq!(errors[0].file, Path::new("src/geometry.cg"));
    let text = std::fs::read_to_string(root.join(&errors[0].file)).unwrap();
    let span = errors[0].span.unwrap();
    assert_eq!(&text[span.start..span.end], "9223372036854775807 + 1");
}

#[test]
fn cli_package_route_executes_pure_records() {
    let root = Path::new("../cgx2-package/fixtures/public-records");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .arg("run")
        .arg(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout,
        b"demo.geometry.Point(self: 42, live: true)\n"
    );
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .args(["--diagnostic-format=json", "run"])
        .arg(root)
        .args(["--", "fail"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(
        errors.contains("CGR005") && errors.contains("geometry.cg"),
        "{errors}"
    );
}
