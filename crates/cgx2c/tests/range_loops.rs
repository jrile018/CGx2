use cgx2_cli::{Value, run_checked_source};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn compare(source: &str) {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("cgx2-range-{}-{id}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let file = dir.join("case.cg");
    fs::write(&file, source).unwrap();
    let interpreted = run_checked_source(source, &[]);
    for release in [false, true] {
        let binary: PathBuf = dir.join(if release { "release" } else { "debug" });
        let mut build = Command::new(env!("CARGO_BIN_EXE_cgx2c"));
        build.arg("build").arg(&file).arg("--out").arg(&binary);
        if release {
            build.arg("--release");
        }
        let built = build.output().unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let native = Command::new(&binary).output().unwrap();
        match &interpreted {
            Ok((result, _)) => {
                assert!(
                    native.status.success(),
                    "{}",
                    String::from_utf8_lossy(&native.stderr)
                );
                let stdout = if result.value == Value::Unit {
                    String::new()
                } else {
                    format!("{}\n", result.value)
                };
                assert_eq!(native.stdout, stdout.as_bytes());
                assert!(native.stderr.is_empty());
            }
            Err(errors) => {
                assert!(!native.status.success());
                assert!(native.stdout.is_empty());
                let first = &errors[0];
                let span = first.span.expect("source span");
                let diagnostic = String::from_utf8_lossy(&native.stderr);
                assert!(
                    diagnostic.contains(&format!("{} at {}..{}", first.code, span.start, span.end)),
                    "{diagnostic}"
                );
                assert!(diagnostic.contains(&first.message), "{diagnostic}");
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn scalar_ranges_match_interpreter_in_debug_and_release() {
    for source in [
        "let mut sum = 7\nfor i in range(3, 3):\n    sum += i\nsum\n",
        "let mut sum = 7\nfor i in range(4, 2):\n    sum += i\nsum\n",
        "let mut sum = 0\nfor i in range(2, 3):\n    sum += i\nsum\n",
        "let mut sum = 0\nfor i in range(-2, 2):\n    sum += i\nsum\n",
        "let i = 99\nlet mut sum = 0\nfor i in range(0, 2):\n    let local = i\n    sum += local\nsum + i\n",
        "let mut sum = 0\nfor i in range(0, 2):\n    for j in range(0, 3):\n        sum += i + j\nsum\n",
        "for i in range(-9223372036854775808, -9223372036854775807):\n    i\n42\n",
        "let mut sum = 0\nfor i in range(9223372036854775806, 9223372036854775807):\n    sum += i\nsum\n",
        "fn twice(x: i64) -> i64:\n    x + x\nfn main() -> i64:\n    let mut sum = 0\n    for i in range(-2, 2):\n        sum += twice(i)\n    sum\n",
    ] {
        compare(source);
    }
}

#[test]
fn range_failures_preserve_first_code_and_span() {
    compare("let mut sum = 9223372036854775807\nfor i in range(0, 2):\n    sum += 1\nsum\n");
    compare("for i in range(0, 1000000):\n    1\n0\n");
}

#[test]
fn build_rejects_malformed_ranges_before_writing_binary() {
    for (source, code) in [
        ("for i in range(1):\n    i\n", "CGT005"),
        ("for i in range(true, 2):\n    i\n", "CGT003"),
        ("range(0, 2)\n", "CGT003"),
    ] {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("cgx2-range-invalid-{}-{id}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let file = dir.join("case.cg");
        let binary = dir.join("program");
        fs::write(&file, source).unwrap();
        let built = Command::new(env!("CARGO_BIN_EXE_cgx2c"))
            .arg("build")
            .arg(&file)
            .arg("--out")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(!built.status.success());
        assert!(String::from_utf8_lossy(&built.stderr).contains(code));
        assert!(!binary.exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
