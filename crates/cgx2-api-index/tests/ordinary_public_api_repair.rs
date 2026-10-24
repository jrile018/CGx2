use cgx2_api_index::{build_memory_index_v2, build_public_memory_index_v1};
use cgx2_sema::ordinary_memory::{compile_ast, compile_source, compile_source_bundle_v1};

const SOURCE: &str =
    include_str!("../../cgx2-syntax/ordinary-api-repair-evidence/fixtures/public.cg");

#[test]
fn original_public_declarations_are_separate_from_private_summaries() {
    let program = compile_source(SOURCE).unwrap();
    let public = build_public_memory_index_v1(&program).unwrap();
    let json = public.render_json();
    let text = public.render_text();
    for name in [
        "Visible", "Choice", "id", "unused", "exported", "Eq", "Show",
    ] {
        assert!(json.contains(name), "{name}: {json}");
        assert!(text.contains(name), "{name}: {text}");
    }
    for name in [
        "private_helper",
        "Secret",
        "hidden",
        "main",
        "id<i64>",
        "$cgx2$",
    ] {
        assert!(!json.contains(name), "{name}: {json}");
        assert!(!text.contains(name), "{name}: {text}");
    }
    assert!(json.contains("\"semantics_status\":\"checked-ordinary-memory-v1\""));
    assert!(!json.contains("unchecked"));
    assert!(json.contains("\"absent_summary\":\"unknown\""));
    assert!(json.contains("\"return_type\":\"T\""));
    let effects = build_memory_index_v2(&program).unwrap();
    assert!(effects.render_json().contains("private_helper"));
    assert!(effects.render_json().contains("\"may_enter_guard\":true"));
    assert!(effects.render_json().contains("\"schema_version\":2"));
}

#[test]
fn bundles_keep_original_modules_paths_and_qualified_type_identities() {
    let sources = vec![
        ("src/library.cg".into(), SOURCE.replace("fn main()", "fn internal_entry()")),
        ("src/main.cg".into(), "module app.main\nimport app.api\nfn main() -> i64 effects(ManagedAlloc):\n    match memory.try_no_gc(budget: 4096):\n        Result.Ok(guard):\n            app.api.exported()\n        Result.Err(error):\n            -1\n".into()),
    ];
    let program = compile_source_bundle_v1(&sources, "src/main.cg").unwrap();
    let public = build_public_memory_index_v1(&program).unwrap();
    let json = public.render_json();
    for expected in [
        "src/library.cg",
        "src/main.cg",
        "app.api.Visible",
        "app.api.Choice",
        "source-bundle-v1",
    ] {
        assert!(json.contains(expected), "{expected}: {json}");
    }
    assert!(!json.contains("internal_entry"));
    assert!(!json.contains("private_helper"));
    assert!(public.render_text().contains("source src/library.cg"));
}

#[test]
fn ast_only_is_rejected_by_public_view_but_effects_remain_supported() {
    let program = compile_ast(&cgx2_syntax::parse_module(SOURCE).unwrap()).unwrap();
    assert!(build_public_memory_index_v1(&program).is_err());
    assert!(build_memory_index_v2(&program).is_ok());
}

#[test]
fn invalid_private_and_uninstantiated_generic_bodies_cannot_produce_checked_api() {
    for extra in [
        "\nfn invalid() -> i64:\n    true\n",
        "\nfn invalid<T>(value: T) -> i64:\n    true\n",
    ] {
        let errors = compile_source(&format!("{SOURCE}{extra}")).unwrap_err();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|d| d.span.end > d.span.start));
    }
}

#[test]
fn region_signatures_are_not_erased() {
    let source = include_str!("../../cgx2-syntax/ordinary-memory-evidence/fixtures/region.cg")
        .replacen("fn build", "pub fn build", 1);
    let program = compile_source(&source).unwrap();
    let index = build_public_memory_index_v1(&program).unwrap();
    let text = index.render_text();
    assert!(text.contains("R: memory.RegionName"), "{text}");
    assert!(text.contains("memory.Value<R, List<i64>>"), "{text}");
}
