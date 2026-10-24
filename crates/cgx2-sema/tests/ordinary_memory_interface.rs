use cgx2_runtime::default_managed::Snapshot;
use cgx2_sema::ordinary_memory::{
    compile_ast, compile_source, compile_source_bundle_v1, MemoryEffectsV2, MemoryProgramV1,
};

fn effect<'a>(p: &'a MemoryProgramV1, name: &str) -> &'a MemoryEffectsV2 {
    let model = &p.runtime().program;
    let i = model
        .functions
        .iter()
        .position(|f| model.strings[f.name] == name)
        .unwrap();
    &p.effects()[i]
}

#[test]
fn effects_distinguish_unused_direct_transitive_and_recursive_bodies() {
    let p = compile_source("fn scalar() -> i64:\n    42\nfn region() -> i64 effects(ManagedAlloc):\n    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n            1\n        Result.Err(error):\n            -1\nfn indirect() -> i64 effects(ManagedAlloc):\n    region()\nfn recurse(n: i64) -> i64 effects(ManagedAlloc):\n    if n == 0:\n        return indirect()\n    else:\n        return recurse(n - 1)\nfn main() -> i64:\n    scalar()\n").unwrap();
    for name in ["scalar", "main"] {
        let e = effect(&p, name);
        assert!(
            !e.may_enter_region
                && !e.may_create_region
                && !e.may_enter_guard
                && !e.may_enter_session
                && !e.may_transfer_graph
                && !e.may_allocate_owned,
            "{name}: {e:?}"
        );
    }
    for name in ["region", "indirect", "recurse"] {
        let e = effect(&p, name);
        assert!(
            e.may_enter_region && e.may_create_region && e.may_allocate_root_bookkeeping,
            "{name}: {e:?}"
        );
        assert!(!e.may_enter_guard && !e.may_enter_session && !e.may_transfer_graph);
    }
    p.authenticate().unwrap();
    assert_eq!(
        p.execute(&[]).unwrap().result.unwrap(),
        Snapshot::Integer(42)
    );
}

#[test]
fn ast_only_authentication_does_not_claim_original_source() {
    let source = "# café🙂\nfn main() -> i64 effects(ManagedAlloc):\n    match memory.try_no_gc(budget: 4096):\n        Result.Ok(guard):\n            42\n        Result.Err(error):\n            -1\n";
    let source_program = compile_source(source).unwrap();
    let ast = compile_ast(&cgx2_syntax::parse_module(source).unwrap()).unwrap();
    assert_eq!(ast.provenance_v1().kind(), "ast-only-v1");
    assert!(ast.source_sha256().is_empty());
    assert_eq!(source_program.provenance_v1().kind(), "source-v1");
    assert_ne!(ast.analysis_sha256(), source_program.analysis_sha256());
    assert_ne!(
        ast.provenance_v1().identity_sha256(),
        source_program.provenance_v1().identity_sha256()
    );
    assert_eq!(
        ast.execute(&[]).unwrap().result.unwrap(),
        source_program.execute(&[]).unwrap().result.unwrap()
    );
    assert!(effect(&ast, "main").may_enter_guard);
}

fn sources(lib: &str, main: &str) -> Vec<(String, String)> {
    vec![
        ("lib.cg".into(), lib.into()),
        ("main.cg".into(), main.into()),
    ]
}

#[test]
fn source_bundle_checks_generic_region_helpers_and_retains_input_identity() {
    let sources = sources("module app.lib\npub fn build<R: memory.RegionName>(r: shared<memory.Region<R>>) -> Result<memory.Value<R, List<i64>>, AllocationError> effects(ManagedAlloc):\n    memory.try_in(r, [20, 22])\n", "module app.main\nimport app.lib\nfn main() -> i64 effects(ManagedAlloc):\n    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n            match app.lib.build(borrow(r)):\n                Result.Ok(xs):\n                    length(xs)\n                Result.Err(error):\n                    -2\n        Result.Err(error):\n            -1\n");
    let p = compile_source_bundle_v1(&sources, "main.cg").unwrap();
    assert_eq!(p.provenance_v1().kind(), "source-bundle-v1");
    assert!(p.source_sha256().is_empty());
    p.authenticate().unwrap();
    assert_eq!(
        p.execute(&[]).unwrap().result.unwrap(),
        Snapshot::Integer(2)
    );
    let mut edited = sources.clone();
    edited[0].1 = edited[0].1.replace("20", "21");
    let changed = compile_source_bundle_v1(&edited, "main.cg").unwrap();
    assert_ne!(
        p.provenance_v1().identity_sha256(),
        changed.provenance_v1().identity_sha256()
    );
}

