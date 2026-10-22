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
            out.push_str("}}");
        }
        out.push_str("]}");
        out
    }

    /// Render one public signature per line, with half-open byte spans.
    pub fn render_text(&self) -> String {
        let mut out = format!("module {}\n", self.module.as_deref().unwrap_or("<unnamed>"));
        for function in &self.functions {
            out.push_str("pub fn ");
            out.push_str(&function.name);
            out.push('(');
            for (index, parameter) in function.parameters.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                out.push_str(&parameter.name);
                out.push_str(": ");
                out.push_str(&parameter.ty);
            }
            out.push_str(") -> ");
            out.push_str(&function.return_type);
            out.push_str(" [");
            out.push_str(&function.span.start.to_string());
            out.push_str(", ");
            out.push_str(&function.span.end.to_string());
            out.push_str(")\n");
        }
        out
    }
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
