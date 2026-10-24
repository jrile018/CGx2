use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use cgx2_package::{load_checked_package, CheckedPackage};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "cgx2-package-binders-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(name);
        fs::copy(source.join("cgx2.toml"), root.join("cgx2.toml")).unwrap();
        for entry in fs::read_dir(source.join("src")).unwrap() {
            let entry = entry.unwrap();
            fs::copy(entry.path(), root.join("src").join(entry.file_name())).unwrap();
        }
        Self(root)
    }

    fn replace(&self, path: &str, from: &str, to: &str) {
        let path = self.0.join(path);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "missing replacement in {path:?}");
        fs::write(path, text.replace(from, to)).unwrap();
    }

    fn check(&self) -> CheckedPackage {
        load_checked_package(&self.0).unwrap()
    }

    fn rejects(&self, code: &str) {
        let errors = load_checked_package(&self.0).unwrap_err();
        assert!(errors.iter().any(|error| error.code == code), "{errors:?}");
        for error in errors {
            if let Some(span) = error.span {
                let source = fs::read_to_string(self.0.join(error.file)).unwrap();
                assert!(source.get(span.start..span.end).is_some());
            }
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn imported_record_binder_shadows_same_named_local_record() {
    let fixture = Fixture::new("binder-record");
    let checked = fixture.check();
    checked.verify_unchanged().unwrap();
    assert_eq!(
        checked.index().render_json(),
        fixture.check().index().render_json()
    );
}

#[test]
fn imported_variant_binder_shadows_same_named_local_variant() {
    Fixture::new("binder-variant").check();
}

#[test]
fn nested_binders_and_qualified_and_unshadowed_nominals_coexist() {
    Fixture::new("binder-nested").check();
    for (from, to) in [
        (
            "let nominal = app.left.T(value: 1)",
            "let nominal = app.right.T.Nominal(true)",
        ),
        (
            "let tag = app.left.Tag(value: 2)",
            "let tag = app.left.T(value: 2)",
        ),
    ] {
        let fixture = Fixture::new("binder-nested");
        fixture.replace("src/main.cg", from, to);
        fixture.rejects("CGT003");
    }
}

#[test]
fn distinct_modules_keep_colliding_generic_identities() {
    let fixture = Fixture::new("binder-nested");
    fixture.replace(
        "src/main.cg",
        "let right = app.right.Box<i64>",
        "let right: app.left.Box<i64> = app.right.Box<i64>",
    );
    fixture.rejects("CGT003");
}

#[test]
fn imported_binders_still_require_the_declared_argument_types() {
    for (name, from, to) in [
        ("binder-record", "value: 42", "value: true"),
        ("binder-variant", ".Value(42)", ".Value(true)"),
        ("binder-nested", "array(right.value)", "array([true])"),
    ] {
        let fixture = Fixture::new(name);
        fixture.replace("src/main.cg", from, to);
        fixture.rejects("CGT003");
    }
}

#[test]
fn imported_generic_constraints_remain_required() {
    let fixture = Fixture::new("binder-nested");
    fixture.replace(
        "src/main.cg",
        "fn main() -> i64:",
        "fn bad<U>(value: U) -> U effects() caps():\n    app.left.identity<U>(value)\nfn main() -> i64:",
    );
    fixture.rejects("CGT039");
}

#[test]
fn generic_type_visibility_remains_checked() {
    let fixture = Fixture::new("binder-record");
    fixture.replace("src/dep.cg", "pub record Box<T>", "record Box<T>");
    fixture.rejects("CGT022");
}

#[test]
fn changed_generic_dependency_invalidates_snapshot_and_effect_admission() {
    let fixture = Fixture::new("binder-nested");
    let checked = fixture.check();
    let dependency = checked
        .modules()
        .iter()
        .find(|module| module.source() == Path::new("src/left.cg"))
        .unwrap();
    let old_hash = dependency.source_sha256().to_owned();
    fixture.replace(
        "src/left.cg",
        "module app.left",
        "module app.left\n# changed source",
    );
    assert_eq!(checked.verify_unchanged().unwrap_err()[0].code, "CGP018");
    let fresh = fixture.check();
    let fresh_dependency = fresh
        .modules()
        .iter()
        .find(|module| module.source() == Path::new("src/left.cg"))
        .unwrap();
    assert_ne!(fresh_dependency.source_sha256(), old_hash);
    fresh.verify_unchanged().unwrap();
    fixture.replace(
        "src/left.cg",
        "    value\npub fn equal",
        "    let count = arg_count()\n    value\npub fn equal",
    );
    assert_eq!(fresh.verify_unchanged().unwrap_err()[0].code, "CGP018");
    fixture.rejects("CGT012");
}
