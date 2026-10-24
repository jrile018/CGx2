use crate::ApiIndex;

impl ApiIndex {
    /// Render schema version 1 as compact UTF-8 JSON.
    pub fn render_json(&self) -> String {
        let mut out = String::from("{\"schema_version\":");
        out.push_str(&self.schema_version.to_string());
        out.push_str(",\"module\":");
        if let Some(module) = &self.module {
            push_json_string(&mut out, module);
        } else {
            out.push_str("null");
        }
        if self.schema_version >= 7 {
            out.push_str(",\"instantiations\":[");
            for (i, instance) in self.instantiations.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str("{\"owner\":");
                push_json_string(&mut out, &instance.owner);
                out.push_str(",\"callee\":");
                push_json_string(&mut out, &instance.callee);
                for (key, values) in [
                    ("type_arguments", &instance.type_arguments),
                    ("parameters", &instance.parameters),
                ] {
                    out.push_str(&format!(",\"{key}\":["));
                    for (j, v) in values.iter().enumerate() {
                        if j > 0 {
                            out.push(',');
                        }
                        push_json_string(&mut out, v);
                    }
                    out.push(']');
                }
                out.push_str(",\"return_type\":");
                push_json_string(&mut out, &instance.return_type);
                out.push_str(&format!(
                    ",\"span\":{{\"start\":{},\"end\":{}}}}}",
                    instance.span.start, instance.span.end
                ));
            }
            out.push(']');
        }
        out.push_str(",\"functions\":[");
        for (index, function) in self.functions.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str("{\"name\":");
            push_json_string(&mut out, &function.name);
            out.push_str(",\"parameters\":[");
            for (parameter_index, parameter) in function.parameters.iter().enumerate() {
                if parameter_index > 0 {
                    out.push(',');
                }
                out.push_str("{\"name\":");
                push_json_string(&mut out, &parameter.name);
                out.push_str(",\"ty\":");
                push_json_string(&mut out, &parameter.ty);
                out.push('}');
            }
            out.push_str("],\"return_type\":");
            push_json_string(&mut out, &function.return_type);
            out.push_str(",\"span\":{\"start\":");
            out.push_str(&function.span.start.to_string());
            out.push_str(",\"end\":");
            out.push_str(&function.span.end.to_string());
            out.push('}');
            if self.schema_version >= 6 {
                out.push_str(",\"type_parameters\":");
                push_parameters(&mut out, &function.type_parameters);
                out.push_str(",\"loop_contracts\":[");
                for (i, contract) in function.loop_contracts.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str("{\"kind\":");
                    push_json_string(&mut out, &contract.kind);
                    out.push_str(",\"expression\":");
                    push_json_string(&mut out, &contract.expression);
                    out.push_str(",\"status\":");
                    push_json_string(&mut out, &contract.status);
                    out.push_str(",\"span\":");
                    push_span(&mut out, contract.span);
                    out.push('}');
                }
                out.push(']');
                out.push_str(",\"parameter_types\":[");
                for (i, ty) in function.parameter_types.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    push_type(&mut out, ty);
                }
                out.push_str("],\"result_type\":");
                if let Some(ty) = &function.result_type {
                    push_type(&mut out, ty);
                } else {
                    out.push_str("null");
                }
            }
            if self.schema_version >= 2 {
                out.push_str(",\"effects\":");
                push_json_array(&mut out, &function.effects);
                out.push_str(",\"effects_status\":");
                push_json_string(&mut out, &function.effects_status);
                out.push_str(",\"capabilities\":");
                push_json_array(&mut out, &function.capabilities);
                out.push_str(",\"capabilities_status\":");
                push_json_string(&mut out, &function.capabilities_status);
                for (name, contracts) in [
                    ("requires", &function.requires),
                    ("ensures", &function.ensures),
                ] {
                    out.push_str(",\"");
                    out.push_str(name);
                    out.push_str("\":[");
                    for (i, contract) in contracts.iter().enumerate() {
                        if i > 0 {
                            out.push(',');
                        }
                        out.push_str("{\"expression\":");
                        push_json_string(&mut out, &contract.expression);
                        out.push_str(",\"status\":");
                        push_json_string(&mut out, &contract.status);
                        out.push_str(",\"span\":{\"start\":");
                        out.push_str(&contract.span.start.to_string());
                        out.push_str(",\"end\":");
                        out.push_str(&contract.span.end.to_string());
                        out.push_str("}}");
                    }
                    out.push(']');
                }
            }
            if self.schema_version >= 3 {
                out.push_str(",\"foreign\":");
                if let Some(foreign) = &function.foreign {
                    out.push_str("{\"route\":");
                    push_json_string(&mut out, &foreign.route);
                    out.push_str(",\"target\":");
                    push_json_string(&mut out, &foreign.target);
                    out.push_str(",\"ownership\":");
                    push_json_string(&mut out, &foreign.ownership);
                    out.push_str(",\"runtime\":");
                    push_json_string(&mut out, &foreign.runtime);
                    if self.schema_version >= 4 {
                        out.push_str(",\"unsafe\":");
                        out.push_str(if foreign.unsafe_call { "true" } else { "false" });
                        out.push_str(",\"binding_id\":");
                        if let Some(id) = &foreign.binding_id {
                            push_json_string(&mut out, id);
                        } else {
                            out.push_str("null");
                        }
                        if let Some(profile) = &foreign.profile {
                            out.push_str(",\"profile\":");
                            push_json_string(&mut out, profile);
                        }
                    }
                    out.push('}');
                } else {
                    out.push_str("null");
                }
            }
            out.push('}');
        }
        out.push(']');
        if self.schema_version >= 5 {
            out.push_str(",\"records\":[");
            for (index, record) in self.records.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str("{\"name\":");
                push_json_string(&mut out, &record.name);
                if self.schema_version >= 6 {
                    out.push_str(",\"identity\":");
                    push_json_string(&mut out, &record.identity);
                    out.push_str(",\"type_parameters\":");
                    push_parameters(&mut out, &record.type_parameters);
                    out.push_str(",\"field_types\":[");
                    for (i, ty) in record.field_types.iter().enumerate() {
                        if i > 0 {
                            out.push(',');
                        }
                        push_type(&mut out, ty);
                    }
                    out.push(']');
                }
                out.push_str(",\"fields\":[");
                for (field_index, field) in record.fields.iter().enumerate() {
                    if field_index > 0 {
                        out.push(',');
                    }
                    out.push_str("{\"name\":");
                    push_json_string(&mut out, &field.name);
                    out.push_str(",\"ty\":");
                    push_json_string(&mut out, &field.ty);
                    out.push('}');
                }
                out.push_str("],\"span\":{\"start\":");
                out.push_str(&record.span.start.to_string());
                out.push_str(",\"end\":");
                out.push_str(&record.span.end.to_string());
                out.push_str("}}");
            }
            out.push(']');
        }
        if self.schema_version >= 6 {
            out.push_str(",\"semantics_status\":");
            push_json_string(&mut out, &self.semantics_status);
            out.push_str(",\"variants\":[");
            for (i, variant) in self.variants.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str("{\"name\":");
                push_json_string(&mut out, &variant.name);
                out.push_str(",\"identity\":");
                push_json_string(&mut out, &variant.identity);
                out.push_str(",\"type_parameters\":");
                push_parameters(&mut out, &variant.type_parameters);
                out.push_str(",\"span\":");
                push_span(&mut out, variant.span);
                out.push_str(",\"cases\":[");
                for (j, case) in variant.cases.iter().enumerate() {
                    if j > 0 {
                        out.push(',');
                    }
                    out.push_str("{\"name\":");
                    push_json_string(&mut out, &case.name);
                    out.push_str(",\"span\":");
                    push_span(&mut out, case.span);
                    out.push_str(",\"payloads\":[");
                    for (k, ty) in case.payloads.iter().enumerate() {
                        if k > 0 {
                            out.push(',');
                        }
                        push_type(&mut out, ty);
                    }
                    out.push_str("]}");
                }
                out.push_str("]}");
            }
            out.push(']');
        }
        out.push('}');
        out
    }

    /// Render one public signature per line, with half-open byte spans.
    pub fn render_text(&self) -> String {
        let mut out = format!(
            "module {} [{}]\n",
            self.module.as_deref().unwrap_or("<unnamed>"),
            self.semantics_status
        );
        for instance in &self.instantiations {
            out.push_str(&format!(
                "instantiate {}<{}>({}) -> {} [checked] [{}, {})\n",
                instance.callee,
                instance.type_arguments.join(", "),
                instance.parameters.join(", "),
                instance.return_type,
                instance.span.start,
                instance.span.end
            ));
        }
        for variant in &self.variants {
            out.push_str(&format!(
                "variant {}{} [{}, {})\n",
                variant.identity,
                text_type_parameters(&variant.type_parameters),
                variant.span.start,
                variant.span.end
            ));
            for case in &variant.cases {
                out.push_str(&format!(
                    "    {}({})\n",
                    case.name,
                    case.payloads
                        .iter()
                        .map(text_type)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        for record in &self.records {
            out.push_str(&format!(
                "record {}{} [{}, {})\n",
                record.identity,
                text_type_parameters(&record.type_parameters),
                record.span.start,
                record.span.end
            ));
            for (index, field) in record.fields.iter().enumerate() {
                let ty = record
                    .field_types
                    .get(index)
                    .map_or_else(|| field.ty.clone(), text_type);
                out.push_str(&format!("    {}: {}\n", field.name, ty));
            }
        }
        for function in &self.functions {
            out.push_str("pub fn ");
            out.push_str(&function.name);
            out.push_str(&text_type_parameters(&function.type_parameters));
            out.push('(');
            for (index, parameter) in function.parameters.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                out.push_str(&parameter.name);
                out.push_str(": ");
                if let Some(ty) = function.parameter_types.get(index) {
                    out.push_str(&text_type(ty));
                } else {
                    out.push_str(&parameter.ty);
                }
            }
            out.push_str(") -> ");
            if let Some(ty) = &function.result_type {
                out.push_str(&text_type(ty));
            } else {
                out.push_str(&function.return_type);
            }
            if self.schema_version >= 2 {
                out.push_str(" effects(");
                out.push_str(&function.effects.join(", "));
                out.push_str(") [");
                out.push_str(&function.effects_status);
                out.push(']');
                out.push_str(" caps(");
                out.push_str(&function.capabilities.join(", "));
                out.push_str(") [");
                out.push_str(&function.capabilities_status);
                out.push(']');
            }
            out.push_str(" [");
            out.push_str(&function.span.start.to_string());
            out.push_str(", ");
            out.push_str(&function.span.end.to_string());
            out.push_str(")\n");
            if self.schema_version >= 2 {
                for contract in &function.requires {
                    out.push_str("    requires ");
                    out.push_str(&contract.expression);
                    out.push_str(&format!(" [{}]\n", contract.status));
                }
                for contract in &function.ensures {
                    out.push_str("    ensures ");
                    out.push_str(&contract.expression);
                    out.push_str(&format!(" [{}]\n", contract.status));
                }
            }
            for contract in &function.loop_contracts {
                out.push_str(&format!(
                    "    {} {} [{}] [{}, {})\n",
                    contract.kind,
                    contract.expression,
                    contract.status,
                    contract.span.start,
                    contract.span.end
                ));
            }
            if let Some(foreign) = &function.foreign {
                out.push_str("    foreign route=");
                out.push_str(&foreign.route);
                out.push_str(" target=");
                out.push_str(&foreign.target);
                out.push_str(" ownership=");
                out.push_str(&foreign.ownership);
                out.push_str(" runtime=");
                out.push_str(&foreign.runtime);
                if foreign.unsafe_call {
                    out.push_str(" unsafe=true");
                }
                if let Some(id) = &foreign.binding_id {
                    out.push_str(" binding_id=");
                    out.push_str(id);
                }
                if let Some(profile) = &foreign.profile {
                    out.push_str(" profile=");
                    out.push_str(profile);
                }
                out.push('\n');
            }
        }
        out
    }
}

fn text_type(ty: &crate::ApiType) -> String {
    let mut out = ty.identity.clone();
    if !ty.arguments.is_empty() {
        out.push('<');
        out.push_str(
            &ty.arguments
                .iter()
                .map(text_type)
                .collect::<Vec<_>>()
                .join(", "),
        );
        out.push('>');
    }
    out
}

fn text_type_parameters(parameters: &[crate::ApiTypeParameter]) -> String {
    if parameters.is_empty() {
        return String::new();
    }
    let declarations = parameters
        .iter()
        .map(|parameter| {
            if parameter.constraints.is_empty() {
                parameter.name.clone()
            } else {
                format!("{}: {}", parameter.name, parameter.constraints.join(" + "))
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("<{declarations}>")
}

fn push_json_array(out: &mut String, values: &[String]) {
    out.push('[');
    for (i, value) in values.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_string(out, value);
    }
    out.push(']');
}

fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch <= '\u{001f}' => {
                out.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}

fn push_span(out: &mut String, span: cgx2_syntax::Span) {
    out.push_str(&format!(
        "{{\"start\":{},\"end\":{}}}",
        span.start, span.end
    ));
}
fn push_type(out: &mut String, ty: &crate::ApiType) {
    out.push_str("{\"identity\":");
    push_json_string(out, &ty.identity);
    out.push_str(",\"arguments\":[");
    for (i, arg) in ty.arguments.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_type(out, arg);
    }
    out.push_str("],\"span\":");
    push_span(out, ty.span);
    out.push('}');
}
fn push_parameters(out: &mut String, parameters: &[crate::ApiTypeParameter]) {
    out.push('[');
    for (i, p) in parameters.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"name\":");
        push_json_string(out, &p.name);
        out.push_str(",\"constraints\":");
        push_json_array(out, &p.constraints);
        out.push_str(",\"span\":");
        push_span(out, p.span);
        out.push('}');
    }
    out.push(']');
}
