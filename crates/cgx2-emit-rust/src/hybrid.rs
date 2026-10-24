use crate::EmitDiagnostic;
use cgx2_sema::default_managed::hybrid::HybridProgramV1;
pub fn emit_hybrid_rust(program: &HybridProgramV1) -> Result<String, Vec<EmitDiagnostic>> {
    let errors = |ds: Vec<cgx2_sema::TypeDiagnostic>| {
        ds.into_iter()
            .map(|d| EmitDiagnostic {
                code: d.code,
                message: d.message,
                span: d.span,
            })
            .collect::<Vec<_>>()
    };
    let owned = program.owned_rust_data().map_err(errors)?;
    let managed = program.managed().rust_data().map_err(errors)?;
    let entries = program
        .bridges()
        .iter()
        .map(|(n, id)| format!("({n:?}.into(),{id})"))
        .collect::<Vec<_>>()
        .join(",");
    let runtime = super::default_managed::embedded_runtime();
    let model = include_str!("../../cgx2-sema/src/ordinary_owned/model.rs");
    let model_v2 = include_str!("../../cgx2-sema/src/ordinary_owned/model_v2.rs");
    let engine = include_str!("../../cgx2-sema/src/ordinary_owned/engine.rs")
        .replace("cgx2_runtime::", "crate::cgx2_runtime::");
    let execution = include_str!("../../cgx2-sema/src/default_managed/hybrid_engine.rs")
        .replace("cgx2_runtime::", "crate::cgx2_runtime::");
    Ok(format!(
        r#"// CGx2 authenticated mixed v1; source {}; analysis {}
#![allow(dead_code)]
{runtime}
mod model {{ {model} }}
mod model_v2 {{ {model_v2} }}
mod engine {{ {engine} }}
mod owned_bridge {{
    pub use crate::model_v2::Model;
    pub use crate::engine::{{run_with_bridge,ManagedBridge,Scalar,Failure,Evidence as EvidenceV2}};
    pub fn source_span(loc:crate::cgx2_runtime::default_managed::model::Loc)->crate::model::SourceSpan {{crate::model::SourceSpan{{start:loc.start,end:loc.end}}}}
}}
mod execution {{ {execution} }}
fn main() {{
    let owned={{use model_v2::*; {owned}}};
    let managed={{use crate::cgx2_runtime::default_managed::model::{{*,Shape::*,Op::*,Statement::*}}; {managed}}};
    let bridges=vec![{entries}];
    #[cfg(feature="fault-injection")]
    let result=if let Some(index)=option_env!("CGX2_TEST_FAIL_RESERVE").and_then(|n|n.parse::<usize>().ok()) {{
        match crate::cgx2_runtime::Session::try_new_with_host_failure_at_reserve(index) {{
            Ok(s)=>execution::run_with_session(&owned,&managed,&bridges,s),
            Err(e)=>{{let loc=owned.functions[owned.entry].span;eprintln!("CGR080: session allocation failed: {{e:?}} at {{}}..{{}}",loc.start,loc.end);std::process::exit(1)}}
        }}
    }} else {{execution::run(&owned,&managed,&bridges)}};
    #[cfg(not(feature="fault-injection"))]
    let result=execution::run(&owned,&managed,&bridges);
    if std::env::var_os("CGX2_OWNED_EVIDENCE").is_some(){{eprintln!("CGX2_OWNED_EVIDENCE {{}}",result.owned.json_v2());}}
    if std::env::var_os("CGX2_MANAGED_EVIDENCE").is_some(){{eprintln!("CGX2_MANAGED_EVIDENCE {{}}",result.managed.json(&result.memory));}}
    match result.result{{Ok(v)=>{{let text=v.to_string();if !text.is_empty(){{println!("{{text}}");}}}},Err(e)=>{{eprintln!("{{}}: {{}} at {{}}..{{}}",e.code,e.message,e.span.start,e.span.end);std::process::exit(1)}}}}
}}
"#,
        program.source_sha256(),
        program.analysis_sha256()
    ))
}
