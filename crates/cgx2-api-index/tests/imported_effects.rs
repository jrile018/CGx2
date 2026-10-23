use cgx2_api_index::{build_index_with_imports, IndexError};
use cgx2_sema::{check_module_with_imports, collect_public_signatures, ImportedSignatures};
use cgx2_syntax::parse_module;

#[test]
fn fresh_import_checks_reject_stale_typed_indexes_and_incomplete_evidence() {
    let dependency = parse_module("module dep\npub fn value() -> i64:\n    1\n").unwrap();
    let caller =
        parse_module("module app\nimport dep\npub fn value() -> i64:\n    dep.value()\n").unwrap();
    let mut imports = ImportedSignatures::default();
    imports.modules.insert("dep".into());
    imports.public_functions.insert(
        "dep.value".into(),
        collect_public_signatures(&dependency)
            .unwrap()
            .remove("value")
            .unwrap(),
    );
    imports.check_and_record_module(&dependency).unwrap();
    let typed = check_module_with_imports(&caller, &imports).unwrap();
    let index = build_index_with_imports(&caller, &typed, &imports).unwrap();
    assert!(index.functions[0].effects.is_empty());
    assert_eq!(index.functions[0].effects_status, "inferred");
    let mut incomplete = imports.clone();
    incomplete.checked_effects = Default::default();
    assert_eq!(
        build_index_with_imports(&caller, &typed, &incomplete),
        Err(IndexError::MismatchedTypedModule)
    );
    let unknown = check_module_with_imports(&caller, &incomplete).unwrap();
    assert_eq!(
        build_index_with_imports(&caller, &unknown, &incomplete)
            .unwrap()
            .functions[0]
            .effects,
        ["Unknown"]
    );
    assert_eq!(
        build_index_with_imports(&caller, &unknown, &imports),
        Err(IndexError::MismatchedTypedModule)
    );

    let changed = parse_module("module dep\npub fn value() -> i64:\n    arg_count()\n").unwrap();
    imports.public_functions.insert(
        "dep.value".into(),
        collect_public_signatures(&changed)
            .unwrap()
            .remove("value")
            .unwrap(),
    );
    imports.check_and_record_module(&changed).unwrap();
    assert_eq!(
        build_index_with_imports(&caller, &typed, &imports),
        Err(IndexError::MismatchedTypedModule)
    );
    let fresh = check_module_with_imports(&caller, &imports).unwrap();
    let index = build_index_with_imports(&caller, &fresh, &imports).unwrap();
    assert_eq!(index.functions[0].effects, ["ReadArgs"]);
    assert_eq!(index.functions[0].capabilities, ["ArgsRead"]);
    let changed_signature =
        parse_module("module dep\npub fn value(x: i64) -> i64:\n    x\n").unwrap();
    imports.public_functions.insert(
        "dep.value".into(),
        collect_public_signatures(&changed_signature)
            .unwrap()
            .remove("value")
            .unwrap(),
    );
    assert!(matches!(
        build_index_with_imports(&caller, &fresh, &imports),
        Err(IndexError::InvalidModule(_))
    ));
}
