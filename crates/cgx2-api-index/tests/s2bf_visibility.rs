use cgx2_api_index::{build_index, build_index_with_imports, build_syntax_index, IndexError};
use cgx2_sema::{check_module, check_module_with_imports, ImportedSignatures, Type};
use cgx2_syntax::parse_module;

fn assert_hidden(index: &cgx2_api_index::ApiIndex, names: &[&str]) {
    for rendered in [index.render_json(), index.render_text()] {
        for name in names {
            assert!(!rendered.contains(name), "leaked {name}: {rendered}");
        }
    }
}

#[test]
fn public_projection_preserves_internal_calls_and_complete_public_types() {
    let source = include_str!("fixtures/s2bf-visibility.cg");
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    let original = typed.clone();
    let index = build_index(&module, &typed).unwrap();
    assert_eq!(typed, original);
    assert_eq!(typed.calls.len(), 11);
    assert_eq!(index.schema_version, 7);
    assert_eq!(index.instantiations.len(), 5);
    assert_hidden(&index, &["Secret", "private_id", "hidden"]);
    let nested = index
        .instantiations
        .iter()
        .find(|i| i.owner == "api.nested")
        .unwrap();
    assert_eq!(nested.type_arguments, ["List<api.nested::U>"]);
    assert_eq!(nested.parameters, nested.type_arguments);
    assert_eq!(nested.return_type, "List<api.nested::U>");
    let complete = "cgx2.core.Option<cgx2.core.Result<List<Array<api.Visible>>, Bool>>";
    assert!(index
        .instantiations
        .iter()
        .any(|i| i.type_arguments == [complete]
            && i.parameters == [complete]
            && i.return_type == complete));
    assert!(index
        .instantiations
        .iter()
        .any(|i| i.return_type == "api.Wrapper<api.Choice<api.Visible>>"));
    assert_eq!(
        index.find("id").unwrap().type_parameters[0].constraints,
        ["Eq", "Show"]
    );
    for instance in &index.instantiations {
        assert!(source[instance.span.start..instance.span.end].contains('<'));
    }
    assert_eq!(
        index.render_json(),
        build_index(&module, &typed).unwrap().render_json()
    );
    assert_eq!(
        index.render_text(),
        build_index(&module, &typed).unwrap().render_text()
    );
    let syntax = build_syntax_index(&module);
    assert_eq!(syntax.schema_version, 6);
    assert!(syntax.instantiations.is_empty());
    assert_eq!(syntax.semantics_status, "syntax-only");
    assert!(syntax
        .functions
        .iter()
        .all(|f| f.effects_status == "unchecked" && f.capabilities_status == "unchecked"));
}

#[test]
fn reviewed_private_owner_and_callee_do_not_enter_public_api() {
    let module = parse_module("module api\nfn id<T>(x: T) -> T:\n    x\nfn hidden() -> i64:\n    id<i64>(1)\npub fn run() -> i64:\n    0\n").unwrap();
    let typed = check_module(&module).unwrap();
    assert_eq!(typed.calls.len(), 1);
    let index = build_index(&module, &typed).unwrap();
    assert!(index.instantiations.is_empty());
    assert_hidden(&index, &["api.hidden", "api.id", "instantiate"]);
    assert_eq!(index.functions[0].name, "run");
}

#[test]
fn unnamed_private_owners_are_filtered() {
    let module = parse_module("pub fn id<T>(x: T) -> T:\n    x\nfn hidden() -> i64:\n    id<i64>(1)\npub fn run() -> i64:\n    id<i64>(2)\n").unwrap();
    let typed = check_module(&module).unwrap();
    assert_eq!(typed.calls.len(), 2);
    let index = build_index(&module, &typed).unwrap();
    assert_eq!(index.instantiations.len(), 1);
    assert_eq!(index.instantiations[0].owner, "run");
    assert_eq!(index.instantiations[0].callee, "id");
}

#[test]
fn every_container_and_nominal_argument_obeys_visibility() {
    for ty in [
        "Secret",
        "List<Secret>",
        "Array<Secret>",
        "Option<Secret>",
        "Result<i64, Secret>",
        "Visible<Secret>",
        "Choice<Secret>",
    ] {
        let source = format!("module api\nrecord Secret:\n    value: i64\npub record Visible<T>:\n    value: T\npub variant Choice<T>:\n    Value(T)\npub fn id<T>(x: T) -> T:\n    x\npub fn run() -> i64:\n    let hidden = id<Option<{ty}>>(Option.None())\n    7\n");
        let module = parse_module(&source).unwrap();
        let typed = check_module(&module).unwrap();
        assert_eq!(typed.calls.len(), 1);
        let index = build_index(&module, &typed).unwrap();
        assert!(index.instantiations.is_empty(), "{ty}");
        assert_hidden(&index, &["Secret"]);
    }
}

