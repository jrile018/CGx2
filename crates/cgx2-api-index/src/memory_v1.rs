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
    let source = if program.source_sha256().is_empty() {
        "null".to_owned()
    } else {
        format!("{:?}", program.source_sha256())
    };
    Ok(ApiMemoryIndexV1{json:format!("{{\"schema_version\":1,\"summary_scope\":\"concrete-specializations\",\"absent_summary\":\"unknown\",\"provenance_schema_version\":1,\"provenance_kind\":{:?},\"inputs_sha256\":{:?},\"source_sha256\":{},\"analysis_sha256\":{:?},\"functions\":[{}]}}",program.provenance_v1().kind(),program.provenance_v1().identity_sha256(),source,program.analysis_sha256(),functions)})
}

#[cfg(test)]
mod auth_tests {
    #[test]
    fn original_source_hash_is_not_claimed_by_ast_only_index() {
        let source = "fn main() -> String:\n \"héllo🙂\"\n";
        let source_program = cgx2_sema::default_managed::compile_source(source).unwrap();
        let source_json = super::build_memory_index(&source_program).unwrap();
        assert!(source_json
            .render_json()
            .contains(source_program.source_sha256()));
        assert!(source_json
            .render_json()
            .contains("\"provenance_kind\":\"source-v1\""));
        let program =
            cgx2_sema::default_managed::compile_ast(&cgx2_syntax::parse_module(source).unwrap())
                .unwrap();
        let json = super::build_memory_index(&program).unwrap();
        assert!(json.render_json().contains("\"source_sha256\":null"));
        assert!(json
            .render_json()
            .contains("\"provenance_kind\":\"ast-only-v1\""));
        assert!(!json.render_json().contains(source_program.source_sha256()));
    }
}
