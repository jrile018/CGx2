use cgx2_sema::{
    check_module, check_module_with_imports, collect_public_signatures_with_imports, lower_module,
    merge_modules, ImportedSignatures, Type,
};
use cgx2_syntax::{canonical_module, parse_module, Module};
use std::collections::BTreeSet;

fn lowered(source: &str) -> Module {
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    let concrete = lower_module(&module, &typed).unwrap();
    check_module(&concrete).unwrap();
    // Consumers also canonicalize the concrete module before rechecking it.
    let canonical = canonical_module(&concrete, &BTreeSet::new());
    assert_eq!(canonical_module(&canonical, &BTreeSet::new()), canonical);
    check_module(&canonical).unwrap();
    concrete
}

#[test]
fn qualified_concrete_fields_payloads_and_containers_recheck() {
    let concrete = lowered(
        "module out.main\nrecord Long:\n    x: String\nvariant Reply:\n    Item(Array<List<Long>>)\nrecord Outer:\n    reply: Reply\nfn main() -> List<Long>:\n    let nested = Outer(reply: Reply.Item(array([[Long(x: \"é\")]])))\n    [Long(x: \"é\")]\n",
    );
    assert_eq!(
        check_module(&concrete).unwrap().functions["main"].return_type,
        Type::List(Box::new(Type::Record("out.main.Long".into())))
    );
}

#[test]
fn checked_public_templates_generate_internal_functions_and_aggregates() {
    let source = include_str!("source_specialization/nested-private.cg");
    let original = parse_module(source).unwrap();
    let typed = check_module(&original).unwrap();
    assert!(typed.functions["id"].public);
    assert!(
        original
            .records
            .iter()
            .find(|r| r.name == "Wrapper")
            .unwrap()
            .public
    );
    let concrete = lowered(source);
    assert!(concrete
        .functions
        .iter()
        .filter(|f| f.name.contains('<'))
        .all(|f| !f.public));
    assert!(concrete
        .records
        .iter()
        .filter(|r| r.name.contains('<'))
        .all(|r| !r.public));
    assert!(concrete
        .variants
        .iter()
        .filter(|v| v.name.contains('<'))
        .all(|v| !v.public));
    assert!(concrete.functions.iter().all(|f| !f.public));
    assert!(concrete
        .functions
        .iter()
        .any(|f| f.name == "unused<api.Secret>"));
    assert!(concrete
        .records
        .iter()
        .any(|r| r.name.contains("Wrapper<api.Choice<api.Secret>>")));
    assert!(concrete
        .variants
        .iter()
        .any(|v| v.name == "api.Choice<api.Secret>"));
    // The caller-owned declarations and checked template evidence remain unchanged.
    assert_eq!(check_module(&original).unwrap(), typed);
    let mut dishonest = concrete.clone();
    let instance = dishonest
        .functions
        .iter_mut()
        .find(|f| f.name == "unused<api.Secret>")
        .unwrap();
    instance.body = parse_module("fn bad() -> i64:\n    arg_count()\n")
        .unwrap()
        .functions
        .remove(0)
        .body;
    assert!(check_module(&dishonest)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "CGT012"));
    let mut tampered = original.clone();
    tampered.functions[0].body = parse_module("fn bad() -> i64:\n    arg_count()\n")
        .unwrap()
        .functions
        .remove(0)
        .body;
    assert!(lower_module(&tampered, &typed).is_err());
}

#[test]
fn same_named_module_nominal_and_binder_keep_distinct_identities() {
    lowered("module T\nrecord T:\n    value: i64\npub fn id<T>(x: T) -> T effects() caps():\n    x\nfn main() -> i64:\n    let hidden = id<T>(T(value: 4))\n    id<i64>(7)\n");
    lowered("module same.same\nrecord same:\n    value: i64\npub record Wrapper<same>:\n    value: same\npub fn id<same>(x: same) -> Wrapper<same> effects() caps():\n    Wrapper<same>(value: x)\nfn main() -> i64:\n    let hidden = id<same>(same(value: 4))\n    7\n");
}

