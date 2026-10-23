#[test]
fn source_and_package_analysis_reject_profile_before_traversal() {
    let source = include_str!("../../cgx2-syntax/s2a-evidence/fixtures/variants.cg");
    for syntax_only in [false, true] {
        let errors = cgx2_effect_index::index_source(source, syntax_only).unwrap_err();
        assert_eq!(errors[0].code, "CGI006");
        assert_eq!(
            &source[errors[0].span.start..errors[0].span.start + 7],
            "variant"
        );
    }
    let errors = cgx2_effect_index::package_v1::index_package(std::path::Path::new(
        "../cgx2-syntax/s2a-evidence/fixtures/package",
    ))
    .unwrap_err();
    assert_eq!(errors[0].code, "CGI006");
    assert!(errors[0].span.is_some());
}
