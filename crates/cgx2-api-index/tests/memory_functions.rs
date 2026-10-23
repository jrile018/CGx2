use cgx2_api_index::build_index;
use cgx2_sema::check_module;
use cgx2_syntax::parse_module;

#[test]
fn indexes_public_memory_function_as_unit_with_declared_effects() {
    let source = "pub fn build() -> Unit effects(ManagedAlloc, Collect, Mutate) caps(MemoryCollect):\n    let r = memory.region(mode: manual, limit: 64)\n    memory.collect(r)\n    memory.close(r)\nfn main() -> Unit effects(ManagedAlloc, Collect, Mutate) caps(MemoryCollect):\n    build()\n";
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    let index = build_index(&module, &typed).unwrap();
    assert_eq!(index.functions.len(), 1);
    let function = &index.functions[0];
    assert_eq!(function.name, "build");
    assert_eq!(function.return_type, "Unit");
    assert!(function.parameters.is_empty());
    assert_eq!(function.effects_status, "explicit");
    assert_eq!(function.capabilities, vec!["MemoryCollect"]);
    assert!(index.render_json().contains("\"return_type\":\"Unit\""));
}

#[test]
fn indexes_public_scalar_signature_and_transitive_memory_capability() {
    let source = "fn inner(size: i64) -> Unit effects(ManagedAlloc, Collect, Mutate) caps(MemoryCollect):\n let region = memory.region(mode: manual, limit: 128)\n let block = memory.alloc_bytes(region, size)\n memory.drop(block)\n memory.collect(region)\n memory.close(region)\npub fn allocate(size: i64) -> Unit effects(ManagedAlloc, Collect, Mutate) caps(MemoryCollect):\n inner(size)\nfn main() -> Unit effects(ManagedAlloc, Collect, Mutate) caps(MemoryCollect):\n allocate(8)\n";
    let module = parse_module(source).unwrap();
    let typed = check_module(&module).unwrap();
    let index = build_index(&module, &typed).unwrap();
    let function = index.find("allocate").unwrap();
    assert_eq!(function.parameters[0].name, "size");
    assert_eq!(function.parameters[0].ty, "i64");
    assert_eq!(function.return_type, "Unit");
    assert_eq!(function.capabilities, vec!["MemoryCollect"]);
    assert!(function.effects.contains(&"Collect".to_string()));
    assert!(index.render_json().contains("\"name\":\"allocate\""));
}
