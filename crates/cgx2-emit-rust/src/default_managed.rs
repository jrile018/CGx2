use crate::EmitDiagnostic;
use cgx2_sema::default_managed::ManagedProgramV1;
pub fn emit_managed_rust(program: &ManagedProgramV1) -> Result<String, Vec<EmitDiagnostic>> {
    if program.program().entry == usize::MAX {
        let loc = program.program().output_loc;
        return Err(vec![EmitDiagnostic {
            code: "CGRS001",
            message: "function module requires main".into(),
            span: cgx2_syntax::Span {
                start: loc.start,
                end: loc.end,
            },
        }]);
    }
    let data = program.rust_data().map_err(|ds| {
        ds.into_iter()
            .map(|d| EmitDiagnostic {
                code: d.code,
                message: d.message,
                span: d.span,
            })
            .collect::<Vec<_>>()
    })?;
    let runtime = embedded_runtime();
    let print_result = program.types()[program.program().functions[program.program().entry].result]
        != cgx2_sema::Type::Unit;
    Ok(format!(
        r#"// CGx2 default managed v1; provenance: {}; inputs sha256: {}; original source sha256: {}; model/root/effects/callee sha256: {}
{runtime}
use cgx2_runtime::default_managed::model::{{*,Shape::*,Op::*,Statement::*}};
fn main() {{
    let program={data};
    let args=std::env::args().skip(1).collect::<Vec<_>>();
    #[cfg(feature="fault-injection")]
    let execution=if let Some(index)=option_env!("CGX2_TEST_FAIL_RESERVE").and_then(|n|n.parse::<usize>().ok()) {{
        match cgx2_runtime::Session::try_new_with_host_failure_at_reserve(index) {{
            Ok(session)=>cgx2_runtime::default_managed::run_with_session(&program,&args,session),
            Err(error)=>{{let loc=program.functions[program.entry].loc;eprintln!("CGR090: managed session entry failed: {{error:?}} at {{}}..{{}}",loc.start,loc.end);std::process::exit(1)}}
        }}
    }} else {{cgx2_runtime::default_managed::run(&program,&args)}};
    #[cfg(not(feature="fault-injection"))]
    let execution=cgx2_runtime::default_managed::run(&program,&args);
    if std::env::var_os("CGX2_MANAGED_EVIDENCE").is_some() {{ eprintln!("CGX2_MANAGED_EVIDENCE {{}}",execution.evidence.json(&execution.memory)); }}
    match execution.result {{Ok(value)=>{{let shown=value.to_string();if {print_result}{{println!("{{shown}}");}}}},Err(error)=>{{eprintln!("{{}}: {{}} at {{}}..{{}}",error.code,error.message,error.loc.start,error.loc.end);std::process::exit(1)}}}}
}}
"#,
        program.provenance_v1().kind(),
        program.provenance_v1().identity_sha256(),
        if program.source_sha256().is_empty() {
            "unavailable"
        } else {
            program.source_sha256()
        },
        program.analysis_sha256()
    ))
}

pub(super) fn embedded_runtime() -> String {
    let mut lib = include_str!("../../cgx2-runtime/src/lib.rs").to_owned();
    for (name, body) in [
        ("budget", include_str!("../../cgx2-runtime/src/budget.rs")),
        ("fault", include_str!("../../cgx2-runtime/src/fault.rs")),
        ("graph", include_str!("../../cgx2-runtime/src/graph.rs")),
        ("heap", include_str!("../../cgx2-runtime/src/heap.rs")),
        ("owned", include_str!("../../cgx2-runtime/src/owned.rs")),
        ("region", include_str!("../../cgx2-runtime/src/region.rs")),
        ("report", include_str!("../../cgx2-runtime/src/report.rs")),
        ("scope", include_str!("../../cgx2-runtime/src/scope.rs")),
    ] {
        lib = lib.replace(
            &format!("mod {name};"),
            &format!("mod {name} {{\n{body}\n}}"),
        );
    }
    format!(
        "#[allow(dead_code,unused_imports)] mod cgx2_runtime {{\n{}\n}}",
        lib.replace("crate::", "crate::cgx2_runtime::")
    )
}

#[cfg(test)]
mod auth_tests {
    #[test]
    fn source_and_ast_only_emitters_advertise_exact_provenance() {
        let source = "# café🙂\nfn main() -> String:\n \"héllo🙂\"\n";
        let program = cgx2_sema::default_managed::compile_source(source).unwrap();
        let output = super::emit_managed_rust(&program).unwrap();
        assert!(
            output
                .lines()
                .next()
                .unwrap()
                .contains(program.source_sha256())
        );
        assert!(
            output
                .lines()
                .next()
                .unwrap()
                .contains("provenance: source-v1;")
        );
        let module = cgx2_syntax::parse_module(source).unwrap();
        let checked = cgx2_sema::check_module(&module).unwrap();
        let output = crate::emit_rust(&module, &checked).unwrap();
        assert!(
            output
                .lines()
                .next()
                .unwrap()
                .contains("provenance: ast-only-v1;")
        );
        assert!(
            output
                .lines()
                .next()
                .unwrap()
                .contains("original source sha256: unavailable;")
        );
        assert!(
            !output
                .lines()
                .next()
                .unwrap()
                .contains(program.source_sha256())
        );
        let other = cgx2_syntax::parse_module("fn main() -> String:\n \"different\"\n").unwrap();
        assert!(crate::emit_rust(&other, &checked).is_err());
    }
}
