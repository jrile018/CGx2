use cgx2_effect_index::package_v1::{index_checked_package, index_package, Identity, Resolution};
use cgx2_package::load_checked_package;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new(base: &str, middle: &str, main: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "effect-package-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("cgx2.toml"), "schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\",\"src/middle.cg\",\"src/base.cg\"]\n").unwrap();
        fs::write(
            root.join("src/base.cg"),
            format!("module demo.base\n{base}"),
        )
        .unwrap();
        fs::write(
            root.join("src/middle.cg"),
            format!("module demo.middle\nimport demo.base\n{middle}"),
        )
        .unwrap();
        fs::write(
            root.join("src/main.cg"),
            format!("module demo.main\nimport demo.middle\n{main}"),
        )
        .unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn pure_wrappers_use_checked_public_edges_and_body_derived_effects() {
    let f = Fixture::new(
        "pub fn value() -> i64 effects(ReadArgs) caps(ArgsRead):\n    7\n",
        "pub fn value() -> i64 effects() caps():\n    demo.base.value()\n",
        "pub fn value() -> i64 effects() caps():\n    demo.middle.value()\n",
    );
    let index = index_package(&f.0).unwrap();
    let main = index
        .entry(&Identity {
            source: "src/main.cg".into(),
            name: "value".into(),
        })
        .unwrap();
    assert!(main.effects.is_empty());
    assert!(main.capabilities.is_empty());
    assert!(
        matches!(&main.calls[0].resolution,Resolution::Function(id) if id.source == std::path::Path::new("src/middle.cg") && id.name == "value")
    );
    assert_eq!(
        main.calls[0].declaration.as_ref().unwrap().source,
        PathBuf::from("src/middle.cg")
    );
    let json = index.to_json().unwrap();
    assert_eq!(json, index.to_json().unwrap());
    assert!(json.contains("sha256"));
    assert!(!json.contains(&f.0.to_string_lossy().to_string()));
}
#[test]
fn inactive_branch_empty_loop_and_forwarding_preserve_effects_and_caps() {
    let f=Fixture::new("pub fn value() -> i64 effects(ReadArgs) caps(ArgsRead):\n    arg_count()\n", "pub fn value() -> i64:\n    if false:\n        demo.base.value()\n    else:\n        0\n    for i in range(0, 0):\n        0\n    return 0\n", "pub fn value() -> i64:\n    demo.middle.value()\n");
    let index = index_package(&f.0).unwrap();
    for entry in index
        .entries()
        .iter()
        .filter(|e| e.identity.name == "value")
    {
        assert!(entry.effects.contains("ReadArgs"));
        assert!(entry.capabilities.contains("ArgsRead"));
        assert!(!entry.effects.contains("Unknown"));
    }
}
#[test]
fn dishonest_private_unimported_duplicate_and_unknown_reject() {
    for (base, middle, main) in [
        (
            "pub fn value() -> i64 effects() caps():\n    arg_count()\n",
            "pub fn value() -> i64:\n    demo.base.value()\n",
            "pub fn value() -> i64:\n    demo.middle.value()\n",
        ),
        (
            "fn value() -> i64:\n    1\n",
            "pub fn value() -> i64:\n    demo.base.value()\n",
            "pub fn value() -> i64:\n    demo.middle.value()\n",
        ),
        (
            "pub fn value() -> i64:\n    1\n",
            "pub fn value() -> i64:\n    demo.base.value()\n",
            "pub fn value() -> i64:\n    demo.base.value()\n",
        ),
        (
            "pub fn value() -> i64:\n    1\npub fn value() -> i64:\n    2\n",
            "pub fn value() -> i64:\n    demo.base.value()\n",
            "pub fn value() -> i64:\n    demo.middle.value()\n",
        ),
        (
            "pub fn value() -> i64:\n    missing()\n",
            "pub fn value() -> i64:\n    demo.base.value()\n",
            "pub fn value() -> i64:\n    demo.middle.value()\n",
        ),
    ] {
        let f = Fixture::new(base, middle, main);
        assert!(index_package(&f.0).is_err());
    }
}
#[test]
fn mutation_rejected_on_entry_and_serialization() {
    let f = Fixture::new(
        "pub fn value() -> i64:\n    1\n",
        "pub fn value() -> i64:\n    demo.base.value()\n",
        "pub fn value() -> i64:\n    demo.middle.value()\n",
    );
    let checked = load_checked_package(&f.0).unwrap();
    let index = index_checked_package(&checked).unwrap();
    fs::write(
        f.0.join("src/base.cg"),
        "module demo.base\npub fn value() -> i64:\n    2\n",
    )
    .unwrap();
    assert!(index_checked_package(&checked).is_err());
    assert!(index.to_json().is_err());
    assert!(index.to_text().is_err());
}
#[test]
fn command_supports_checked_package_and_rejects_unchecked_package() {
    let f = Fixture::new(
        "pub fn value() -> i64:\n    1\n",
        "pub fn value() -> i64:\n    demo.base.value()\n",
        "pub fn value() -> i64:\n    demo.middle.value()\n",
    );
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cgx2-effect-index"))
        .arg("--package")
        .arg(&f.0)
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["analysis"], "checked_package_effects_v1");
    assert!(
        !std::process::Command::new(env!("CARGO_BIN_EXE_cgx2-effect-index"))
            .arg("--package")
            .arg(&f.0)
            .arg("--syntax-only")
            .output()
            .unwrap()
            .status
            .success()
    );
}
#[test]
fn computed_shadowed_calls_and_duplicate_modules_do_not_receive_trusted_edges() {
    for body in ["    (1)(2)\n", "    let value = 1\n    value(2)\n"] {
        let f = Fixture::new(
            "pub fn value() -> i64:\n    1\n",
            "pub fn value() -> i64:\n    demo.base.value()\n",
            &format!("pub fn value() -> i64:\n{body}"),
        );
        assert!(index_package(&f.0).is_err());
    }
    let f = Fixture::new(
        "pub fn value() -> i64:\n    1\n",
        "pub fn value() -> i64:\n    demo.base.value()\n",
        "pub fn value() -> i64:\n    demo.middle.value()\n",
    );
    fs::write(
        f.0.join("src/base.cg"),
        "module demo.middle\npub fn value() -> i64:\n    1\n",
    )
    .unwrap();
    assert!(index_package(&f.0).is_err());
}
#[test]
fn manifest_drift_is_rejected_when_rendering_snapshot() {
    let f = Fixture::new(
        "pub fn value() -> i64:\n    1\n",
        "pub fn value() -> i64:\n    demo.base.value()\n",
        "pub fn value() -> i64:\n    demo.middle.value()\n",
    );
    let index = index_package(&f.0).unwrap();
    let manifest = f.0.join("cgx2.toml");
    fs::write(
        &manifest,
        fs::read_to_string(&manifest).unwrap() + "# drift\n",
    )
    .unwrap();
    assert!(index.to_json().is_err());
}
#[cfg(unix)]
#[test]
fn canonical_source_retarget_rejects_even_identical_bytes() {
    let f = Fixture::new(
        "pub fn value() -> i64:\n    1\n",
        "pub fn value() -> i64:\n    demo.base.value()\n",
        "pub fn value() -> i64:\n    demo.middle.value()\n",
    );
    let index = index_package(&f.0).unwrap();
    fs::rename(f.0.join("src/base.cg"), f.0.join("src/base-copy.cg")).unwrap();
    std::os::unix::fs::symlink("base-copy.cg", f.0.join("src/base.cg")).unwrap();
    assert!(index.to_json().is_err());
}
#[test]
fn imported_record_constructor_and_comparison_are_checked_pure() {
    let f = Fixture::new(
        "pub record Point:\n    x: i64\n",
        "pub fn value() -> demo.base.Point effects() caps():\n    demo.base.Point(x: 1)\n",
        "pub fn value() -> Bool effects() caps():\n    demo.middle.value().x == 1\n",
    );
    // The frozen parser supports only a named value as a field base.
    fs::write(f.0.join("src/main.cg"),"module demo.main\nimport demo.middle\nimport demo.base\npub fn value() -> Bool effects() caps():\n    let point = demo.middle.value()\n    point.x == 1\n").unwrap();
    let index = index_package(&f.0).unwrap();
    assert!(index.entries().iter().all(|e| e.effects.is_empty()));
}

#[test]
fn checked_native_declaration_effect_and_capability_propagate_without_execution() {
    let f=Fixture::new("unsafe foreign native pub fn value(x: i64) -> i64 effects(Foreign) caps(native) = fixture.value runtime \"fixture@1\" ownership copied\n", "pub fn value() -> i64 effects(Foreign) caps(native):\n    demo.base.value(1)\n", "pub fn value() -> i64 effects(Foreign) caps(native):\n    demo.middle.value()\n");
    let manifest = f.0.join("cgx2.toml");
    fs::write(
        &manifest,
        fs::read_to_string(&manifest)
            .unwrap()
            .replace("schema_version = 1", "schema_version = 2")
            + "binding_lock = \"cgx2-bind.lock.json\"\n",
    )
    .unwrap();
    let index = index_package(&f.0).unwrap();
    for entry in index
        .entries()
        .iter()
        .filter(|e| e.identity.name == "value")
    {
        assert_eq!(
            entry.effects.iter().map(String::as_str).collect::<Vec<_>>(),
            ["Foreign"]
        );
        assert_eq!(
            entry
                .capabilities
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["native"]
        );
    }
}
