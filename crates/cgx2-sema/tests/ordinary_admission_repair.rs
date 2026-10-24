use cgx2_runtime::default_managed::Snapshot;
use cgx2_sema::ordinary_memory::{compile_ast, compile_source, compile_source_bundle_v1};

fn region(declaration: &str, body: &str) -> String {
    format!("fn main() -> i64 {declaration}:\n    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n{body}\n        Result.Err(error):\n            -1\n")
}

fn integer(source: &str, expected: i64) {
    let program = compile_source(source).unwrap();
    let execution = program.execute(&[]).unwrap();
    assert_eq!(execution.result.unwrap(), Snapshot::Integer(expected));
    let evidence = execution.evidence.unwrap();
    assert_eq!(evidence.baseline_roots, evidence.final_roots);
    assert_eq!(evidence.cleanup_failures, 0);
}

#[test]
fn shadowed_managed_templates_cannot_erase_region_requirements() {
    let shadow = "module managed\nfn ref(value: i64) -> i64 effects() caps():\n    value\nfn collect() -> Unit effects() caps():\n    return\n";
    let source = format!("{shadow}{}", region("effects() caps()", "            42"));
    let errors = compile_source(&source).unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.code == "CGT012" && e.message.contains("ManagedAlloc")));
    integer(
        &format!(
            "{shadow}{}",
            region("effects(ManagedAlloc) caps()", "            42")
        ),
        42,
    );
}

#[test]
fn imported_shadows_preserve_intrinsic_effect_and_capability_closure() {
    let library = "module managed\npub fn ref(value: i64) -> i64 effects() caps():\n    value\npub fn collect() -> Unit effects() caps():\n    return\n";
    for (declaration, code, requirement) in [
        ("effects(ManagedAlloc) caps()", "CGT012", "Collect"),
        (
            "effects(ManagedAlloc, Collect) caps()",
            "CGT013",
            "MemoryCollect",
        ),
    ] {
        let app = format!("module app\nimport managed\nfn hidden() -> i64:\n    memory.collect(memory.default_region())\n    42\nfn main() -> i64 {declaration}:\n    hidden()\n");
        let errors = compile_source_bundle_v1(
            &[("managed".into(), library.into()), ("app".into(), app)],
            "app",
        )
        .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.code == code && e.message.contains(requirement)),
            "{errors:?}"
        );
    }
    let app = "module app\nimport managed\nfn hidden() -> i64:\n    memory.collect(memory.default_region())\n    42\nfn main() -> i64 effects(Collect) caps(MemoryCollect):\n    hidden()\n";
    let program = compile_source_bundle_v1(
        &[
            ("managed".into(), library.into()),
            ("app".into(), app.into()),
        ],
        "app",
    )
    .unwrap();
    assert_eq!(
        program.execute(&[]).unwrap().result.unwrap(),
        Snapshot::Integer(42)
    );
    let main = &program.typed().effects["main"];
    assert!(main.effects.contains("Collect"));
    assert!(main.capabilities.contains("MemoryCollect"));
}

#[test]
fn user_managed_names_keep_their_resolved_effects() {
    let source = "module managed\nfn ref(value: i64) -> i64 effects() caps():\n    value + 1\nfn collect() -> i64 effects() caps():\n    1\n";
    integer(
        &format!(
            "{source}{}",
            region(
                "effects(ManagedAlloc) caps()",
                "            managed.ref(40) + managed.collect()"
            )
        ),
        42,
    );
    let bad = format!(
        "module managed\nfn ref(value: i64) -> i64:\n    arg_count()\n{}",
        region("effects(ManagedAlloc) caps()", "            managed.ref(0)")
    );
    let errors = compile_source(&bad).unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.code == "CGT012" && e.message.contains("ReadArgs")));
    assert!(errors
        .iter()
        .any(|e| e.code == "CGT013" && e.message.contains("ArgsRead")));
}

