use cgx2_emit_rust::emit_package_rust;
use cgx2_package::load_checked_package;
use std::path::Path;

// The public output names encode the entire defining identity, including type
// arguments. Checking field declarations catches errors hidden by scalar output.
fn assert_field(
    program: &cgx2_sema::default_managed::ManagedProgramV1,
    field: &str,
    identity: &str,
) {
    let p = program.program();
    assert!(
        p.layouts.iter().any(|layout| match &layout.shape {
            cgx2_sema::default_managed::model::Shape::KRecord(fields) =>
                fields.iter().any(|(n, t)| p.strings[*n] == field
                    && cgx2_sema::type_key(&program.types()[*t]) == identity),
            _ => false,
        }),
        "missing canonical field {field}: {identity}"
    );
}

#[test]
fn package_record_fields_preserve_complete_nominal_identities() {
    for fixture in ["s2a-local", "s2a-imports"] {
        let package = load_checked_package(&Path::new("tests/fixtures").join(fixture)).unwrap();
        let generated = emit_package_rust(&package).unwrap();
        let program = package.managed_program_v1().unwrap();
        assert!(generated.contains(&program.rust_data().unwrap()));
        assert_eq!(generated, emit_package_rust(&package).unwrap());
        let modules: &[&str] = if fixture == "s2a-local" {
            &["app.main"]
        } else {
            &["app.left", "app.right"]
        };
        for module in modules {
            assert_field(&program, "flag", &format!("{module}.Flag"));
            assert_field(&program, "point", &format!("{module}.Point"));
            assert_field(
                &program,
                "nested",
                &format!("cgx2.core.Option<cgx2.core.Result<{module}.Point, {module}.Flag>>"),
            );
            assert!(
                program
                    .program()
                    .layouts
                    .iter()
                    .any(|layout| match &layout.shape {
                        cgx2_sema::default_managed::model::Shape::KVariant(cases) => cases
                            .iter()
                            .any(|(n, ts)| program.program().strings[*n] == "Value"
                                && ts.iter().any(|t| cgx2_sema::type_key(&program.types()[*t])
                                    == format!("{module}.Point"))),
                        _ => false,
                    })
            );
        }
        if fixture == "s2a-local" {
            assert_field(&program, "maybe", "cgx2.core.Option<app.main.Flag>");
            assert_field(
                &program,
                "outcome",
                "cgx2.core.Result<app.main.Point, app.main.Flag>",
            );
            assert_field(
                &program,
                "reversed",
                "cgx2.core.Result<cgx2.core.Option<app.main.Point>, app.main.Flag>",
            );
        } else {
            assert_field(&program, "left", "app.left.Holder");
            assert_field(&program, "right", "app.right.Holder");
            assert_field(&program, "maybe", "cgx2.core.Option<app.left.Flag>");
            assert_field(
                &program,
                "outcome",
                "cgx2.core.Result<app.right.Point, app.left.Flag>",
            );
            assert_field(
                &program,
                "nested",
                "cgx2.core.Option<cgx2.core.Result<app.left.Point, app.right.Flag>>",
            );
            assert_field(
                &program,
                "reversed",
                "cgx2.core.Result<cgx2.core.Option<app.right.Point>, app.left.Flag>",
            );
        }
        for bare in ["Flag", "Point", "Holder"] {
            assert!(!generated.contains(&format!("cgx2_user_{}_{bare}", bare.len())));
        }
    }
}
