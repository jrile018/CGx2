#[test]
fn security_reports_unsupported_admission_instead_of_empty_analysis() {
    let policy = cgx2_security::parse_policy(
        "{\"schema_version\":1,\"allowed_effects\":[],\"allow_unknown\":true}",
    )
    .unwrap();
    let error = cgx2_security::audit("variant Reply:\n    Empty\n", &policy).unwrap_err();
    assert!(error.contains("CGI006"));
    assert!(error.contains("admission"));
}
