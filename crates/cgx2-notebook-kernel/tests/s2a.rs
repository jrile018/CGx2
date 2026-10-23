#[test]
fn notebook_variants_and_deferred_expressions_are_source_located_rejections() {
    for source in [
        "variant Reply:\n    Empty\n",
        "let x = Option<i64>.Some(1)\n",
        "let x = [1, 2]\n",
        "for i in range(0, 1):\n    invariant i >= 0\n    i\n",
    ] {
        let error = cgx2_notebook_kernel::facts_for_source(source).unwrap_err();
        assert!(error.to_string().contains("CGNK026"));
        assert!(error.to_string().contains(" at "));
    }
    let book =
        cgx2_notebook_format::parse_notebook("#!cgnb 1\n%% code id=imports\nimport app.left\n")
            .unwrap();
    let error = match cgx2_notebook_kernel::NotebookKernel::open_with_project(
        book,
        Some(std::path::Path::new(
            "../cgx2-syntax/s2a-evidence/fixtures/package",
        )),
    ) {
        Err(error) => error,
        Ok(_) => panic!("package must reject"),
    };
    assert!(error.to_string().contains("CGNK026"));
}