#[test]
fn original_public_privacy_and_effects_still_reject_before_lowering() {
    for (source, code) in [
        ("record Secret:\n    value: i64\npub fn leak<T>(x: T) -> Secret:\n    Secret(value: 1)\n", "CGT022"),
        ("record Secret:\n    value: i64\npub record Leak<T>:\n    secret: Secret\n", "CGT022"),
        ("record Secret:\n    value: i64\npub variant Leak<T>:\n    Secret(Secret)\n", "CGT022"),
        ("pub fn dishonest<T>(x: T) -> i64 effects() caps():\n    arg_count()\nfn main() -> i64:\n    dishonest<i64>(1)\n", "CGT012"),
    ] {
        let module = parse_module(source).unwrap();
        assert!(check_module(&module).unwrap_err().iter().any(|e| e.code == code));
    }
}

#[test]
fn integrated_visibility_and_public_source_fixtures_recheck() {
    for source in [
        include_str!("../../cgx2-api-index/tests/fixtures/s2bf-visibility.cg"),
        include_str!("../../cgx2-api-index/tests/fixtures/s2bf-public.cg"),
        include_str!("../../cgx2-api-index/tests/fixtures/s2bf-public-values.cg"),
    ] {
        lowered(source);
    }
}

#[test]
fn imported_generic_private_arguments_keep_provenance_and_source_admission() {
    let dep = parse_module(include_str!("source_specialization/package/src/dep.cg")).unwrap();
    let caller = parse_module(include_str!("source_specialization/package/src/main.cg")).unwrap();
    let mut imports = ImportedSignatures::default();
    imports.modules.insert("dep.library".into());
    let canonical = canonical_module(&dep, &BTreeSet::new());
    for mut record in canonical.records {
        record.name = format!("dep.library.{}", record.name);
        if record.public {
            imports.public_records.insert(record.name.clone(), record);
        } else {
            imports.private_records.insert(record.name);
        }
    }
    for mut variant in canonical.variants {
        variant.name = format!("dep.library.{}", variant.name);
        imports
            .public_variants
            .insert(variant.name.clone(), variant);
    }
    for (name, signature) in collect_public_signatures_with_imports(&dep, &imports).unwrap() {
        imports
            .public_functions
            .insert(format!("dep.library.{name}"), signature);
    }
    imports.check_and_record_module(&dep).unwrap();
    let checked = check_module_with_imports(&caller, &imports).unwrap();
    assert!(checked
        .calls
        .values()
        .any(|call| call.callee == "dep.library.unused"
            && call.arguments == [Type::Record("app.main.Secret".into())]));
    let merged = merge_modules(&[&dep, &caller], "app.main");
    let concrete = lower_module(&merged, &check_module(&merged).unwrap()).unwrap();
    check_module(&concrete).unwrap();
    assert!(concrete
        .functions
        .iter()
        .any(|f| f.name == "dep.library.nested<app.main.Secret>" && !f.public));
    assert!(concrete
        .functions
        .iter()
        .any(|f| f.name == "dep.library.nested<dep.library.Secret>" && !f.public));

    for (name, code) in [
        ("dep.library.Secret", "CGT022"),
        ("dep.library.Absent", "CGT002"),
    ] {
        let bad = parse_module(&format!("module app.main\nimport dep.library\nfn main() -> i64:\n    dep.library.unused<{name}>(7)\n")).unwrap();
        assert!(check_module_with_imports(&bad, &imports)
            .unwrap_err()
            .iter()
            .any(|e| e.code == code));
    }
    // A changed imported signature invalidates the authenticated generic body.
    let mut stale = imports.clone();
    stale
        .public_functions
        .get_mut("dep.library.id")
        .unwrap()
        .span
        .end += 1;
    assert!(check_module_with_imports(&caller, &stale)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "CGT012"));
}

#[test]
fn public_concrete_interfaces_keep_source_exports_separate_from_lowered_aggregates() {
    let source = include_str!("source_specialization/public-interfaces.cg");
    let original = parse_module(source).unwrap();
    let typed = check_module(&original).unwrap();
    assert!(typed.functions["wrapped"].public);
    assert!(original.records.iter().all(|r| r.public));
    let concrete = lowered(source);
    assert!(concrete.records.iter().all(|r| !r.public));
    assert!(concrete.variants.iter().all(|v| !v.public));
    assert!(concrete.functions.iter().all(|f| !f.public));
    assert_eq!(check_module(&original).unwrap(), typed);
}
