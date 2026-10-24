use cgx2_cli::{run_checked_source, run_native_package};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const CAP: usize = 1_048_576;

struct Package(PathBuf);
impl Package {
    fn new(source: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "cgx2-output-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).unwrap();
        // The dependency sorts before the entry and has a different source span.
        std::fs::write(
            root.join("src/helper.cg"),
            "module a.helper\npub fn value() -> i64:\n    1\n",
        )
        .unwrap();
        std::fs::write(root.join("src/main.cg"), source).unwrap();
        std::fs::write(root.join("cgx2.toml"), "schema_version = 1\nname = \"out\"\nentry = \"src/main.cg\"\nsources = [\"src/helper.cg\", \"src/main.cg\"]\n").unwrap();
        Self(root)
    }
}
impl Drop for Package {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

// Names dominate the rendered size while the finite value's data stays small.
// Source escapes decode to raw displayed scalars, whose UTF-8 bytes count.
fn boundary_source(bytes: usize, package: bool) -> (String, String) {
    let name = format!("Long{}", "x".repeat(596));
    let prefix = if package { "out.main." } else { "" };
    let item = format!("{prefix}{name}(x: 1)");
    let items = vec![item; 1600].join(", ");
    let empty = format!("{prefix}Outer(xs: [{items}], pad: )");
    let padding_bytes = bytes - empty.len();
    let pattern = "é\n\"\\🙂";
    let padding = format!(
        "{}{}",
        pattern.repeat(padding_bytes / pattern.len()),
        "x".repeat(padding_bytes % pattern.len())
    );
    let expected = format!("{prefix}Outer(xs: [{items}], pad: {padding})");
    let source = format!(
        "{}record {name}:\n    x: i64\nrecord Outer:\n    xs: List<{name}>\n    pad: String\nfn main() -> Outer:\n    let v = {name}(x: 1)\n    Outer(xs: [{}], pad: {})\n",
        if package { "module out.main\n" } else { "" },
        vec!["v"; 1600].join(", "),
        serde_json::to_string(&padding).unwrap()
    );
    assert_eq!(expected.len(), bytes);
    (source, expected)
}

#[test]
fn module_and_package_render_below_at_and_above_utf8_byte_cap() {
    for bytes in [CAP - 1, CAP, CAP + 1] {
        let (source, expected) = boundary_source(bytes, false);
        let (package_source, package_expected) = boundary_source(bytes, true);
        let package = Package::new(&package_source);
        let standalone = run_checked_source(&source, &[]);
        let packaged = run_native_package(&package.0, &[]);
        if bytes <= CAP {
            assert_eq!(standalone.unwrap().0.value.to_string(), expected);
            assert_eq!(packaged.unwrap().value.to_string(), package_expected);
        } else {
            let standalone = standalone.unwrap_err();
            let packaged = packaged.unwrap_err();
            assert_eq!(standalone[0].code, "CGR010");
            assert_eq!(standalone[0].message, "output exceeds 1048576 bytes");
            assert_eq!(packaged[0].code, standalone[0].code);
            assert_eq!(packaged[0].message, standalone[0].message);
            assert_eq!(packaged[0].file, Path::new("src/main.cg"));
            assert_eq!(
                standalone[0].span,
                Some(cgx2_syntax::parse_module(&source).unwrap().span)
            );
            assert_eq!(
                packaged[0].span,
                Some(cgx2_syntax::parse_module(&package_source).unwrap().span)
            );
        }
    }
}

#[test]
fn cli_rejects_before_publishing_and_json_points_to_entry() {
    let (source, _) = boundary_source(CAP + 1, true);
    let package = Package::new(&source);
    let standalone = package.0.join("standalone.cg");
    let (standalone_source, _) = boundary_source(CAP + 1, false);
    std::fs::write(&standalone, &standalone_source).unwrap();
    for path in [package.0.clone(), standalone] {
        for json in [false, true] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_cgx2"));
            command.arg("run");
            if json {
                command.arg("--diagnostic-format=json");
            }
            let output = command
                .arg(&path)
                .current_dir(std::env::temp_dir())
                .output()
                .unwrap();
            assert!(!output.status.success());
            assert!(output.stdout.is_empty());
            let stderr = String::from_utf8(output.stderr).unwrap();
            assert!(stderr.contains("CGR010"), "{stderr}");
            if json {
                let diagnostic: serde_json::Value = serde_json::from_str(stderr.trim()).unwrap();
                assert_eq!(diagnostic["code"], "CGR010");
                assert!(diagnostic["file"]
                    .as_str()
                    .unwrap()
                    .ends_with(if path.is_dir() {
                        "src/main.cg"
                    } else {
                        "standalone.cg"
                    }));
                assert!(
                    diagnostic["span"]["end"].as_u64().unwrap()
                        <= if path.is_dir() {
                            source.len()
                        } else {
                            standalone_source.len()
                        } as u64
                );
            }
        }
    }
}

#[test]
fn nested_variant_record_array_and_show_preserve_display_and_scalar_semantics() {
    let source = "record Boxed:\n    text: String\nvariant Reply:\n    Item(Array<List<Boxed>>)\nfn main() -> String:\n    let text = \"é\\n\\\"\\\\🙂\"\n    let result = Reply.Item(array([[Boxed(text: text)]]))\n    concat(show(result), show(length(text)))\n";
    let expected = "Reply.Item(Array[[Boxed(text: é\n\"\\🙂)]])5";
    assert_eq!(
        run_checked_source(source, &[]).unwrap().0.value.to_string(),
        expected
    );
    assert_eq!(
        run_native_package(&Package::new(&format!("module nested\n{source}")).0, &[])
            .unwrap()
            .value
            .to_string(),
        expected
            .replace("Reply.", "nested.Reply.")
            .replace("Boxed(", "nested.Boxed(")
    );
}

#[test]
fn show_rejects_oversized_display_before_string_materialization() {
    for package in [false, true] {
        let (source, _) = boundary_source(CAP + 1, package);
        let source = source
            .replace("fn main() -> Outer:", "fn main() -> String:")
            .replace("    Outer(xs:", "    show(Outer(xs:");
        let source = format!("{})\n", source.trim_end());
        let span = if package {
            let error = run_native_package(&Package::new(&source).0, &[]).unwrap_err();
            assert_eq!(error[0].code, "CGR010");
            assert_eq!(error[0].message, "output exceeds 1048576 bytes");
            error[0].span.unwrap()
        } else {
            let error = run_checked_source(&source, &[]).unwrap_err();
            assert_eq!(error[0].code, "CGR010");
            assert_eq!(error[0].message, "output exceeds 1048576 bytes");
            error[0].span.unwrap()
        };
        assert!(source[span.start..span.end].starts_with("show("));
    }
}
