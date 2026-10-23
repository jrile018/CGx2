use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
const MAIN: &str = include_str!("../../cgx2-syntax/s2a-evidence/fixtures/package/src/main.cg");
fn fixture(main: &str) -> Temp {
    let root = std::env::temp_dir().join(format!(
        "cgx2-s2a-package-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(root.join("src")).unwrap();
    for (path, text) in [
        (
            "cgx2.toml",
            include_str!("../../cgx2-syntax/s2a-evidence/fixtures/package/cgx2.toml"),
        ),
        ("src/main.cg", main),
        (
            "src/left.cg",
            include_str!("../../cgx2-syntax/s2a-evidence/fixtures/package/src/left.cg"),
        ),
        (
            "src/right.cg",
            include_str!("../../cgx2-syntax/s2a-evidence/fixtures/package/src/right.cg"),
        ),
    ] {
        fs::write(root.join(path), text).unwrap();
    }
    Temp(root)
}
#[test]
fn imported_private_unknown_and_unrelated_types_have_source_diagnostics() {
    for (source, code) in [
        (
            MAIN.replace(
                "let first: app.left.Reply = app.left.make(20)",
                "let first: app.right.Reply = app.left.make(20)",
            ),
            "CGT003",
        ),
        (
            MAIN.replace(
                "let first: app.left.Reply = app.left.make(20)",
                "let first = app.left.Secret.Hidden()",
            ),
            "CGT022",
        ),
        (
            MAIN.replace(
                "app.right.Reply.Value(point)",
                "app.left.Reply.Value(point)",
            ),
            "CGT003",
        ),
        (
            MAIN.replace(
                "let first: app.left.Reply = app.left.make(20)",
                "let first: app.left.Unknown = app.left.make(20)",
            ),
            "CGT002",
        ),
        (
            MAIN.replace(
                "let first: app.left.Reply = app.left.make(20)",
                "let first = app.left.Reply.Missing()",
            ),
            "CGT033",
        ),
        (MAIN.replace("import app.left\n", ""), "CGT008"),
    ] {
        let temp = fixture(&source);
        let errors = cgx2_package::load_checked_package(&temp.0).unwrap_err();
        assert!(errors.iter().any(|e| e.code == code), "{errors:?}");
        for error in errors {
            if let Some(span) = error.span {
                let text = fs::read_to_string(temp.0.join(error.file)).unwrap();
                assert!(text.get(span.start..span.end).is_some());
            }
        }
    }
}
