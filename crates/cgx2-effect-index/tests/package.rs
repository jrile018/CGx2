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
fn public_contract_graph_resolves_same_named_local_helpers_and_preserves_sites() {
    let fixture = Fixture::new(
        "pub fn helper(x: Bool) -> Bool:\n    x\n",
        "fn helper(x: Bool) -> Bool:\n    x\npub fn value(x: Bool) -> Bool effects() caps():\n    requires helper(x)\n    ensures helper(result)\n    x\n",
        "# café 🦀\nfn helper(x: Bool) -> Bool:\n    x\npub fn value(x: Bool) -> Bool effects() caps():\n    requires helper(helper(x))\n    ensures helper(result)\n    ensures helper(result)\n    let helper = false\n    x\npub fn caller(x: Bool) -> Bool effects() caps():\n    demo.middle.value(value(x))\n",
    );
    let checked = load_checked_package(&fixture.0).unwrap();
    let index = index_checked_package(&checked).unwrap();
    for (source, count) in [("src/main.cg", 4), ("src/middle.cg", 2)] {
        let identity = Identity {
            source: source.into(),
            name: "value".into(),
        };
        let value = index.entry(&identity).unwrap();
        let helper = index
            .entry(&Identity {
                source: source.into(),
                name: "helper".into(),
            })
            .unwrap();
        assert!(value.public);
        assert!(value.effects.is_empty());
        assert!(value.capabilities.is_empty());
        assert_eq!(value.calls.len(), count);
        let text = fs::read_to_string(fixture.0.join(source)).unwrap();
        for call in &value.calls {
            assert_eq!(
                call.resolution,
                Resolution::Function(helper.identity.clone())
            );
            assert_eq!(call.declaration.as_ref(), Some(&helper.declaration));
            assert_eq!(call.location.source, PathBuf::from(source));
            assert!(text[call.location.span.start..call.location.span.end].starts_with("helper("));
            assert_eq!(
                index.call(&call.location.source, call.location.span),
                Some(call)
            );
        }
        assert!(value
            .calls
            .windows(2)
            .all(|c| c[0].location.span.start < c[1].location.span.start));
    }
    let caller = index
        .entry(&Identity {
            source: "src/main.cg".into(),
            name: "caller".into(),
        })
        .unwrap();
    assert_eq!(caller.calls.len(), 2);
    assert!(caller.effects.is_empty());
    assert!(caller.capabilities.is_empty());
    assert_eq!(
        caller
            .calls
            .iter()
            .map(|c| &c.resolution)
            .collect::<Vec<_>>(),
        vec![
            &Resolution::Function(Identity {
                source: "src/middle.cg".into(),
                name: "value".into()
            }),
            &Resolution::Function(Identity {
                source: "src/main.cg".into(),
                name: "value".into()
            }),
        ]
    );
    let json = index.to_json().unwrap();
    assert_eq!(json, index_package(&fixture.0).unwrap().to_json().unwrap());
    let json: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["analysis"], "checked_package_effects_v1");
    assert_eq!(json["provenance"].as_array().unwrap().len(), 3);
    let value = json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["identity"]["source"] == "src/main.cg" && e["identity"]["name"] == "value")
        .unwrap();
    assert_eq!(value["calls"].as_array().unwrap().len(), 4);
    assert!(value["calls"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["resolution"] == "function" && c["identity"]["source"] == "src/main.cg"));
}

#[test]
fn contract_graph_does_not_admit_effectful_recursive_or_imported_contract_calls() {
    for main in [
        "fn helper() -> Bool:\n    arg_count() == 0\npub fn value() -> Bool:\n    requires helper()\n    true\n",
        "fn helper() -> Bool:\n    helper()\npub fn value() -> Bool:\n    ensures helper()\n    true\n",
        "pub fn value() -> Bool:\n    requires demo.middle.helper()\n    true\n",
        "pub fn value() -> Bool:\n    requires hidden\n    let hidden = true\n    true\n",
    ] {
        let fixture = Fixture::new("pub fn helper() -> Bool:\n    true\n", "pub fn helper() -> Bool:\n    true\n", main);
        assert!(index_package(&fixture.0).is_err(), "{main}");
    }
}

#[test]
fn checked_contract_scalar_bindings_remain_unknown_without_foreign_capabilities() {
    let fixture = Fixture::new(
        "pub fn helper() -> Bool:\n    true\n",
        "pub fn helper() -> Bool:\n    true\n",
        "fn helper() -> Bool:\n    true\nfn result() -> Bool:\n    true\npub fn parameter(helper: Bool) -> Bool:\n    requires helper()\n    true\npub fn post() -> Bool:\n    requires result()\n    ensures result()\n    true\n",
    );
    // The frozen checker admits these scalar-shadowed calls. The advisory graph
    // must not turn them into resolved helper calls or invent capabilities.
    let index = index_package(&fixture.0).unwrap();
    let parameter = index
        .entry(&Identity {
            source: "src/main.cg".into(),
            name: "parameter".into(),
        })
        .unwrap();
    assert_eq!(parameter.calls.len(), 1);
    assert_eq!(parameter.calls[0].resolution, Resolution::Unknown);
    assert_eq!(
        parameter.effects,
        std::collections::BTreeSet::from(["Unknown".into()])
    );
    assert!(parameter.capabilities.is_empty());
    let post = index
        .entry(&Identity {
            source: "src/main.cg".into(),
            name: "post".into(),
        })
        .unwrap();
    assert_eq!(post.calls.len(), 2);
    assert_eq!(
        post.calls[0].resolution,
        Resolution::Function(Identity {
            source: "src/main.cg".into(),
            name: "result".into()
        })
    );
    assert_eq!(post.calls[1].resolution, Resolution::Unknown);
    assert_eq!(
        post.effects,
        std::collections::BTreeSet::from(["Unknown".into()])
    );
    assert!(post.capabilities.is_empty());
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
