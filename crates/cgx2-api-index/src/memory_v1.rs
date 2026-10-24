//! Additive authenticated concrete memory/effect index; old API schemas stay readable.
use crate::IndexError;
use cgx2_sema::default_managed::ManagedProgramV1;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiMemoryIndexV1 {
    json: String,
}
impl ApiMemoryIndexV1 {
    pub fn render_json(&self) -> &str {
        &self.json
    }
}
pub fn build_memory_index(program: &ManagedProgramV1) -> Result<ApiMemoryIndexV1, IndexError> {
    program.authenticate().map_err(IndexError::InvalidModule)?;
    let p = program.program();
    let functions=p.functions.iter().map(|f|{
        let effects=f.behavioral_effects.iter().map(|i|format!("{:?}",p.strings[*i])).collect::<Vec<_>>().join(",");
        let caps=f.capabilities.iter().map(|i|format!("{:?}",p.strings[*i])).collect::<Vec<_>>().join(",");
        let parameters=f.parameter_types.iter().map(|t|format!("{:?}",cgx2_sema::type_key(&program.types()[*t]))).collect::<Vec<_>>().join(",");
        let e=f.effects;
        format!("{{\"name\":{:?},\"parameters\":[{}],\"return_type\":{:?},\"behavioral_effects\":[{}],\"capabilities\":[{}],\"memory_effects_v1\":{{\"may_allocate_managed\":{},\"may_auto_collect_at_allocation\":{},\"may_explicitly_collect\":{},\"may_allocate_root_bookkeeping\":{}}}}}",p.strings[f.name],parameters,cgx2_sema::type_key(&program.types()[f.result]),effects,caps,e.may_allocate_managed,e.may_auto_collect_at_allocation,e.may_explicitly_collect,e.may_allocate_root_bookkeeping)
    }).collect::<Vec<_>>().join(",");
    Ok(ApiMemoryIndexV1{json:format!("{{\"schema_version\":1,\"summary_scope\":\"concrete-specializations\",\"absent_summary\":\"unknown\",\"source_sha256\":{:?},\"analysis_sha256\":{:?},\"functions\":[{}]}}",program.source_sha256(),program.analysis_sha256(),functions)})
}
