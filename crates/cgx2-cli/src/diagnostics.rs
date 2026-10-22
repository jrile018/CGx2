use std::fmt::Write;

use cgx2_cli::{CheckDiagnostic, RunError};
use cgx2_syntax::Span;

#[derive(Clone, Copy)]
pub(crate) enum DiagnosticFormat {
    Human,
    Json,
}

impl DiagnosticFormat {
    pub(crate) fn from_args(args: &[String]) -> Self {
        if args
            .iter()
            .take_while(|arg| arg.as_str() != "--")
            .any(|arg| arg == "--diagnostic-format=json")
        {
            Self::Json
        } else {
            Self::Human
        }
    }
}

pub(crate) struct CliDiagnostic {
    code: String,
    message: String,
    file: String,
    span: Option<Span>,
}

impl CliDiagnostic {
    pub(crate) fn new(code: &str, message: String, file: String, span: Option<Span>) -> Self {
        Self {
            code: code.into(),
            message,
            file,
            span,
        }
    }

    pub(crate) fn from_check(path: &str, error: CheckDiagnostic) -> Self {
        Self::new(&error.code, error.message, path.into(), Some(error.span))
    }

    pub(crate) fn from_run(path: &str, error: RunError) -> Self {
        Self::new(error.code, error.message, path.into(), error.span)
    }

    pub(crate) fn render(&self, format: DiagnosticFormat) -> String {
        match format {
            DiagnosticFormat::Human => {
                let location = match self.span {
                    Some(Span { start, end }) => format!("{}:{start}..{end}", self.file),
                    None => self.file.clone(),
                };
                format!("{}: {location}: {}", self.code, self.message)
            }
            DiagnosticFormat::Json => {
                let span = match self.span {
                    Some(Span { start, end }) => format!("{{\"start\":{start},\"end\":{end}}}"),
                    None => "null".into(),
                };
                format!(
                    "{{\"code\":{},\"severity\":\"error\",\"message\":{},\"file\":{},\"span\":{span}}}",
                    json_string(&self.code),
                    json_string(&self.message),
                    json_string(&self.file),
                )
            }
        }
    }
}

fn json_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character < ' ' => {
                write!(&mut output, "\\u{:04x}", character as u32).unwrap();
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}
