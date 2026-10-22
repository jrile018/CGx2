use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use cgx2_package::load_package;

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2-package-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn root(&self) -> &Path {
        &self.0
    }

    fn write(&self, name: &str, content: &str) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn manifest(&self, body: &str) {
        self.write("cgx2.toml", body);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const MANIFEST: &str = "schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\", \"src/math.cg\"]\n";

fn codes(fixture: &Fixture) -> Vec<&'static str> {
    load_package(fixture.root())
        .unwrap_err()
        .iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn two_modules_have_sorted_public_api_and_stable_rendering() {
    let f = Fixture::new();
    f.manifest(MANIFEST);
    f.write(
        "src/main.cg",
        "module demo.main\npub fn run(x: i64) -> i64:\n    x\n",
    );
    f.write(
        "src/math.cg",
        "module demo.math\nfn hidden(x: i64) -> i64:\n    x\npub fn twice(x: i64) -> i64:\n    x * 2\n",
    );
    let first = load_package(f.root()).unwrap();
    let second = load_package(f.root()).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.entry, PathBuf::from("src/main.cg"));
    assert_eq!(first.modules.len(), 2);
    assert_eq!(first.modules[0].name.as_deref(), Some("demo.main"));
    assert_eq!(first.modules[1].name.as_deref(), Some("demo.math"));
    assert_eq!(first.modules[0].api.functions[0].name, "run");
    assert_eq!(first.modules[1].api.functions[0].name, "twice");
    assert_eq!(first.modules[1].api.functions.len(), 1);
    assert!(!first.imports_supported);
    assert_eq!(first.render_text(), second.render_text());
    assert_eq!(first.render_json(), second.render_json());
    assert!(first.render_json().contains("\"imports_supported\":false"));
    assert!(first.render_json().contains("\"return_type\":\"i64\""));
}

#[test]
fn committed_example_matches_expected_index() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo");
    let index = load_package(&root).unwrap();
    let expected = include_str!("../fixtures/demo/expected-index.json").trim_end();
    assert_eq!(index.render_json(), expected);
    assert_eq!(index.modules[0].name.as_deref(), Some("demo.main"));
    assert_eq!(index.modules[1].name.as_deref(), Some("demo.math"));
}

#[test]
fn manifest_rejects_missing_duplicate_unknown_and_malformed_values() {
    let f = Fixture::new();
    f.manifest("name = \"demo\"\n");
    let errors = load_package(f.root()).unwrap_err();
    assert!(errors
        .iter()
        .any(|d| d.code == "CGP006" && d.span.is_some()));
    f.manifest("schema_version = 1\nname = \"demo\"\nname = \"again\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\n");
    assert!(codes(&f).contains(&"CGP002"));
    f.manifest("schema_version = 1\nname = \"demo\"\nextra = 1\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\n");
    assert!(codes(&f).contains(&"CGP001"));
    f.manifest("schema_version = 1\nname = \"unterminated\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\n");
    assert!(codes(&f).contains(&"CGP003"));
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\", \"src/main.cg\"]\n");
    assert!(codes(&f).contains(&"CGP007"));
}

#[test]
fn invalid_utf8_and_missing_entry_are_reported() {
    let f = Fixture::new();
    fs::write(f.root().join("cgx2.toml"), [0xff]).unwrap();
    assert!(codes(&f).contains(&"CGP004"));
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src/other.cg\"\nsources = [\"src/main.cg\"]\n");
    assert!(codes(&f).contains(&"CGP008"));
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\n");
    fs::create_dir_all(f.root().join("src")).unwrap();
    fs::write(f.root().join("src/main.cg"), [0xff]).unwrap();
    let errors = load_package(f.root()).unwrap_err();
    assert!(errors
        .iter()
        .any(|d| d.code == "CGP004" && d.file.ends_with("src/main.cg")));
}

#[test]
fn size_limits_and_missing_file_are_explicit() {
    let f = Fixture::new();
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\n");
    let missing = load_package(f.root()).unwrap_err();
    assert!(missing.iter().any(|d| d.code == "CGP005"
        && d.file.ends_with("src/main.cg")
        && d.message.contains("src/main.cg")));
    fs::create_dir_all(f.root().join("src")).unwrap();
    fs::write(
        f.root().join("src/main.cg"),
        vec![b' '; cgx2_package::MAX_SOURCE_BYTES + 1],
    )
    .unwrap();
    assert!(codes(&f).contains(&"CGP013"));
    fs::write(
        f.root().join("cgx2.toml"),
        vec![b' '; cgx2_package::MAX_MANIFEST_BYTES + 1],
    )
    .unwrap();
    assert!(codes(&f).contains(&"CGP013"));
}

#[test]
fn source_errors_and_duplicate_module_names_keep_source_locations() {
    let f = Fixture::new();
    f.manifest(MANIFEST);
    f.write(
        "src/main.cg",
        "module demo.same\npub fn run(x: i64) -> i64:\n    x\n",
    );
    f.write(
        "src/math.cg",
        "module demo.same\npub fn twice(x: i64) -> i64:\n    x\n",
    );
    assert!(codes(&f).contains(&"CGP010"));
    f.write(
        "src/math.cg",
        "module demo.math\npub fn twice(x: i64) -> i64:\n    nope\n",
    );
    let errors = load_package(f.root()).unwrap_err();
    assert!(errors
        .iter()
        .any(|d| d.code.starts_with("CGT") && d.span.is_some() && d.file.ends_with("src/math.cg")));
    f.write(
        "src/math.cg",
        "module demo.math\npub fn twice(x: i64):\n    x\n",
    );
    assert!(codes(&f).iter().any(|code| code.starts_with("CGT")));
    f.write("src/math.cg", "module demo.math\npub fn twice(:\n");
    assert!(codes(&f).iter().any(|code| code.starts_with("CGS")));
    f.write("src/math.cg", "pub fn twice(x: i64) -> i64:\n    x\n");
    assert!(codes(&f).contains(&"CGP009"));
}

#[test]
fn paths_cannot_escape_and_aliases_cannot_duplicate_sources() {
    let f = Fixture::new();
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"../outside.cg\"\nsources = [\"../outside.cg\"]\n");
    assert!(codes(&f).contains(&"CGP011"));
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src/./main.cg\"\nsources = [\"src/./main.cg\"]\n");
    assert!(codes(&f).contains(&"CGP011"));
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src//main.cg\"\nsources = [\"src//main.cg\"]\n");
    assert!(codes(&f).contains(&"CGP011"));
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\", \"src/alias.cg\"]\n");
    f.write("src/main.cg", "module demo.main\n");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("main.cg", f.root().join("src/alias.cg")).unwrap();
        assert!(codes(&f).contains(&"CGP007"));
    }
}

#[cfg(unix)]
#[test]
fn outside_symlink_is_rejected_before_source_read() {
    let f = Fixture::new();
    let outside = Fixture::new();
    outside.write("secret.cg", "module secret\n");
    f.manifest("schema_version = 1\nname = \"demo\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\n");
    fs::create_dir_all(f.root().join("src")).unwrap();
    std::os::unix::fs::symlink(
        outside.root().join("secret.cg"),
        f.root().join("src/main.cg"),
    )
    .unwrap();
    let errors = load_package(f.root()).unwrap_err();
    assert!(errors
        .iter()
        .any(|d| d.code == "CGP011" && d.file.ends_with("src/main.cg")));
}
