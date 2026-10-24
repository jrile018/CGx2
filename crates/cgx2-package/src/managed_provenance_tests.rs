use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "managed-source-auth-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("cgx2.toml"), "schema_version = 1\nname = \"auth\"\nentry = \"main.cg\"\nsources = [\"main.cg\", \"lib.cg\"]\n").unwrap();
        fs::write(
            root.join("lib.cg"),
            "module app.lib\npub fn values() -> List<i64> effects():\n [40, 2]\n",
        )
        .unwrap();
        fs::write(root.join("main.cg"), "module app.main\nimport app.lib\nfn main() -> i64 effects():\n length(app.lib.values())\n").unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn package_sources_rebuild_and_reject_stale_import_or_manifest() {
    let fixture = Fixture::new();
    let package = load_checked_package(&fixture.0).unwrap();
    let program = package.managed_program_v1().unwrap();
    assert_eq!(
        program.execute(&[]).unwrap().result.unwrap().to_string(),
        "2"
    );
    assert_eq!(program.provenance_v1().kind(), "source-bundle-v1");
    let lib = fixture.0.join("lib.cg");
    let original = fs::read_to_string(&lib).unwrap();
    fs::write(&lib, original.replace("[40, 2]", "[40, 2, 3]")).unwrap();
    assert_eq!(package.managed_program_v1().unwrap_err()[0].code, "CGP018");
    let current = load_checked_package(&fixture.0)
        .unwrap()
        .managed_program_v1()
        .unwrap();
    assert_ne!(current.analysis_sha256(), program.analysis_sha256());
    assert_ne!(
        current.provenance_v1().identity_sha256(),
        program.provenance_v1().identity_sha256()
    );
    fs::write(&lib, original).unwrap();
    fs::write(fixture.0.join("cgx2.toml"), "# changed manifest\n").unwrap();
    assert_eq!(package.managed_program_v1().unwrap_err()[0].code, "CGP018");
}
#[test]
fn checked_ast_cannot_substitute_for_source_bundle_and_import_visibility_is_kept() {
    let fixture = Fixture::new();
    let mut package = load_checked_package(&fixture.0).unwrap();
    package.modules[0].syntax = parse_module("fn main() -> i64:\n 999\n").unwrap();
    let program = package.managed_program_v1().unwrap();
    assert_eq!(
        program.execute(&[]).unwrap().result.unwrap().to_string(),
        "2"
    );
    let lib = fixture.0.join("lib.cg");
    let original = fs::read_to_string(&lib).unwrap();
    fs::write(&lib, original.replace("pub fn", "fn")).unwrap();
    assert!(load_checked_package(&fixture.0).is_err());
}
