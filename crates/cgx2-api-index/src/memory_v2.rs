//! Additive ordinary-memory metadata. V1 managed indices retain their schema.
use crate::IndexError;
use cgx2_sema::ordinary_memory::MemoryProgramV1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiMemoryIndexV2 {
    json: String,
    text: String,
}
impl ApiMemoryIndexV2 {
    pub fn render_json(&self) -> &str {
        &self.json
    }
    pub fn render_text(&self) -> &str {
        &self.text
    }
}

fn string(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c <= '\u{1f}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn build_memory_index_v2(program: &MemoryProgramV1) -> Result<ApiMemoryIndexV2, IndexError> {
    program.authenticate().map_err(IndexError::InvalidModule)?;
    let p = &program.runtime().program;
    let declarations = program.signatures().iter().map(|s| {
        let regions = s.region_parameters.iter().map(|s| string(s)).collect::<Vec<_>>().join(",");
        let parameters = s.parameters.iter().map(|t| string(&t.to_string())).collect::<Vec<_>>().join(",");
        let result = s.result.as_ref().map_or_else(|| "null".into(), |t| string(&t.to_string()));
        format!("{{\"name\":{},\"region_parameters\":[{regions}],\"parameters\":[{parameters}],\"result\":{result}}}", string(&s.name))
    }).collect::<Vec<_>>().join(",");
    let functions = p.functions.iter().zip(program.effects()).filter(|(f, _)| !p.strings[f.name].starts_with('$')).map(|(f, e)| {
        let parameters = f.parameter_types.iter().map(|t| string(&cgx2_sema::type_key(&program.types()[*t]))).collect::<Vec<_>>().join(",");
        let behavior = f.behavioral_effects.iter().map(|i| string(&p.strings[*i])).collect::<Vec<_>>().join(",");
        let capabilities = f.capabilities.iter().map(|i| string(&p.strings[*i])).collect::<Vec<_>>().join(",");
        let effects = format!("{{\"may_allocate_managed\":{},\"may_auto_collect_at_allocation\":{},\"may_explicitly_collect\":{},\"may_allocate_root_bookkeeping\":{},\"may_enter_session\":{},\"may_create_region\":{},\"may_enter_guard\":{},\"may_allocate_owned\":{},\"may_transfer_graph\":{}}}", e.may_allocate_managed, e.may_auto_collect_at_allocation, e.may_explicitly_collect, e.may_allocate_root_bookkeeping, e.may_enter_session, e.may_create_region, e.may_enter_guard, e.may_allocate_owned, e.may_transfer_graph);
        format!("{{\"name\":{},\"parameters\":[{parameters}],\"return_type\":{},\"behavioral_effects\":[{behavior}],\"capabilities\":[{capabilities}],\"memory_effects_v2\":{effects}}}", string(&p.strings[f.name]), string(&cgx2_sema::type_key(&program.types()[f.result])))
    }).collect::<Vec<_>>().join(",");
    let source = program.source_sha256();
    let source = if source.is_empty() {
        "null".into()
    } else {
        string(&source)
    };
    let mut text = format!(
        "ordinary memory API v2 ({})\n",
        program.provenance_v1().kind()
    );
    for s in program.signatures() {
        let regions = if s.region_parameters.is_empty() {
            String::new()
        } else {
            format!(
                "<{}>",
                s.region_parameters
                    .iter()
                    .map(|r| format!("{r}: memory.RegionName"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        text.push_str(&format!(
            "fn {}{}({}) -> {}\n",
            s.name,
            regions,
            s.parameters
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
            s.result
                .as_ref()
                .map_or_else(|| "Unit".into(), ToString::to_string)
        ));
    }
    Ok(ApiMemoryIndexV2 { text, json: format!("{{\"schema_version\":2,\"memory_model_version\":1,\"summary_scope\":\"concrete-specializations\",\"declaration_types\":\"original-source-syntax\",\"absent_summary\":\"unknown\",\"provenance_schema_version\":1,\"provenance_kind\":{},\"inputs_sha256\":{},\"source_sha256\":{source},\"analysis_sha256\":{},\"entry_policy\":{},\"declarations\":[{declarations}],\"functions\":[{functions}]}}", string(program.provenance_v1().kind()), string(&program.provenance_v1().identity_sha256()), string(program.analysis_sha256()), string(match program.entry_policy() { cgx2_sema::ordinary_memory::EntryPolicyV1::Implicit => "implicit", cgx2_sema::ordinary_memory::EntryPolicyV1::Recoverable => "recoverable" })) })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provenance_and_effect_versions_are_explicit() {
        let source = "fn scalar() -> i64:\n    42\nfn main() -> i64 effects(ManagedAlloc):\n    match memory.try_no_gc(budget: 4096):\n        Result.Ok(guard):\n            scalar()\n        Result.Err(error):\n            -1\n";
        let source = cgx2_sema::ordinary_memory::compile_source(source).unwrap();
        let json = build_memory_index_v2(&source).unwrap();
        assert!(json.render_json().contains("\"schema_version\":2"));
        assert!(json
            .render_json()
            .contains("\"provenance_kind\":\"source-v1\""));
        assert!(json.render_json().contains("\"may_enter_guard\":true"));
        assert!(json.render_json().contains("\"may_enter_guard\":false"));
        assert!(!json.render_json().contains("$cgx2$"));
    }
    #[test]
    fn json_strings_escape_all_control_characters() {
        assert_eq!(
            string("\0\u{1f}\n\t\r\"\\é"),
            "\"\\u0000\\u001f\\n\\t\\r\\\"\\\\é\""
        );
    }
}
