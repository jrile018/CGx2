use crate::EmitDiagnostic;
use cgx2_sema::ordinary_memory::MemoryProgramV1;
use cgx2_sema::ordinary_memory::{EntryPolicyV1, MemoryOpV1};

pub fn emit_memory_rust(program: &MemoryProgramV1) -> Result<String, Vec<EmitDiagnostic>> {
    program.authenticate().map_err(|ds| {
        ds.into_iter()
            .map(|d| EmitDiagnostic {
                code: d.code,
                message: d.message,
                span: d.span,
            })
            .collect::<Vec<_>>()
    })?;
    let runtime = program.runtime();
    let operations=runtime.operations.iter().map(|(id,op)|{
        let op=match op {
            MemoryOpV1::RegionEntry{manual,arguments,limit,trigger}=>format!("RegionEntry {{manual:{manual},arguments:vec!{arguments:?},limit:{limit},trigger:{trigger:?}}}"),
            _=>format!("{op:?}"),
        };
        format!("({id},{op})")
    }).collect::<Vec<_>>().join(",");
    let entry = match runtime.entry_policy {
        EntryPolicyV1::Implicit => "Implicit",
        EntryPolicyV1::Recoverable => "Recoverable",
    };
    Ok(format!(
        r#"// CGx2 authenticated ordinary-memory source {source} analysis {digest}
{embedded}
fn main() {{
    use cgx2_runtime::ordinary_memory::MemoryOpV1::*;
    use cgx2_runtime::default_managed::model::{{*,Op::*,Shape::*,Statement::*}};
    let program=cgx2_runtime::ordinary_memory::MemoryProgramV1 {{ program:{data},operations:[{operations}].into_iter().collect(),entry_policy:cgx2_runtime::ordinary_memory::EntryPolicyV1::{entry} }};
    let args=std::env::args().skip(1).collect::<Vec<_>>();
    #[cfg(feature="fault-injection")]
    let execution=if let Some(index)=option_env!("CGX2_TEST_FAIL_RESERVE").and_then(|n|n.parse::<usize>().ok()) {{cgx2_runtime::ordinary_memory::run_with_host_failure_v1(&program,&args,index)}} else {{cgx2_runtime::ordinary_memory::run(&program,&args)}};
    #[cfg(not(feature="fault-injection"))]
    let execution=cgx2_runtime::ordinary_memory::run(&program,&args);
    if std::env::var_os("CGX2_MEMORY_EVIDENCE").is_some() {{
        match (&execution.evidence,&execution.memory) {{
            (Some(e),Some(m))=>eprintln!("CGX2_MEMORY_EVIDENCE {{}}",e.json(m)),
            _=>eprintln!("CGX2_MEMORY_EVIDENCE {{{{\"session_entered\":false,\"metrics\":null}}}}"),
        }}
    }}
    match execution.result {{Ok(value)=>{{if !matches!(value,cgx2_runtime::default_managed::Snapshot::Unit) {{println!("{{value}}");}}}},Err(error)=>{{eprintln!("{{}}: {{}} {{}}..{{}}",error.code,error.message,error.loc.start,error.loc.end);std::process::exit(1)}}}}
}}
"#,
        source = program.source_sha256(),
        digest = program.analysis_sha256(),
        embedded = super::default_managed::embedded_runtime(),
        data = runtime.program.rust()
    ))
}
