use cgx2_emit_rust::emit_package_rust;
use cgx2_package::load_checked_package;
use std::fs;
#[test]
fn changed_generic_source_invalidates_the_native_snapshot() {
    let root =
        std::env::temp_dir().join(format!("cgx2-s2b-native-snapshot-{}", std::process::id()));
    fs::create_dir_all(root.join("src")).unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../cgx2-syntax/s2b-evidence/package");
    for name in ["cgx2.toml", "src/main.cg", "src/left.cg", "src/right.cg"] {
        fs::copy(source.join(name), root.join(name)).unwrap();
    }
    let package = load_checked_package(&root).unwrap();
    let changed = root.join("src/left.cg");
    let mut bytes = fs::read(&changed).unwrap();
    bytes.extend_from_slice(b"\n");
    fs::write(&changed, bytes).unwrap();
    let errors = emit_package_rust(&package).unwrap_err();
    assert!(errors.iter().any(|d| d.diagnostic.code == "CGP018"));
    fs::remove_dir_all(root).unwrap();
}
