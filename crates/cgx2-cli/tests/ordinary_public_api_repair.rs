use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const SOURCE: &str =
    include_str!("../../cgx2-syntax/ordinary-api-repair-evidence/fixtures/public.cg");
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "ordinary-public-api-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }
    fn api(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_cgx2"))
            .args(args)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn checked_json(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn file_human_and_json_api_hide_private_bodies_but_effects_keep_them() {
    let fixture = Fixture::new();
    fixture.write("public.cg", SOURCE);
    let index = checked_json(fixture.api(&["api", "--json", "public.cg"]));
    let names: Vec<_> = index["functions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["exported", "id", "unused"]);
    assert_eq!(index["records"][0]["identity"], "app.api.Visible");
    assert_eq!(index["variants"][0]["identity"], "app.api.Choice");
    assert_eq!(
        index["functions"][1]["type_parameters"][0]["constraints"],
        serde_json::json!(["Eq", "Show"])
    );
    assert_eq!(index["functions"][1]["return_type"], "T");
    assert_eq!(index["semantics_status"], "checked-ordinary-memory-v1");
    let text = fixture.api(&["api", "public.cg"]);
    assert!(text.status.success());
    let text = String::from_utf8(text.stdout).unwrap();
    assert!(text.contains("pub fn unused<T: Eq>"));
    assert!(!text.contains("private_helper"));
    assert!(!text.contains("Secret"));
    let effects = checked_json(fixture.api(&["memory-effects", "public.cg"]));
    let helper = effects["functions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "private_helper")
        .unwrap();
    assert_eq!(effects["schema_version"], 2);
    assert_eq!(helper["memory_effects_v2"]["may_enter_guard"], true);
    assert_eq!(
        effects["functions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == "main")
            .unwrap()["memory_effects_v2"]["may_enter_guard"],
        true
    );
}

#[test]
fn packages_keep_source_paths_declarations_and_manifest_certificate() {
    let fixture = Fixture::new();
    fixture.write("cgx2.toml", "schema_version = 1\nname = \"api-repair\"\nentry = \"main.cg\"\nsources = [\"main.cg\", \"lib.cg\"]\n");
    fixture.write(
        "lib.cg",
        &SOURCE.replace("fn main()", "fn internal_entry()"),
    );
    fixture.write("main.cg", "module app.main\nimport app.api\nfn main() -> i64 effects(ManagedAlloc):\n    match memory.try_no_gc(budget: 4096):\n        Result.Ok(guard):\n            app.api.exported()\n        Result.Err(error):\n            -1\n");
    let index = checked_json(fixture.api(&["api", "--json", "."]));
    assert_eq!(index["package"]["entry"], "main.cg");
    assert_eq!(
        index["package"]["manifest_sha256"].as_str().unwrap().len(),
        64
    );
    assert_eq!(
        index["package"]["source_hashes"].as_array().unwrap().len(),
        2
    );
    assert_eq!(index["modules"][0]["source"], "lib.cg");
    assert_eq!(
        index["modules"][0]["api"]["records"][0]["identity"],
        "app.api.Visible"
    );
    assert_eq!(
        index["modules"][1]["imports"],
        serde_json::json!(["app.api"])
    );
    assert!(!index.to_string().contains("internal_entry"));
    let text = fixture.api(&["api", "."]);
    assert!(text.status.success());
    assert!(String::from_utf8(text.stdout)
        .unwrap()
        .contains("source lib.cg"));
    let effects = checked_json(fixture.api(&["memory-effects", "."]));
    assert!(effects.to_string().contains("app.api.private_helper"));
}

#[test]
fn checked_errors_keep_spans_and_syntax_only_remains_explicit() {
    let fixture = Fixture::new();
    fixture.write("bad.cg", &format!("{SOURCE}\nfn bad() -> i64:\n    true\n"));
    let output = fixture.api(&["--diagnostic-format=json", "api", "--json", "bad.cg"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let diagnostic: Value = serde_json::from_str(
        String::from_utf8(output.stderr)
            .unwrap()
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap();
    assert!(diagnostic.to_string().contains("bad.cg"));
    assert!(diagnostic["span"].is_object(), "{diagnostic}");
    let syntax = checked_json(fixture.api(&["api", "--syntax-only", "--json", "bad.cg"]));
    assert_eq!(syntax["semantics_status"], "syntax-only");
}
