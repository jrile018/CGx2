use cgx2_sema::default_managed::{
    compile_ast, compile_module, compile_source, compile_source_bundle_v1,
};
use cgx2_syntax::parse_module;
use sha2::{Digest, Sha256};

const SOURCE: &str = "fn main() -> i64:\n    let xs = [40, 2]\n    get(xs, 0) + get(xs, 1)\n";
fn bundle(lib: &str, main: &str) -> Vec<(String, String)> {
    vec![
        ("lib.cg".into(), lib.into()),
        ("main.cg".into(), main.into()),
    ]
}
#[test]
fn source_constructor_rejects_invalid_unrelated_or_stale_source_and_spans() {
    let ast = parse_module(SOURCE).unwrap();
    for text in [
        "this is not CGx2 source",
        "fn main() -> i64:\n    41\n",
        "# different bytes\n",
    ] {
        assert!(compile_module(&ast, text).is_err(), "{text}");
    }
    let mut changed = ast.clone();
    changed.functions[0].span.start += 1;
    assert_eq!(
        compile_module(&changed, SOURCE).unwrap_err()[0].code,
        "CGT090"
    );
    let other = SOURCE.replace("40", "41");
    assert!(compile_module(&ast, &other).is_err());
    assert!(compile_module(&parse_module(&other).unwrap(), SOURCE).is_err());
}
#[test]
fn source_digest_is_original_utf8_and_ast_only_digest_is_unavailable() {
    let source = "# café🙂\nfn main() -> String:\n    \"héllo🙂\"\n";
    let p = compile_source(source).unwrap();
    assert_eq!(
        p.source_sha256(),
        format!("{:x}", Sha256::digest(source.as_bytes()))
    );
    assert_eq!(p.provenance_v1().kind(), "source-v1");
    let ast = compile_ast(&parse_module(source).unwrap()).unwrap();
    assert_eq!(ast.source_sha256(), "");
    assert_eq!(ast.provenance_v1().kind(), "ast-only-v1");
    assert_ne!(p.analysis_sha256(), ast.analysis_sha256());
    assert_eq!(
        p.execute(&[]).unwrap().result.unwrap(),
        ast.execute(&[]).unwrap().result.unwrap()
    );
    assert!(ast.rust_data().is_ok());
}
#[test]
fn bundle_reconstructs_callees_and_both_effect_dimensions() {
    let lib = "module app.lib\npub fn values() -> List<i64> effects():\n    [40, 2]\n";
    let main = "module app.main\nimport app.lib\nfn main() -> i64 effects():\n    length(app.lib.values())\n";
    let sources = bundle(lib, main);
    let p = compile_source_bundle_v1(&sources, "main.cg").unwrap();
    assert_eq!(p.provenance_v1().kind(), "source-bundle-v1");
    assert!(p.source_sha256().is_empty());
    assert_eq!(p.execute(&[]).unwrap().result.unwrap().to_string(), "2");
    let main_fn = &p.program().functions[p.program().entry];
    assert!(main_fn.behavioral_effects.is_empty());
    assert!(main_fn.effects.may_allocate_managed);
    assert!(main_fn.effects.may_allocate_root_bookkeeping);
    let changed = bundle(&lib.replace("[40, 2]", "[40, 2, 3]"), main);
    let q = compile_source_bundle_v1(&changed, "main.cg").unwrap();
    assert_ne!(p.analysis_sha256(), q.analysis_sha256());
    assert_ne!(
        p.provenance_v1().identity_sha256(),
        q.provenance_v1().identity_sha256()
    );
    assert_eq!(q.execute(&[]).unwrap().result.unwrap().to_string(), "3");
    assert!(compile_module(
        &parse_module(main).unwrap(),
        &sources
            .iter()
            .map(|s| s.1.clone())
            .collect::<Vec<_>>()
            .join("\n")
    )
    .is_err());
}
#[test]
fn bundle_cannot_bypass_import_visibility_effects_or_guard_collection() {
    let main = "module app.main\nimport app.lib\nfn main() -> i64 effects():\n    app.lib.work()\n";
    let private = "module app.lib\nfn work() -> i64 effects():\n    42\n";
    assert!(compile_source_bundle_v1(&bundle(private, main), "main.cg").is_err());
    let hidden = "module app.lib\npub fn work() -> i64 effects(ManagedAlloc):\n    let r = managed.ref(42)\n    42\n";
    assert!(compile_source_bundle_v1(&bundle(hidden, main), "main.cg").is_err());
    let collect = "module app.lib\npub fn work() -> i64 effects(Collect) caps(MemoryCollect):\n    managed.collect()\n    42\n";
    let guarded = "module app.main\nimport app.lib\nfn main() -> i64 effects(Collect) caps(MemoryCollect):\n    no_gc(budget: 1024):\n        return app.lib.work()\n    0\n";
    assert!(compile_source_bundle_v1(&bundle(collect, guarded), "main.cg").is_err());
    assert!(compile_source_bundle_v1(
        &bundle(private, &main.replace("import app.lib\n", "")),
        "main.cg"
    )
    .is_err());
    assert!(compile_source_bundle_v1(&[("main.cg".into(), main.into())], "main.cg").is_err());
    assert!(compile_source_bundle_v1(&bundle(private, main), "missing.cg").is_err());
}
#[test]
fn bundles_reject_cycles_duplicate_identities_and_ambiguous_sources() {
    for sources in [
        bundle(
            "module app.lib\nimport app.main\n",
            "module app.main\nimport app.lib\n",
        ),
        bundle("module same\n", "module same\n"),
        bundle("42\n", "43\n"),
        vec![("same".into(), "1\n".into()), ("same".into(), "2\n".into())],
    ] {
        assert!(compile_source_bundle_v1(&sources, "main.cg").is_err());
    }
}
