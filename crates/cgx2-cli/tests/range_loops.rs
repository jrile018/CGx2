use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn invoke(command: &str, source: &str) -> std::process::Output {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let file = std::env::temp_dir().join(format!("cgx2-range-cli-{}-{id}.cg", std::process::id()));
    fs::write(&file, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cgx2"))
        .arg(command)
        .arg(&file)
        .output()
        .unwrap();
    fs::remove_file(file).unwrap();
    output
}

#[test]
fn check_and_run_bounded_range_loops() {
    for (source, expected) in [
        (
            "let mut sum = 7\nfor i in range(3, 3):\n    sum += i\nsum\n",
            "7\n",
        ),
        (
            "let mut sum = 0\nfor i in range(2, 3):\n    sum += i\nsum\n",
            "2\n",
        ),
        (
            "let mut sum = 0\nfor i in range(-2, 2):\n    sum += i\nsum\n",
            "-2\n",
        ),
    ] {
        let checked = invoke("check", source);
        assert!(
            checked.status.success(),
            "{}",
            String::from_utf8_lossy(&checked.stderr)
        );
        let run = invoke("run", source);
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(run.stdout, expected.as_bytes());
    }
    for (source, code) in [
        ("for i in range(1):\n    i\n", "CGT005"),
        ("for i in range(true, 2):\n    i\n", "CGT003"),
        ("range(0, 2)\n", "CGT003"),
    ] {
        for command in ["check", "run"] {
            let result = invoke(command, source);
            assert!(!result.status.success());
            assert!(String::from_utf8_lossy(&result.stderr).contains(code));
        }
    }
    let budget = invoke("run", "for i in range(0, 1000000):\n    1\n0\n");
    assert!(!budget.status.success());
    assert!(String::from_utf8_lossy(&budget.stderr).contains("CGR010"));
}
