use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use cgx2_package::load_checked_package;

static NEXT: AtomicU64 = AtomicU64::new(0);

#[test]
fn scalar_v1_and_explicit_binding_v2_manifests() {
    let root = std::env::temp_dir().join(format!(
        "cgx2-package-bindings-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/main.cg"),
        "module demo.main\npub fn run(x: i64) -> i64:\n    x\n",
    )
    .unwrap();
    let v1 = "schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\n";
    fs::write(root.join("cgx2.toml"), v1).unwrap();
    assert_eq!(load_checked_package(&root).unwrap().binding_lock(), None);
    let v2 = format!("{v1}binding_lock = \"cgx2-bind.lock.json\"\n")
        .replace("schema_version = 1", "schema_version = 2");
    fs::write(root.join("cgx2.toml"), &v2).unwrap();
    assert_eq!(
        load_checked_package(&root).unwrap().binding_lock(),
        Some(PathBuf::from("cgx2-bind.lock.json").as_path())
    );
    fs::write(
        root.join("cgx2.toml"),
        v2.replace("cgx2-bind.lock.json", "../escape.json"),
    )
    .unwrap();
    assert_eq!(load_checked_package(&root).unwrap_err()[0].code, "CGP011");
    fs::write(
        root.join("cgx2.toml"),
        format!("{v1}binding_lock = \"x.json\"\n"),
    )
    .unwrap();
    assert_eq!(load_checked_package(&root).unwrap_err()[0].code, "CGP011");
    fs::write(root.join("cgx2.toml"), format!("{v1}unknown = 1\n")).unwrap();
    assert_eq!(load_checked_package(&root).unwrap_err()[0].code, "CGP001");
    fs::remove_dir_all(root).unwrap();
}
