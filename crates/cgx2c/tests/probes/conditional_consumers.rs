//! Read-only boundary probe. Compile against the workspace's consumer rlibs;
//! no notebook/Python source or Cargo dependencies are changed by this probe.
use cgx2_notebook_format::{Cell, CellKind, Notebook};
use cgx2_notebook_kernel::{NotebookKernel, facts_for_source};

fn main() {
    for (name, source) in [
        (
            "script",
            include_str!("../fixtures/conditional_memory_success.cg"),
        ),
        (
            "functions",
            include_str!("../fixtures/conditional_memory_functions.cg"),
        ),
        (
            "nested-graph",
            include_str!("../fixtures/conditional_memory_graph.cg"),
        ),
    ] {
        let module = cgx2_syntax::parse_module(source).unwrap();
        let checked = cgx2_sema::check_module(&module).unwrap();
        let errors = cgx2_emit_python::emit_python(&module, &checked).unwrap_err();
        assert_eq!(errors[0].code, "CGE001");
        assert!(errors[0].message.contains("memory"));
        let facts_error = facts_for_source(source).unwrap_err();
        let mut kernel = NotebookKernel::open(Notebook {
            version: 1,
            cells: vec![Cell {
                id: "memory".into(),
                kind: CellKind::Code,
                source: source.into(),
                outputs: vec![],
                provenance: None,
            }],
        })
        .unwrap();
        let error = kernel.run_cell("memory").unwrap_err();
        assert_eq!(error.code, facts_error.code);
        assert!(kernel.notebook().cells[0].outputs.is_empty());
        println!(
            "{name}: Python={} notebook={} ({})",
            errors[0].code, error.code, error.message
        );
    }
}
