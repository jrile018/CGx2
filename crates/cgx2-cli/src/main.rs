use std::{env, fs, path::Path, process::ExitCode};

use cgx2_cli::{run_source, RunError, Value};
use cgx2_syntax::{format_module, parse_module, Span};

enum Command {
    Run { path: String, args: Vec<String> },
    Fmt { path: String },
}

fn main() -> ExitCode {
    match execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, message)) => {
            eprintln!("{code}: {message}");
            ExitCode::FAILURE
        }
    }
}

fn execute() -> Result<(), (&'static str, String)> {
    let command = parse_args(env::args().skip(1).collect())?;
    let path = match &command {
        Command::Run { path, .. } | Command::Fmt { path } => path.clone(),
    };
    let source = fs::read_to_string(&path)
        .map_err(|error| ("CGC002", format!("cannot read {}: {error}", path)))?;
    match command {
        Command::Run { args, .. } => {
            let result =
                run_source(&source, &args).map_err(|error| format_run_error(&path, error))?;
            if result.value != Value::Unit {
                println!("{}", result.value);
            }
        }
        Command::Fmt { .. } => {
            let module = parse_module(&source).map_err(|diagnostics| {
                let first = &diagnostics[0];
                format_run_error(
                    &path,
                    RunError {
                        code: first.code,
                        message: first.message.clone(),
                        span: Some(first.span),
                    },
                )
            })?;
            print!("{}", format_module(&module));
        }
    }
    Ok(())
}

fn parse_args(args: Vec<String>) -> Result<Command, (&'static str, String)> {
    let usage = || {
        (
            "CGC001",
            "usage: cgx2 run FILE.cg [-- ARGS] | cgx2 FILE.cg [-- ARGS] | cgx2 fmt FILE.cg".into(),
        )
    };
    let (mode, rest) = match args.first().map(String::as_str) {
        Some("run") => ("run", &args[1..]),
        Some("fmt") => ("fmt", &args[1..]),
        Some(path) if path.ends_with(".cg") => ("run", &args[..]),
        _ => return Err(usage()),
    };
    let Some(path) = rest.first() else {
        return Err(usage());
    };
    if Path::new(path).extension().and_then(|ext| ext.to_str()) != Some("cg") {
        return Err(usage());
    }
    if mode == "fmt" {
        if rest.len() != 1 {
            return Err(usage());
        }
        return Ok(Command::Fmt { path: path.clone() });
    }
    let program_args = match rest.get(1).map(String::as_str) {
        None => Vec::new(),
        Some("--") => rest[2..].to_vec(),
        _ => return Err(usage()),
    };
    Ok(Command::Run {
        path: path.clone(),
        args: program_args,
    })
}

fn format_run_error(path: &str, error: RunError) -> (&'static str, String) {
    let location = match error.span {
        Some(Span { start, end }) => format!("{path}:{start}..{end}"),
        None => path.to_owned(),
    };
    (error.code, format!("{location}: {}", error.message))
}
