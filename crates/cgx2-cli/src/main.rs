mod diagnostics;

use std::{env, fs, path::Path, process::ExitCode};

use cgx2_cli::{check_source, run_source, Value};
use cgx2_syntax::{format_module, parse_module};

use diagnostics::{CliDiagnostic, DiagnosticFormat};

const HELP: &str = "usage: cgx2 [--diagnostic-format=json] check FILE.cg | cgx2 [--diagnostic-format=json] run FILE.cg [-- ARGS] | cgx2 FILE.cg [-- ARGS] | cgx2 fmt FILE.cg\ncheck validates syntax and baseline names/types/mutability only; ownership, effects, and capabilities are not checked";

enum Command {
    Check { path: String },
    Run { path: String, args: Vec<String> },
    Fmt { path: String },
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if matches!(args.as_slice(), [arg] if arg == "--help" || arg == "-h") {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }
    let format = DiagnosticFormat::from_args(&args);
    match execute(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(errors) => {
            for error in errors {
                eprintln!("{}", error.render(format));
            }
            ExitCode::FAILURE
        }
    }
}

fn execute(args: Vec<String>) -> Result<(), Vec<CliDiagnostic>> {
    let command = parse_args(args).map_err(|error| vec![error])?;
    let path = match &command {
        Command::Check { path } | Command::Run { path, .. } | Command::Fmt { path } => path,
    };
    let source = fs::read_to_string(path).map_err(|error| {
        vec![CliDiagnostic::new(
            "CGC002",
            format!("cannot read {path}: {error}"),
            path.clone(),
            None,
        )]
    })?;
    match command {
        Command::Check { path } => {
            check_source(&source).map_err(|errors| {
                errors
                    .into_iter()
                    .map(|error| CliDiagnostic::from_check(&path, error))
                    .collect::<Vec<_>>()
            })?;
        }
        Command::Run { path, args } => {
            check_source(&source).map_err(|errors| {
                errors
                    .into_iter()
                    .map(|error| CliDiagnostic::from_check(&path, error))
                    .collect::<Vec<_>>()
            })?;
            let result = run_source(&source, &args)
                .map_err(|error| vec![CliDiagnostic::from_run(&path, error)])?;
            if result.value != Value::Unit {
                println!("{}", result.value);
            }
        }
        Command::Fmt { path } => {
            let module = parse_module(&source).map_err(|errors| {
                errors
                    .into_iter()
                    .map(|error| {
                        CliDiagnostic::new(
                            error.code,
                            error.message,
                            path.clone(),
                            Some(error.span),
                        )
                    })
                    .collect::<Vec<_>>()
            })?;
            print!("{}", format_module(&module));
        }
    }
    Ok(())
}

fn parse_args(args: Vec<String>) -> Result<Command, CliDiagnostic> {
    let usage = || CliDiagnostic::new("CGC001", HELP.into(), "<command>".into(), None);
    let mut filtered = Vec::new();
    let mut json = false;
    let mut program_args = false;
    for arg in args {
        if arg == "--" {
            program_args = true;
        }
        if !program_args && arg == "--diagnostic-format=json" {
            if json {
                return Err(usage());
            }
            json = true;
        } else {
            filtered.push(arg);
        }
    }
    let (mode, rest) = match filtered.first().map(String::as_str) {
        Some("check") => ("check", &filtered[1..]),
        Some("run") => ("run", &filtered[1..]),
        Some("fmt") if !json => ("fmt", &filtered[1..]),
        Some(path) if path.ends_with(".cg") => ("run", &filtered[..]),
        _ => return Err(usage()),
    };
    let Some(path) = rest.first() else {
        return Err(usage());
    };
    if Path::new(path).extension().and_then(|ext| ext.to_str()) != Some("cg") {
        return Err(usage());
    }
    if mode == "check" || mode == "fmt" {
        if rest.len() != 1 {
            return Err(usage());
        }
        return Ok(if mode == "check" {
            Command::Check { path: path.clone() }
        } else {
            Command::Fmt { path: path.clone() }
        });
    }
    let args = match rest.get(1).map(String::as_str) {
        None => Vec::new(),
        Some("--") => rest[2..].to_vec(),
        _ => return Err(usage()),
    };
    Ok(Command::Run {
        path: path.clone(),
        args,
    })
}
