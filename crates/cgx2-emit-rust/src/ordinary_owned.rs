use crate::EmitDiagnostic;
use cgx2_sema::ordinary_owned::{OwnedProgram, OwnedProgramV2};

/// Emit only from a private-field, source-authenticated and body-checked producer.
pub fn emit_owned_rust(program: &OwnedProgram) -> Result<String, Vec<EmitDiagnostic>> {
    let upgraded = program.upgrade_v2().map_err(|ds| {
        ds.into_iter()
            .map(|d| EmitDiagnostic {
                code: d.code,
                message: d.message,
                span: d.span,
            })
            .collect::<Vec<_>>()
    })?;
    emit_owned_result_rust(&upgraded)
}

pub fn emit_owned_result_rust(program: &OwnedProgramV2) -> Result<String, Vec<EmitDiagnostic>> {
    let data = program.rust_data().map_err(|ds| {
        ds.into_iter()
            .map(|d| EmitDiagnostic {
                code: d.code,
                message: d.message,
                span: d.span,
            })
            .collect::<Vec<_>>()
    })?;
    let model = include_str!("../../cgx2-sema/src/ordinary_owned/model.rs");
    let model_v2 = include_str!("../../cgx2-sema/src/ordinary_owned/model_v2.rs");
    let engine = include_str!("../../cgx2-sema/src/ordinary_owned/engine.rs");
    Ok(format!(
        r#"// CGx2 ordinary-owned schema {}; source sha256: {}; module sha256: {}
// Checked layout/effect/arm/callee analysis sha256: {}
#![allow(dead_code)]
mod model {{
{model}
}}
mod model_v2 {{
{model_v2}
}}
mod engine {{
{engine}
}}
use model_v2::*;
fn main() {{
    let model = {data};
    #[cfg(feature = "fault-injection")]
    let execution = if let Some(index) = option_env!("CGX2_TEST_FAIL_RESERVE").and_then(|s| s.parse::<usize>().ok()) {{
        match cgx2_runtime::Session::try_new_with_host_failure_at_reserve(index) {{
            Ok(session) => engine::run_with_session(&model, session),
            Err(error) => engine::Execution {{ result: Err(engine::Failure {{ code: "CGR080", message: format!("session allocation failed: {{error:?}}"), span: model.functions[model.entry].span }}), evidence: engine::Evidence::default() }},
        }}
    }} else {{ engine::run(&model) }};
    #[cfg(not(feature = "fault-injection"))]
    let execution = engine::run(&model);
    if std::env::var_os("CGX2_OWNED_EVIDENCE").is_some() {{ eprintln!("CGX2_OWNED_EVIDENCE {{}}", execution.evidence.{evidence_json}()); }}
    match execution.result {{
        Ok(engine::Scalar::Unit) => {{}},
        Ok(value) => println!("{{value}}"),
        Err(error) => {{ eprintln!("{{}}: {{}} at {{}}..{{}}", error.code, error.message, error.span.start, error.span.end); std::process::exit(1); }}
    }}
}}
"#,
        program.schema_version(),
        program.source_sha256(),
        program.module_sha256(),
        program.analysis_sha256(),
        evidence_json = if program.schema_version() == 1 {
            "json"
        } else {
            "json_v2"
        }
    ))
}
