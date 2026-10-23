use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::process::ExitCode;

use cgx2_effect_index::{index_source, Diagnostic, MAX_SOURCE_BYTES};
use cgx2_syntax::Span;

fn main() -> ExitCode {
    match run() {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            let _ = writeln!(std::io::stderr(), "{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<String, String> {
    let mut args = env::args().skip(1);
    let mut file = None;
    let mut format = "text";
    let mut syntax_only = false;
    let mut package = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--format" => {
                let value = args.next().ok_or_else(usage)?;
                if value != "text" && value != "json" {
                    return Err(usage());
                }
                format = if value == "json" { "json" } else { "text" };
            }
            "--syntax-only" => syntax_only = true,
            "--package" => package = true,
            _ if arg.starts_with('-') || file.is_some() => return Err(usage()),
            _ => file = Some(arg),
        }
    }
    let file = file.ok_or_else(usage)?;
    if package {
        if syntax_only {
            return Err("--package requires checked sources".into());
        }
        let index = cgx2_effect_index::package_v1::index_package(std::path::Path::new(&file))
            .map_err(|errors| format!("{errors:?}"))?;
        return if format == "json" {
            index.to_json().map(|s| format!("{s}\n"))
        } else {
            index.to_text()
        }
        .map_err(|errors| format!("{errors:?}"));
    }
    let mut input = Vec::new();
    File::open(&file)
        .map_err(|err| format!("CGI002 input {file}:0:0 [0..0): {err}"))?
        .take(MAX_SOURCE_BYTES as u64 + 1)
        .read_to_end(&mut input)
        .map_err(|err| format!("CGI002 input {file}:0:0 [0..0): {err}"))?;
    if input.len() > MAX_SOURCE_BYTES {
        return Err(format!(
            "CGI001 input {file}:0:0 [0..0): source exceeds {MAX_SOURCE_BYTES} byte limit"
        ));
    }
    let source = String::from_utf8(input)
        .map_err(|err| format!("CGI003 input {file}:0:0 [0..0): invalid UTF-8: {err}"))?;
    let index = index_source(&source, syntax_only).map_err(|errors| {
        errors
            .iter()
            .map(|error| display_diagnostic(&file, &source, error))
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    Ok(if format == "json" {
        format!("{}\n", index.to_json())
    } else {
        index.to_text()
    })
}

fn usage() -> String {
    "usage: cgx2-effect-index FILE.cg [--format text|json] [--syntax-only], or --package ROOT [--format text|json]".into()
}

fn display_diagnostic(file: &str, source: &str, diagnostic: &Diagnostic) -> String {
    let (line, column) = line_column(source, diagnostic.span);
    format!(
        "{} {} {file}:{line}:{column} [{}..{}): {}",
        diagnostic.code,
        diagnostic.stage,
        diagnostic.span.start,
        diagnostic.span.end,
        diagnostic.message
    )
}

fn line_column(source: &str, span: Span) -> (usize, usize) {
    let offset = span.start.min(source.len());
    let prefix = &source.as_bytes()[..offset];
    let line = prefix.iter().filter(|&&byte| byte == b'\n').count() + 1;
    let column = prefix
        .iter()
        .rposition(|&byte| byte == b'\n')
        .map_or(offset, |pos| offset - pos - 1)
        + 1;
    (line, column)
}
