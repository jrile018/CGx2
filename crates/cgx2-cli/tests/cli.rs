use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

struct SourceFile(PathBuf);

impl SourceFile {
    fn new(source: &str) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("cgx2-cli-test-{}-{id}.cg", std::process::id()));
        fs::write(&path, source).unwrap();
        Self(path)
    }
}

impl Drop for SourceFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn invoke(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn run_and_shortcut_print_tail_value() {
    let file = SourceFile::new(include_str!("fixtures/answer.cg"));
    let path = file.0.to_str().unwrap();
    for args in [vec!["run", path], vec![path]] {
        let output = invoke(&args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"7\n");
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn delimiter_forwards_spaced_unicode_arguments() {
    let file = SourceFile::new(include_str!("fixtures/arguments.cg"));
    let path = file.0.to_str().unwrap();
    for args in [
        vec!["run", path, "--", "hello 世界"],
        vec![path, "--", "hello 世界"],
    ] {
        let output = invoke(&args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8(output.stdout).unwrap(), "hello 世界\n");
    }
}

#[test]
fn fmt_prints_canonical_text_without_rewriting() {
    let original = "let x=1+2\nx\n";
    let file = SourceFile::new(original);
    let output = invoke(&["fmt", file.0.to_str().unwrap()]);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"let x = 1 + 2\nx\n");
    assert_eq!(fs::read_to_string(&file.0).unwrap(), original);
}

#[test]
fn file_and_runtime_errors_are_nonzero_without_success_output() {
    let missing = std::env::temp_dir().join(format!("cgx2-cli-missing-{}.cg", std::process::id()));
    let output = invoke(&["run", missing.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("CGC002") && stderr.contains(missing.to_str().unwrap()),
        "{stderr}"
    );

    let directory = std::env::temp_dir().join(format!(
        "cgx2-cli-unreadable-{}-{}.cg",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let output = invoke(&["run", directory.to_str().unwrap()]);
    fs::remove_dir(&directory).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8(output.stderr).unwrap().contains("CGC002"));

    let file = SourceFile::new("1 / 0\n");
    let output = invoke(&["run", file.0.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("CGR006") && stderr.contains(file.0.to_str().unwrap()),
        "{stderr}"
    );
}

#[test]
fn invalid_commands_and_missing_delimiter_fail() {
    for args in [
        vec!["check"],
        vec!["run"],
        vec!["run", "file.cg", "not-delimited"],
    ] {
        let output = invoke(&args);
        assert!(!output.status.success(), "{args:?}");
        assert!(output.stdout.is_empty());
    }
}