#[test]
fn all_allocating_markers_require_managed_alloc_through_callers() {
    for body in [
        "match memory.try_no_gc(budget: 4096):\n        Result.Ok(g):\n            42\n        Result.Err(e):\n            -1",
        "match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n            42\n        Result.Err(e):\n            -1",
        "match memory.try_in(memory.default_region(), 42):\n        Result.Ok(v):\n            v\n        Result.Err(e):\n            -1",
        "match memory.copy_to(memory.default_region(), 42):\n        Result.Ok(v):\n            v\n        Result.Err(e):\n            -1",
    ] {
        let source = format!("module managed\nfn ref(x: i64) -> i64 effects():\n    x\nfn hidden() -> i64:\n    {body}\nfn main() -> i64 effects() caps():\n    hidden()\n");
        let errors = compile_source(&source).unwrap_err();
        assert!(errors.iter().any(|e| e.code == "CGT012" && e.message.contains("ManagedAlloc")), "{errors:?}");
        integer(&source.replace("fn main() -> i64 effects()", "fn main() -> i64 effects(ManagedAlloc)"), 42);
    }
    let source = "module managed\nfn ref(x: i64) -> i64 effects():\n    x\nfn main() -> i64 effects() caps():\n    match memory.try_session():\n        Result.Ok(s):\n            42\n        Result.Err(e):\n            -1\n";
    assert!(compile_source(source)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "CGT012" && e.message.contains("ManagedAlloc")));
    integer(
        &source.replace(
            "fn main() -> i64 effects()",
            "fn main() -> i64 effects(ManagedAlloc)",
        ),
        42,
    );
}

#[test]
fn imported_shadow_ref_cannot_hide_any_allocating_marker() {
    let library = "module managed\npub fn ref(x: i64) -> i64 effects() caps():\n    x\n";
    for operation in ["region", "guard", "try_in", "copy_to"] {
        let body = match operation {
            "region" => "memory.try_region(mode: memory.Mode.Manual, limit: 4096)",
            "guard" => "memory.try_no_gc(budget: 4096)",
            "try_in" => "memory.try_in(memory.default_region(), 42)",
            _ => "memory.copy_to(memory.default_region(), 42)",
        };
        let app = format!("module app\nimport managed\nfn hidden() -> i64:\n    match {body}:\n        Result.Ok(value):\n            42\n        Result.Err(error):\n            -1\nfn main() -> i64 effects() caps():\n    hidden()\n");
        let sources = vec![
            ("managed".into(), library.into()),
            ("app".into(), app.clone()),
        ];
        let errors = compile_source_bundle_v1(&sources, "app").unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "CGT012" && e.message.contains("ManagedAlloc")),
            "{operation}: {errors:?}"
        );
        let sources = vec![
            ("managed".into(), library.into()),
            (
                "app".into(),
                app.replace(
                    "fn main() -> i64 effects()",
                    "fn main() -> i64 effects(ManagedAlloc)",
                ),
            ),
        ];
        let program = compile_source_bundle_v1(&sources, "app").unwrap();
        assert_eq!(
            program.execute(&[]).unwrap().result.unwrap(),
            Snapshot::Integer(42)
        );
    }
}

#[test]
fn legacy_budgets_and_assignment_tags_are_not_expression_indices() {
    for budget in [1024_u64, 1_048_576, 4_294_967_296] {
        let source = region("effects(ManagedAlloc, Mutate)", &format!("            let mut answer = 40\n            no_gc(budget: {budget}):\n                no_gc(budget: {budget}):\n                    answer += 2\n                    return answer\n            0"));
        integer(&source, 42);
        let program = compile_source(&source).unwrap();
        let main = &program.effects()[program.runtime().program.entry];
        assert!(!main.may_explicitly_collect);
        assert!(!main.may_transfer_graph);
    }
}

#[test]
fn malformed_ast_bounds_reject_without_panicking() {
    for invalid in [
        cgx2_syntax::ExprKind::Integer("-1".into()),
        cgx2_syntax::ExprKind::Integer("18446744073709551616".into()),
        cgx2_syntax::ExprKind::Boolean(true),
    ] {
        let source = region(
            "effects(ManagedAlloc)",
            "            no_gc(budget: 1024):\n                return 42\n            0",
        );
        let mut module = cgx2_syntax::parse_module(&source).unwrap();
        let cgx2_syntax::StmtKind::Match { arms, .. } = &mut module.functions[0].body[0].kind
        else {
            panic!()
        };
        let cgx2_syntax::StmtKind::NoGc { budget, .. } = &mut arms[0].body[0].kind else {
            panic!()
        };
        budget.kind = invalid;
        assert!(compile_ast(&module).is_err());
    }
}

