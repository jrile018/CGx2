use cgx2_emit_rust::emit_package_rust;
use cgx2_package::load_checked_package;
use std::path::Path;

// The public output names encode the entire defining identity, including type
// arguments. Checking field declarations catches errors hidden by scalar output.
fn emitted_type(identity: &str) -> String {
    format!(
        "cgx2_user_hex_{}",
        identity
            .bytes()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}

fn assert_field(generated: &str, field: &str, identity: &str) {
    assert!(
        generated.contains(&format!(
            "cgx2_user_{}_{field}: {},",
            field.len(),
            emitted_type(identity)
        )),
        "missing canonical field {field}: {identity}"
    );
}

#[test]
fn package_record_fields_preserve_complete_nominal_identities() {
    for fixture in ["s2a-local", "s2a-imports"] {
        let package = load_checked_package(&Path::new("tests/fixtures").join(fixture)).unwrap();
        let generated = emit_package_rust(&package).unwrap();
        assert_eq!(generated, emit_package_rust(&package).unwrap());
        let modules: &[&str] = if fixture == "s2a-local" {
            &["app.main"]
        } else {
            &["app.left", "app.right"]
        };
        for module in modules {
            assert_field(&generated, "flag", &format!("{module}.Flag"));
            assert_field(&generated, "point", &format!("{module}.Point"));
            assert_field(
                &generated,
                "nested",
                &format!("cgx2.core.Option<cgx2.core.Result<{module}.Point, {module}.Flag>>"),
            );
            assert!(generated.contains(&format!(
                "{}({}),",
                "cgx2_user_5_Value",
                emitted_type(&format!("{module}.Point"))
            )));
        }
        if fixture == "s2a-local" {
            assert_field(&generated, "maybe", "cgx2.core.Option<app.main.Flag>");
            assert_field(
                &generated,
                "outcome",
                "cgx2.core.Result<app.main.Point, app.main.Flag>",
            );
            assert_field(
                &generated,
                "reversed",
                "cgx2.core.Result<cgx2.core.Option<app.main.Point>, app.main.Flag>",
            );
        } else {
            assert_field(&generated, "left", "app.left.Holder");
            assert_field(&generated, "right", "app.right.Holder");
            assert_field(&generated, "maybe", "cgx2.core.Option<app.left.Flag>");
            assert_field(
                &generated,
                "outcome",
                "cgx2.core.Result<app.right.Point, app.left.Flag>",
            );
            assert_field(
                &generated,
                "nested",
                "cgx2.core.Option<cgx2.core.Result<app.left.Point, app.right.Flag>>",
            );
            assert_field(
                &generated,
                "reversed",
                "cgx2.core.Result<cgx2.core.Option<app.right.Point>, app.left.Flag>",
            );
        }
        for bare in ["Flag", "Point", "Holder"] {
            assert!(!generated.contains(&format!("cgx2_user_{}_{bare}", bare.len())));
        }
    }
}
