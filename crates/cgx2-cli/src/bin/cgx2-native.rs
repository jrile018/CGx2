//! Native package process: linked without the Python bridge or libpython.
#[path = "../native.rs"]
mod native;
#[path = "../native_eval.rs"]
mod native_eval;

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mode = args.next();
    let root = args.next();
    let rest: Vec<_> = args.collect();
    let json = rest.iter().any(|arg| arg == "--diagnostic-format=json");
    let program_args = rest
        .iter()
        .position(|arg| arg == "--")
        .map_or_else(Vec::new, |index| rest[index + 1..].to_vec());
    let (Some(mode), Some(root)) = (mode, root) else {
        eprintln!("CGN001: native helper requires check|run PACKAGE");
        return ExitCode::FAILURE;
    };
    let root = Path::new(&root);
    let result = match mode.as_str() {
        "check" => native::check_native_package(root).map(|()| None),
        "run" => native::run_native_package_i64(root, &program_args).map(Some),
        _ => Err(vec![native::NativeDiagnostic {
            code: "CGN001".into(),
            message: "native helper requires check|run".into(),
            file: root.into(),
            span: None,
        }]),
    };
    match result {
        Ok(value) => {
            if let Some(value) = value {
                println!("{value}");
            }
            ExitCode::SUCCESS
        }
        Err(errors) => {
            for error in errors {
                if json {
                    let span = error.span.map_or(
                        serde_json::Value::Null,
                        |span| serde_json::json!({"start": span.start, "end": span.end}),
                    );
                    println_error_json(&error, span);
                } else if let Some(span) = error.span {
                    eprintln!(
                        "{}: {}:{}..{}: {}",
                        error.code,
                        error.file.display(),
                        span.start,
                        span.end,
                        error.message
                    );
                } else {
                    eprintln!(
                        "{}: {}: {}",
                        error.code,
                        error.file.display(),
                        error.message
                    );
                }
            }
            ExitCode::FAILURE
        }
    }
}

fn println_error_json(error: &native::NativeDiagnostic, span: serde_json::Value) {
    eprintln!(
        "{}",
        serde_json::json!({
            "code": error.code,
            "severity": "error",
            "message": error.message,
            "file": error.file.display().to_string(),
            "span": span,
        })
    );
}
