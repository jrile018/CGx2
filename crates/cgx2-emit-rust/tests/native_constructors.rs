use cgx2_cli::run_source;
use cgx2_emit_rust::{emit_package_rust, emit_rust};
use cgx2_package::load_checked_package;
use cgx2_sema::check_module;
use cgx2_syntax::parse_module;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

const NAMES: &[&str] = &[
    "length",
    "show",
    "parse_i64",
    "array",
    "list",
    "get",
    "append",
    "concat",
    "updated",
    "slice",
];
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn directory() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cgx2-native-constructors-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn execute(generated: &str, expected: Result<&str, &str>) {
    let dir = directory();
    fs::write(dir.join("main.rs"), generated).unwrap();
    for optimized in [false, true] {
        let mut compiler = Command::new("rustc");
        compiler
            .current_dir(&dir)
            .args(["--edition=2021", "main.rs", "-o", "program"]);
        if optimized {
            compiler.arg("-O");
        }
        let compiled = compiler.output().unwrap();
        assert!(
            compiled.status.success(),
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        let output = Command::new(dir.join("program"))
            .current_dir(&dir)
            .output()
            .unwrap();
        match expected {
            Ok(value) => {
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(output.stdout, value.as_bytes());
                assert!(output.stderr.is_empty());
            }
            Err(code) => {
                assert!(!output.status.success());
                assert!(output.stdout.is_empty());
                let error = String::from_utf8_lossy(&output.stderr);
                assert!(error.contains(code), "{error}");
                assert!(!error.contains("panicked at"), "{error}");
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

fn source(source: &str, expected: &str) {
    let interpreted = run_source(source, &[]).unwrap();
    assert_eq!(format!("{}\n", interpreted.value), expected);
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    let generated = emit_rust(&module, &typed).unwrap();
    assert_eq!(generated, emit_rust(&module, &typed).unwrap());
    execute(&generated, Ok(expected));
}

#[test]
fn every_intrinsic_spelling_can_construct_a_record() {
    let mut text = String::new();
    for name in NAMES {
        text.push_str(&format!(
            "record {name}:\n    value: i64\n    marker: Bool\n"
        ));
    }
    text.push_str("fn main() -> i64:\n");
    for (i, name) in NAMES.iter().enumerate() {
        text.push_str(&format!("    let v{i} = {name}(marker: true, value: 42)\n"));
    }
    text.push_str(&format!(
        "    {}\n",
        (0..NAMES.len())
            .map(|i| format!("v{i}.value"))
            .collect::<Vec<_>>()
            .join(" + ")
    ));
    source(&text, "420\n");
}

#[test]
fn generic_constructors_in_generic_bodies_keep_concrete_types() {
    let mut text = String::new();
    for name in NAMES {
        text.push_str(&format!("record {name}<T>:\n    value: T\nfn make_{name}<T>(value: T) -> {name}<T>:\n    {name}(value: value)\n"));
        text.push_str(&format!("fn nest_{name}<T>(value: T) -> List<{name}<T>>:\n    [make_{name}(value), {name}(value: value)]\n"));
    }
    text.push_str("fn main() -> i64:\n");
    for (i, name) in NAMES.iter().enumerate() {
        text.push_str(&format!(
            "    let v{i} = make_{name}(42)\n    let b{i} = make_{name}(true)\n"
        ));
        text.push_str(&format!(
            "    let nested{i} = nest_{name}(42)\n    let flags{i} = nest_{name}(true)\n"
        ));
    }
    text.push_str(&format!(
        "    {}\n",
        (0..NAMES.len())
            .map(|i| format!("v{i}.value"))
            .collect::<Vec<_>>()
            .join(" + ")
    ));
    source(&text, "420\n");
}

#[test]
fn qualified_and_imported_constructors_keep_defining_identity() {
    let dir = directory();
    fs::create_dir(dir.join("src")).unwrap();
    fs::write(dir.join("cgx2.toml"), "schema_version = 1\nname = \"app\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\", \"src/dep.cg\"]\n").unwrap();
    let mut dep = String::from("module dep\n");
    let mut main = String::from("module app\nimport dep\nfn main() -> i64:\n");
    for (i, name) in NAMES.iter().enumerate() {
        dep.push_str(&format!("pub record {name}<T>:\n    value: T\npub fn make_{name}<T>(value: T) -> {name}<T>:\n    dep.{name}(value: value)\n"));
        main.push_str(&format!(
            "    let a{i} = dep.{name}<i64>(value: 21)\n    let b{i} = dep.make_{name}(21)\n"
        ));
    }
    main.push_str(&format!(
        "    {}\n",
        (0..NAMES.len())
            .map(|i| format!("a{i}.value + b{i}.value"))
            .collect::<Vec<_>>()
            .join(" + ")
    ));
    fs::write(dir.join("src/dep.cg"), dep).unwrap();
    fs::write(dir.join("src/main.cg"), main).unwrap();
    let package = load_checked_package(&dir).unwrap();
    let generated = emit_package_rust(&package).unwrap();
    assert_eq!(generated, emit_package_rust(&package).unwrap());
    execute(&generated, Ok("420\n"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn record_returning_functions_and_real_intrinsics_still_dispatch() {
    source(
        "record Box<T>:\n    value: T\nfn length<T>(value: T) -> Box<T>:\n    Box(value: value)\nfn main() -> i64:\n    let x = length(42)\n    let y = length(true)\n    x.value\n",
        "42\n",
    );
    source(
        "fn main() -> String:\n    let xs = append([40], 2)\n    let ys = list(array(xs))\n    let n = length(ys)\n    let a = get(ys, 0)\n    let b = updated(ys, 0, 42)\n    let c = slice(\"éab\", 0, 1)\n    let d = parse_i64(\"42\")\n    concat(show(n), show(d))\n",
        "2cgx2.core.Result<i64, String>.Ok(42)\n",
    );
}

#[test]
fn constructor_arguments_keep_source_order_and_single_evaluation() {
    // One call fits the checked work budget; two exceed it. A duplicate
    // evaluation while constructing the record must therefore fail this case.
    let once = "record length:\n    value: i64\nfn costly() -> i64:\n    let mut x = 0\n    for i in range(0, 150000):\n        x += 1\n    x\nfn main() -> i64:\n    let x = length(value: costly())\n    x.value\n";
    source(once, "150000\n");
    let twice = once.replace("value: costly()", "value: costly() + costly()");
    assert_eq!(run_source(&twice, &[]).unwrap_err().code, "CGR010");
    let module = parse_module(&twice).unwrap();
    let typed = check_module(&module).unwrap();
    execute(&emit_rust(&module, &typed).unwrap(), Err("CGR010"));
    // The first source argument fails with division by zero. Reordering to field
    // declaration order would report overflow instead.
    let text = "record length:\n    first: i64\n    second: i64\nfn main() -> i64:\n    let x = length(second: 1 / 0, first: 9223372036854775807 + 1)\n    x.first\n";
    assert_eq!(run_source(text, &[]).unwrap_err().code, "CGR006");
    let module = parse_module(text).unwrap();
    let typed = check_module(&module).unwrap();
    let generated = emit_rust(&module, &typed).unwrap();
    execute(&generated, Err("CGR006"));
}