#[test]
fn bundle_rejects_private_calls_invalid_unused_bodies_and_missing_imports() {
    let main = "module app.main\nimport app.lib\nfn main() -> i64:\n    app.lib.hidden()\n";
    for lib in [
        "module app.lib\nfn hidden() -> i64:\n    42\n",
        "module app.lib\npub fn hidden() -> i64:\n    42\nfn unused() -> i64:\n    true\n",
    ] {
        assert!(compile_source_bundle_v1(&sources(lib, main), "main.cg").is_err());
    }
    let lib = "module app.lib\npub fn hidden() -> i64:\n    42\n";
    assert!(compile_source_bundle_v1(
        &sources(lib, &main.replace("import app.lib\n", "")),
        "main.cg"
    )
    .is_err());
}

#[test]
fn imported_user_memory_function_retains_precedence_inside_memory_program() {
    let sources = sources("module memory\npub fn try_region() -> i64:\n    42\n", "module app.main\nimport memory\nfn main() -> i64 effects(ManagedAlloc):\n    match memory.try_no_gc(budget: 4096):\n        Result.Ok(guard):\n            memory.try_region()\n        Result.Err(error):\n            -1\n");
    let p = compile_source_bundle_v1(&sources, "main.cg").unwrap();
    assert_eq!(
        p.execute(&[]).unwrap().result.unwrap(),
        Snapshot::Integer(42)
    );
    assert!(!effect(&p, "main").may_create_region);
    assert!(effect(&p, "main").may_enter_guard);
}

#[test]
fn fallback_rejects_transitive_resource_operations_in_source_bundles() {
    let source = sources("module app.lib\npub fn bad() -> i64:\n    let r = memory.default_region()\n    1\n", "module app.main\nimport app.lib\nfn main() -> i64 effects(ManagedAlloc):\n    match memory.try_session():\n        Result.Ok(session):\n            42\n        Result.Err(error):\n            app.lib.bad()\n");
    let errors = compile_source_bundle_v1(&source, "main.cg").unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.code == "CGT095" && d.message.contains("fallback")),
        "{errors:?}"
    );
}

#[test]
fn ast_only_cannot_spell_internal_callees_or_compiler_region_binders() {
    let source = "fn main() -> i64 effects(ManagedAlloc):\n    match memory.try_no_gc(budget: 4096):\n        Result.Ok(guard):\n            42\n        Result.Err(error):\n            -1\n";
    let mut ast = cgx2_syntax::parse_module(source).unwrap();
    ast.functions[0].name = "$cgx2$ordinary_memory$stub$0".into();
    let error = compile_ast(&ast).unwrap_err();
    assert!(error[0].message.contains("compiler-internal identifiers"));
    let source = "fn helper<R: memory.RegionName>(r: shared<memory.Region<R>>) -> i64:\n    42\nfn main() -> i64 effects(ManagedAlloc):\n    match memory.try_no_gc(budget: 4096):\n        Result.Ok(guard):\n            42\n        Result.Err(error):\n            -1\n";
    let mut ast = cgx2_syntax::parse_module(source).unwrap();
    ast.functions[0].type_parameters[0].name = "memory.Default".into();
    assert!(compile_ast(&ast).unwrap_err()[0]
        .message
        .contains("compiler region names cannot be manufactured"));
}

#[test]
fn ast_only_rejects_colliding_expression_type_spans() {
    let mut ast = cgx2_syntax::parse_module("fn main() -> i64 effects(ManagedAlloc):\n    match memory.try_no_gc(budget: 4096):\n        Result.Ok(guard):\n            42\n        Result.Err(error):\n            -1\n").unwrap();
    let cgx2_syntax::StmtKind::Match { arms, .. } = &mut ast.functions[0].body[0].kind else {
        panic!()
    };
    let cgx2_syntax::StmtKind::Expr(e) = &arms[0].body[0].kind else {
        panic!()
    };
    let span = e.span;
    let cgx2_syntax::StmtKind::Expr(e) = &mut arms[1].body[0].kind else {
        panic!()
    };
    e.span = span;
    let error = compile_ast(&ast).unwrap_err();
    assert_eq!(error[0].code, "CGT095");
    assert!(error[0].message.contains("distinct spans"));
}
