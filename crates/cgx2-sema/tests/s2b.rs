use cgx2_sema::{
    check_module, check_module_with_imports, collect_public_signatures_with_imports,
    ImportedSignatures, Type,
};
use cgx2_syntax::parse_module;
fn checked(s: &str) -> cgx2_sema::TypedModule {
    check_module(&parse_module(s).unwrap()).unwrap()
}
fn rejects(s: &str, code: &str) {
    let errors = check_module(&parse_module(s).unwrap()).unwrap_err();
    assert!(errors.iter().any(|e| e.code == code), "{errors:?}");
    for e in errors {
        assert!(e.span.start <= e.span.end && e.span.end <= s.len());
    }
}
#[test]
fn symbolic_constraints_govern_bodies_not_only_observed_instantiations() {
    rejects("fn equal<T>(a: T, b: T) -> Bool:\n    a == b\n", "CGT039");
    rejects(
        "fn show_it<T: Eq>(a: T) -> String:\n    show(a)\n",
        "CGT039",
    );
    rejects("fn add<T: Eq + Show>(a: T) -> T:\n    a + a\n", "CGT003");
    rejects("fn f<T: Invented>(x: T) -> T:\n    x\n", "CGT039");
    rejects("fn f<T: Eq + Eq>(x: T) -> T:\n    x\n", "CGT039");
    rejects(
        "fn eq<T: Eq>(a: T) -> Bool:\n    a == a\nfn bad<U>(x: U) -> Bool:\n    eq(x)\n",
        "CGT039",
    );
    checked("fn eq<T: Eq + Show>(a: T) -> String:\n    let same = a == a\n    show(a)\nfn main() -> String:\n    eq([1, 2])\n");
}
#[test]
fn binders_signatures_and_instantiations_keep_canonical_identity() {
    let typed = checked(
        "module demo\nfn id<T>(x: T) -> T:\n    x\nfn main() -> String:\n    id<String>(\"yes\")\n",
    );
    assert!(
        matches!(&typed.functions["id"].parameters[0],Type::Parameter{name,..} if name=="demo.id::T")
    );
    assert_eq!(
        typed.calls.values().next().unwrap().arguments,
        vec![Type::String]
    );
}
#[test]
fn inference_arity_nominal_arguments_and_ambiguity() {
    checked("record Box<T>:\n    value: T\nfn id<T>(x: T) -> T:\n    x\nfn main() -> Box<List<i64>>:\n    id(Box(value: [1]))\n");
    checked(
        "fn make<T>() -> Option<T>:\n    Option<T>.None()\nfn main() -> Option<i64>:\n    make()\n",
    );
    rejects("fn make<T>() -> Option<T>:\n    Option<T>.None()\nfn main() -> Unit:\n    let x = make()\n","CGT040");
    rejects("record Box<T>:\n    value: T\nfn main() -> Unit:\n    let x: Box<i64, Bool> = Box(value: 1)\n","CGT031");
    assert_eq!(
        parse_module("fn f<T, T>(x: T) -> T:\n    x\n").unwrap_err()[0].code,
        "CGS113"
    );
    rejects("fn f<T>(x: U) -> T:\n    x\n", "CGT002");
    rejects(
        "record Box<T>:\n    value: T\nfn main() -> Box<Bool>:\n    Box<i64>(value: 1)\n",
        "CGT003",
    );
    rejects(
        "fn f<T>(a: T, b: T) -> T:\n    a\nfn main() -> i64:\n    f(1, true)\n",
        "CGT003",
    );
    rejects("fn main() -> Unit:\n    let xs = []\n", "CGT040");
    rejects("fn main() -> Unit:\n    let xs = [1, true]\n", "CGT003");
}
#[test]
fn expansive_recursion_and_recursive_shapes_reject_before_output() {
    rejects("record Loop<T>:\n    next: Loop<T>\n", "CGT030");
    rejects(
        "fn expand<T>(x: T) -> i64:\n    expand<List<T>>([x])\nfn main() -> i64:\n    expand(1)\n",
        "CGT030",
    );
    rejects("record Pair<A, B>:\n    a: A\n    b: B\nfn expand<T>(x: T) -> i64:\n    expand<Pair<T, T>>(Pair(a: x, b: x))\nfn main() -> i64:\n    expand(1)\n","CGT030");
}
#[test]
fn changed_generic_variant_declarations_invalidate_checked_effects() {
    let source=parse_module("module dep\npub variant Choice<T>:\n    Value(T)\npub fn id<T: Eq>(x: T) -> T effects() caps():\n    x\n").unwrap();
    let mut imports = ImportedSignatures::default();
    imports.modules.insert("dep".into());
    let mut variant = source.variants[0].clone();
    variant.name = "dep.Choice".into();
    imports
        .public_variants
        .insert(variant.name.clone(), variant);
    for (n, s) in collect_public_signatures_with_imports(&source, &imports).unwrap() {
        imports.public_functions.insert(format!("dep.{n}"), s);
    }
    imports.check_and_record_module(&source).unwrap();
    let caller =
        parse_module("import dep\nfn use_it() -> i64 effects() caps():\n    dep.id<i64>(1)\n")
            .unwrap();
    check_module_with_imports(&caller, &imports).unwrap();
    imports.public_variants.get_mut("dep.Choice").unwrap().cases[0].name = "Changed".into();
    assert!(check_module_with_imports(&caller, &imports)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "CGT012"));
}
#[test]
fn new_data_fixtures_are_semantically_checked() {
    for source in [
        include_str!("../../cgx2-syntax/s2b-evidence/application.cg"),
        include_str!("../../cgx2-syntax/s2b-evidence/boundaries.cg"),
        include_str!("../../cgx2-syntax/s2b-evidence/selected.cg"),
    ] {
        checked(source);
    }
}

#[test]
fn contextual_inference_uses_later_arguments_and_nested_nonempty_literals() {
    checked("fn head<T>(values: List<T>, fallback: T) -> T:\n    fallback\nfn main() -> i64:\n    let xs = [[], [1]]\n    let array_value: Array<i64> = array([])\n    head([], 42)\n");
}
#[test]
fn intrinsic_shadowing_still_propagates_real_user_effects() {
    rejects("fn show(x: i64) -> i64:\n    arg_count()\nfn main() -> i64 effects() caps():\n    show(1)\n","CGT012");
}

#[test]
fn distinct_specializations_have_a_total_count_budget() {
    let mut source = String::new();
    for i in 0..257 {
        source.push_str(&format!("record Item{i}:\n    value: i64\n"));
    }
    source.push_str("fn id<T>(value: T) -> T:\n    value\nfn main() -> i64:\n");
    for i in 0..257 {
        source.push_str(&format!("    let x{i} = id(Item{i}(value: 1))\n"));
    }
    source.push_str("    1\n");
    rejects(&source, "CGT030");
}
