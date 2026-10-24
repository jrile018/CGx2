use cgx2_sema::ordinary_owned::{compile_source, compile_source_v2, Scalar};

const CASES: &[&str] = &[
    include_str!("../../cgx2-syntax/owned-symbol-compatibility-evidence/fixtures/nominal-result.cg"),
    include_str!("../../cgx2-syntax/owned-symbol-compatibility-evidence/fixtures/nominal-error.cg"),
    include_str!("../../cgx2-syntax/owned-symbol-compatibility-evidence/fixtures/user-function.cg"),
    include_str!("../../cgx2-syntax/owned-symbol-compatibility-evidence/fixtures/local-name.cg"),
    include_str!("../../cgx2-syntax/owned-symbol-compatibility-evidence/fixtures/fields-parameters.cg"),
    include_str!("../../cgx2-syntax/owned-symbol-compatibility-evidence/fixtures/nominal-signatures.cg"),
    include_str!("../../cgx2-syntax/owned-symbol-compatibility-evidence/fixtures/nominal-try-owned.cg"),
    include_str!("../../cgx2-syntax/owned-symbol-compatibility-evidence/fixtures/user-owned-call.cg"),
    include_str!("../../cgx2-syntax/owned-symbol-compatibility-evidence/fixtures/function-Result.cg"),
    include_str!("../../cgx2-syntax/owned-symbol-compatibility-evidence/fixtures/function-AllocationError.cg"),
];

#[test]
fn old_symbols_resolve_through_both_authenticated_entries() {
    for source in CASES {
        let old = compile_source(source).unwrap_or_else(|e| panic!("{e:?}\n{source}"));
        let new = compile_source_v2(source).unwrap();
        assert_eq!(old.schema_version(), 1);
        assert_eq!(new.schema_version(), 1);
        assert!(!new.ownership_functions().is_empty());
        assert!(!new.ownership_functions_v2().is_empty());
        old.authenticate().unwrap();
        new.authenticate().unwrap();
        let before = old.execute().unwrap();
        let after = new.execute().unwrap();
        assert_eq!(before.result, Ok(Scalar::I64(42)));
        assert_eq!(before.result, after.result);
        assert_eq!(before.evidence.json(), after.evidence.json());
        assert_eq!(after.evidence.allocations, after.evidence.releases);
        assert_eq!(after.evidence.live_bytes, 0);
    }
}

#[test]
fn genuine_guard_profile_still_resolves_user_symbols() {
    let source = include_str!(
        "../../cgx2-syntax/owned-symbol-compatibility-evidence/fixtures/guard-user-call.cg"
    );
    assert_eq!(compile_source(source).unwrap_err()[0].code, "CGT081");
    let checked = compile_source_v2(source).unwrap();
    assert_eq!(checked.schema_version(), 2);
    let execution = checked.execute().unwrap();
    assert_eq!(execution.result, Ok(Scalar::I64(42)));
    assert_eq!(execution.evidence.guard_entries, 1);
    assert_eq!(execution.evidence.result_ok, 0);
    assert_eq!(execution.evidence.allocations, execution.evidence.releases);
}

#[test]
fn ordinary_names_do_not_enable_additive_expansion_limits() {
    let nested = format!("{}42{}", "1 + (".repeat(34), ")".repeat(34));
    for nominal in ["Result", "AllocationError", "try_owned"] {
        let source = format!("record {nominal}:\n    x: i64\nfn copy(p: {nominal}) -> {nominal}:\n    {nested}\n    p\nfn main() -> i64:\n    let p = owned(copy({nominal}(x: 42)))\n    p.x\n");
        assert_eq!(
            compile_source(&source).unwrap().execute().unwrap().result,
            Ok(Scalar::I64(42))
        );
    }
    let source = format!("record R:\n    x: i64\nfn try_owned(x: i64) -> i64:\n    {nested}\n    x\nfn main() -> i64:\n{}    let p = owned(R(x: try_owned(42)))\n    p.x\n", "    0\n".repeat(4100));
    assert_eq!(
        compile_source(&source).unwrap().execute().unwrap().result,
        Ok(Scalar::I64(42))
    );
}

#[test]
fn unresolved_intrinsic_and_builtin_error_remain_version_two() {
    let source = include_str!("../../cgx2-syntax/owned-result-evidence/fixtures/recover.cg");
    assert_eq!(compile_source(source).unwrap_err()[0].code, "CGT081");
    let checked = compile_source_v2(source).unwrap();
    assert_eq!(checked.schema_version(), 2);
    assert_eq!(checked.execute().unwrap().result, Ok(Scalar::I64(42)));
    let ambiguous = format!("record AllocationError:\n    x: i64\n{source}");
    assert_eq!(compile_source_v2(&ambiguous).unwrap_err()[0].code, "CGT081");
}

#[test]
fn historical_shadow_parameter_is_valid_schema_one() {
    let source = include_str!("../../cgx2-syntax/owned-result-evidence/fixtures/shadow-builtin.cg");
    for execution in [
        compile_source(source).unwrap().execute().unwrap().result,
        compile_source_v2(source).unwrap().execute().unwrap().result,
    ] {
        assert_eq!(execution, Ok(Scalar::I64(0)));
    }
}

#[test]
fn nominal_result_and_ordinary_bindings_coexist_with_real_intrinsic() {
    let source = "record Result:\n    x: i64\nfn allocate(try_owned: Result) -> Result<owned<Result>, AllocationError>:\n    try_owned(try_owned)\nfn main() -> i64:\n    let Result = Result(x: 42)\n    match allocate(Result):\n        Result.Ok(AllocationError):\n            return AllocationError.x\n        Result.Err(try_owned):\n            return 0\n";
    assert_eq!(compile_source(source).unwrap_err()[0].code, "CGT081");
    let execution = compile_source_v2(source).unwrap().execute().unwrap();
    assert_eq!(execution.result, Ok(Scalar::I64(42)));
    assert_eq!(execution.evidence.result_ok, 1);
    assert_eq!(execution.evidence.allocations, execution.evidence.releases);
}