#[test]
fn imported_public_identities_and_owner_binders_are_retained() {
    let dep = parse_module("module dep\npub record Value:\n    number: i64\npub variant Choice<T>:\n    Value(T)\npub fn id<T: Eq + Show>(x: T) -> T effects() caps():\n    x\n").unwrap();
    let mut imports = ImportedSignatures::default();
    imports.modules.insert("dep".into());
    imports.public_functions.insert(
        "dep.id".into(),
        cgx2_sema::collect_public_signatures(&dep)
            .unwrap()
            .remove("id")
            .unwrap(),
    );
    let mut record = dep.records[0].clone();
    record.name = "dep.Value".into();
    imports.public_records.insert(record.name.clone(), record);
    let mut variant = dep.variants[0].clone();
    variant.name = "dep.Choice".into();
    imports
        .public_variants
        .insert(variant.name.clone(), variant);
    imports.check_and_record_module(&dep).unwrap();
    let module = parse_module("module app\nimport dep\nrecord Secret:\n    value: i64\npub fn wrap<U: Eq + Show>(x: Array<U>) -> Array<U> effects() caps():\n    dep.id<Array<U>>(x)\npub fn run() -> i64:\n    let good = dep.id<Option<dep.Choice<dep.Value>>>(Option.None())\n    let hidden = dep.id<Option<Secret>>(Option.None())\n    7\n").unwrap();
    let typed = check_module_with_imports(&module, &imports).unwrap();
    let original = typed.clone();
    let index = build_index_with_imports(&module, &typed, &imports).unwrap();
    assert_eq!(typed, original);
    assert_eq!(typed.calls.len(), 3);
    assert_eq!(index.instantiations.len(), 2);
    assert!(index.instantiations.iter().all(|i| i.callee == "dep.id"));
    assert!(index
        .instantiations
        .iter()
        .any(|i| i.return_type == "cgx2.core.Option<dep.Choice<dep.Value>>"));
    assert!(index
        .instantiations
        .iter()
        .any(|i| i.return_type == "Array<app.wrap::U>"));
    assert_hidden(&index, &["Secret"]);
}

#[test]
fn imported_signature_identities_are_checked_beyond_type_arguments() {
    // Conservative behavior even for caller-supplied signatures that mention
    // a nominal identity absent from the public imported declaration set.
    for hidden in [
        Type::Record("dep.Secret".into()),
        Type::Variant {
            name: "dep.Secret".into(),
            arguments: vec![],
        },
    ] {
        let dep = parse_module("module dep\npub fn value<T>(x: T) -> T:\n    x\n").unwrap();
        let mut signature = cgx2_sema::collect_public_signatures(&dep)
            .unwrap()
            .remove("value")
            .unwrap();
        let mut consume = signature.clone();
        consume.parameters = vec![Type::List(Box::new(hidden.clone()))];
        consume.return_type = Type::I64;
        signature.return_type = hidden;
        let mut imports = ImportedSignatures::default();
        imports.modules.insert("dep".into());
        imports
            .public_functions
            .insert("dep.value".into(), signature);
        imports
            .public_functions
            .insert("dep.consume".into(), consume);
        let module = parse_module("module app\nimport dep\npub fn run() -> i64:\n    let hidden = dep.value<i64>(1)\n    dep.consume<i64>([hidden])\n").unwrap();
        let typed = check_module_with_imports(&module, &imports).unwrap();
        assert_eq!(typed.calls.len(), 2);
        let index = build_index_with_imports(&module, &typed, &imports).unwrap();
        assert!(index.instantiations.is_empty());
        assert_hidden(&index, &["dep.Secret"]);
    }
}

#[test]
fn invalid_or_changed_modules_are_rejected_before_projection() {
    let good =
        parse_module("pub fn id<T: Eq>(x: T) -> T:\n    x\npub fn run() -> i64:\n    id<i64>(1)\n")
            .unwrap();
    let typed = check_module(&good).unwrap();
    let mut stale = typed.clone();
    stale.calls.clear();
    assert_eq!(
        build_index(&good, &stale),
        Err(IndexError::MismatchedTypedModule)
    );
    let invalid = parse_module("pub fn run() -> i64:\n    missing<i64>(1)\n").unwrap();
    assert!(matches!(
        build_index(&invalid, &typed),
        Err(IndexError::InvalidModule(_))
    ));
}
