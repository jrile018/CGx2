use cgx2_sema::default_managed::compile_source;

#[test]
fn typed_result_equality_is_independent_of_recovery_representation() {
    for (name, source, failures) in [
        (
            "ok",
            include_str!("../../cgx2-syntax/managed-equality-repair-evidence/ok.cg"),
            0,
        ),
        (
            "err",
            include_str!("../../cgx2-syntax/managed-equality-repair-evidence/err.cg"),
            2,
        ),
        (
            "nested",
            include_str!("../../cgx2-syntax/managed-equality-repair-evidence/nested.cg"),
            0,
        ),
        (
            "nested-err",
            include_str!("../../cgx2-syntax/managed-equality-repair-evidence/nested-err.cg"),
            1,
        ),
    ] {
        let p = compile_source(source).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let execution = p.execute(&[]).unwrap();
        assert_eq!(execution.result.unwrap().to_string(), "42", "{name}");
        assert_eq!(execution.evidence.recovered, failures, "{name}");
        assert_eq!(execution.evidence.baseline_roots, 0);
        assert_eq!(execution.evidence.final_roots, 0);
        assert_eq!(execution.evidence.cleanup_failures, 0);
        assert_eq!(
            execution.evidence.roots_registered,
            execution.evidence.roots_released
        );
        assert_eq!(execution.memory.totals.collection_count, 0);
    }
}

#[test]
fn typed_ref_equality_and_different_nominal_types_still_reject() {
    for source in [
        "fn main() -> Bool effects(ManagedAlloc):\n let a = managed.ref(1)\n a == a\n",
        "record Box<T>:\n    value: T\nfn main() -> Bool effects(ManagedAlloc):\n let a = Box(value: managed.ref(1))\n a == a\n",
        "variant A:\n    Item(i64)\nvariant B:\n    Item(i64)\nfn main() -> Bool:\n A.Item(1) == B.Item(1)\n",
        "fn main() -> Bool effects(ManagedAlloc):\n managed.try_alloc(1) == Result<Bool, AllocationError>.Ok(true)\n",
    ] {
        let errors = compile_source(source).unwrap_err();
        assert!(errors.iter().any(|e| e.code == "CGT039"), "{source}: {errors:?}");
    }
}
