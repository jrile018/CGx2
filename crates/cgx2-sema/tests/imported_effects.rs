use cgx2_sema::{check_module_with_imports, collect_public_signatures, ImportedSignatures};
use cgx2_syntax::{parse_module, Module};

fn environment(sources: &[&str]) -> (ImportedSignatures, Vec<Module>) {
    let modules: Vec<_> = sources.iter().map(|s| parse_module(s).unwrap()).collect();
    let mut imports = ImportedSignatures::default();
    for module in &modules {
        let name = module.name.as_ref().unwrap().segments.join(".");
        imports.modules.insert(name.clone());
        for (function, signature) in collect_public_signatures(module).unwrap() {
            imports
                .public_functions
                .insert(format!("{name}.{function}"), signature);
        }
    }
    (imports, modules)
}

#[test]
fn signatures_annotations_and_partial_evidence_cannot_prove_purity() {
    let (mut imports, modules) = environment(&[
        "module base\npub fn value() -> i64 effects() caps():\n    1\n",
        "module middle\nimport base\npub fn value() -> i64:\n    base.value()\n",
    ]);
    let caller =
        parse_module("import middle\nfn f() -> i64 effects() caps():\n    middle.value()\n")
            .unwrap();
    assert!(check_module_with_imports(&caller, &imports).is_err());
    imports.check_and_record_module(&modules[1]).unwrap();
    assert!(check_module_with_imports(&caller, &imports).is_err());
    imports.check_and_record_module(&modules[0]).unwrap();
    // New base evidence does not upgrade the previously Unknown middle summary.
    assert!(check_module_with_imports(&caller, &imports).is_err());
    imports.check_and_record_module(&modules[1]).unwrap();
    assert!(check_module_with_imports(&caller, &imports)
        .unwrap()
        .effects["f"]
        .effects
        .is_empty());
    imports.checked_effects = Default::default();
    assert!(check_module_with_imports(&caller, &imports).is_err());
}

#[test]
fn changed_declarations_or_dependency_bodies_invalidate_older_evidence() {
    let (mut imports, modules) = environment(&[
        "module base\npub fn value() -> i64:\n    1\n",
        "module middle\nimport base\npub fn value() -> i64:\n    base.value()\n",
    ]);
    imports.check_and_record_module(&modules[0]).unwrap();
    imports.check_and_record_module(&modules[1]).unwrap();
    let caller =
        parse_module("import middle\nfn f() -> i64 effects() caps():\n    middle.value()\n")
            .unwrap();
    check_module_with_imports(&caller, &imports).unwrap();
    let mut changed = imports.clone();
    changed
        .public_functions
        .get_mut("base.value")
        .unwrap()
        .span
        .end += 1;
    assert!(check_module_with_imports(&caller, &changed).is_err());
    let dishonest =
        parse_module("module base\npub fn value() -> i64 effects() caps():\n    arg_count()\n")
            .unwrap();
    assert!(imports.check_and_record_module(&dishonest).is_err());
    assert!(check_module_with_imports(&caller, &imports).is_err());
    imports.check_and_record_module(&modules[0]).unwrap();
    assert!(check_module_with_imports(&caller, &imports).is_err());
    imports.check_and_record_module(&modules[1]).unwrap();
    check_module_with_imports(&caller, &imports).unwrap();
}

#[test]
fn checked_imports_do_not_supply_contract_termination_or_range_bound_proofs() {
    let (mut imports, modules) = environment(&["module dep\npub fn value() -> i64:\n    1\n"]);
    imports.check_and_record_module(&modules[0]).unwrap();
    for source in [
        "import dep\nfn f() -> i64:\n    requires dep.value() == 1\n    1\n",
        "import dep\nfn wrapper() -> i64:\n    dep.value()\nfn f() -> i64:\n    requires wrapper() == 1\n    1\n",
        "import dep\nfn f() -> i64:\n    for i in range(0, dep.value()):\n        i\n    0\n",
    ] {
        assert!(check_module_with_imports(&parse_module(source).unwrap(), &imports).is_err(), "{source}");
    }
}

#[test]
fn local_recursive_fixed_point_keeps_pure_and_argument_read_summaries() {
    for (body, expected) in [
        ("pub fn value() -> i64 effects() caps():\n    helper()\nfn helper() -> i64:\n    value()\n", false),
        ("pub fn value() -> i64:\n    helper()\nfn helper() -> i64:\n    if false:\n        return value()\n    else:\n        return arg_count()\n", true),
    ] {
        let (mut imports, modules) = environment(&[&format!("module dep\n{body}")]);
        let checked = imports.check_and_record_module(&modules[0]).unwrap();
        assert_eq!(checked.effects["value"].effects.contains("ReadArgs"), expected);
        let caller = parse_module("import dep\nfn f() -> i64:\n    dep.value()\n").unwrap();
        let typed = check_module_with_imports(&caller, &imports).unwrap();
        assert_eq!(typed.effects["f"].effects.contains("ReadArgs"), expected);
        assert_eq!(typed.effects["f"].capabilities.contains("ArgsRead"), expected);
        assert!(!typed.effects["f"].effects.contains("Unknown"));
    }
}

#[test]
fn unicode_qualified_identities_do_not_share_same_named_function_evidence() {
    let (mut imports, modules) = environment(&[
        "module café.left\npub fn value() -> i64:\n    1\n",
        "module café.right\npub fn value() -> i64:\n    arg_count()\n",
    ]);
    for module in &modules {
        imports.check_and_record_module(module).unwrap();
    }
    let caller = parse_module("import café.left\nimport café.right\nfn left() -> i64 effects() caps():\n    café.left.value()\nfn right() -> i64:\n    café.right.value()\n").unwrap();
    let typed = check_module_with_imports(&caller, &imports).unwrap();
    assert!(typed.effects["left"].effects.is_empty());
    assert!(typed.effects["right"].effects.contains("ReadArgs"));
}