#[test]
fn user_borrow_reproduction_retains_call_semantics() {
    let source = include_str!(
        "../../../plans/release-completion/s2b-ordinary-memory-review/repros/user-borrow.cg"
    );
    integer(source, 42);
}

#[test]
fn imported_borrow_and_real_region_borrow_both_work() {
    let library = "module util\npub fn borrow(value: i64) -> i64 effects():\n    value + 1\n";
    let source = format!("module app\nimport util\n{}", region("effects(ManagedAlloc)", "            match memory.try_in(borrow(r), [util.borrow(41)]):\n                Result.Ok(xs):\n                    match xs[0]:\n                        Option.Some(n):\n                            n\n                        Option.None:\n                            0\n                Result.Err(e):\n                    -2"));
    let program = compile_source_bundle_v1(
        &[("util".into(), library.into()), ("app".into(), source)],
        "app",
    )
    .unwrap();
    assert_eq!(
        program.execute(&[]).unwrap().result.unwrap(),
        Snapshot::Integer(42)
    );
}

#[test]
fn user_err_escape_is_rejected_at_exact_utf8_return_span() {
    let source = include_str!("../../../plans/release-completion/s2b-ordinary-memory-review/repros/user-err-region-escape.cg").replace("secret", "秘密");
    let errors = compile_source(&source).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, "CGT095");
    let start = source.find("return text").unwrap();
    assert_eq!(
        errors[0].span,
        cgx2_syntax::Span {
            start,
            end: start + "return text\n".len()
        }
    );
}

#[test]
fn user_err_whole_graph_survives_fields_and_builtin_results() {
    for (declarations, payload, pattern, returned) in [
        (
            "variant Message:\n    Err(String)\n",
            "Message.Err(\"秘密\")",
            "Message.Err(text)",
            "text",
        ),
        (
            "record Box:\n    text: String\nvariant Message:\n    Err(Box)\n",
            "Message.Err(Box(text: \"秘密\"))",
            "Message.Err(box)",
            "box.text",
        ),
        (
            "",
            "Result<i64, String>.Err(\"秘密\")",
            "Result.Err(text)",
            "text",
        ),
    ] {
        let source = format!("{declarations}fn leak() -> String effects(ManagedAlloc):\n    match memory.try_region(mode: memory.Mode.Manual, limit: 4096):\n        Result.Ok(r):\n            match memory.try_in(borrow(r), {payload}):\n                Result.Ok(value):\n                    match value:\n                        {pattern}:\n                            return {returned}\n{}                Result.Err(error):\n                    return \"allocation failed\"\n        Result.Err(error):\n            return \"region failed\"\nfn main() -> String effects(ManagedAlloc):\n    leak()\n", if declarations.is_empty() { "                        Result.Ok(n):\n                            return \"ok\"\n" } else { "" });
        let errors = compile_source(&source).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.code == "CGT095" && e.message.contains("region")),
            "{errors:?}"
        );
    }
}

#[test]
fn user_err_inside_region_and_builtin_scalar_error_are_valid() {
    let source = "variant Message:\n    Err(String)\n".to_owned() + &region("effects(ManagedAlloc)", "            match memory.try_in(borrow(r), Message.Err(\"secret\")):\n                Result.Ok(value):\n                    match value:\n                        Message.Err(text):\n                            length(text)\n                Result.Err(error):\n                    0");
    integer(&source, 6);
    integer(&region("effects(ManagedAlloc)", "            match memory.try_in(borrow(r), [42]):\n                Result.Ok(xs):\n                    match xs[0]:\n                        Option.Some(n):\n                            n\n                        Option.None:\n                            0\n                Result.Err(error):\n                    match error:\n                        AllocationError.ShapeLimit:\n                            0\n                        AllocationError.BudgetExceeded:\n                            0\n                        AllocationError.RegionLimit:\n                            0\n                        AllocationError.HostAllocation:\n                            0\n                        AllocationError.InvalidRegion:\n                            0"), 42);
}
