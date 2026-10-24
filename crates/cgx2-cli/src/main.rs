mod diagnostics;

use std::{env, fs, path::Path, process::ExitCode};

use cgx2_api_index::build_index;
use cgx2_cli::{check_source, run_checked_source, run_checked_source_with_failure, Value};
use cgx2_syntax::{format_module, parse_module};

use diagnostics::{CliDiagnostic, DiagnosticFormat};

const HELP: &str = "usage: cgx2 [--diagnostic-format=json] check FILE.cg|PACKAGE | cgx2 [--diagnostic-format=json] run FILE.cg|PACKAGE [-- ARGS] | cgx2 FILE.cg [-- ARGS] | cgx2 api [--json] [--syntax-only] FILE.cg|PACKAGE | cgx2 build FILE.cg|PACKAGE --out BINARY [--release] [--force] [--rustc PATH] | cgx2 emit FILE.cg|PACKAGE --target rust|python --out FILE [--force] | cgx2 memory report FILE.cg [--format=json] | cgx2 memory failure-report FILE.cg | cgx2 fmt FILE.cg\ncheck validates syntax, generic constraints, copied finite values, and the bounded memory/ownership script subset; a PACKAGE directory can run checked value functions through cgx2-native; schema 2 also supports bounded pinned native C calls; memory report shows CGx2 allocations only";

enum Command {
    Check {
        path: String,
    },
    Compile {
        path: String,
        args: Vec<std::ffi::OsString>,
    },
    Api {
        path: String,
        json: bool,
        syntax_only: bool,
    },
    Run {
        path: String,
        args: Vec<String>,
    },
    MemoryReport {
        path: String,
        json: bool,
    },
    FailureReport {
        path: String,
    },
    Fmt {
        path: String,
    },
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
    let json = matches!(DiagnosticFormat::from_args(&args), DiagnosticFormat::Json);
    let command = parse_args(args).map_err(|error| vec![error])?;
    if let Command::Compile { path, args } = command {
        return cgx2c::run(args).map_err(|error| {
            vec![CliDiagnostic::new(
                &error.code,
                error.message,
                error
                    .file
                    .map_or(path, |p| p.to_string_lossy().into_owned()),
                error.span,
            )]
        });
    }
    if let Command::Api {
        path,
        json,
        syntax_only,
    } = &command
    {
        if Path::new(path).is_dir() {
            if *syntax_only {
                return Err(vec![CliDiagnostic::new(
                    "CGC001",
                    "syntax-only API metadata currently requires a source file".into(),
                    path.clone(),
                    None,
                )]);
            }
            let root = Path::new(path);
            let package = cgx2_package::load_checked_package(root).map_err(|errors| {
                errors
                    .into_iter()
                    .map(|error| {
                        CliDiagnostic::new(
                            error.code,
                            error.message,
                            error
                                .file
                                .strip_prefix(root)
                                .unwrap_or(&error.file)
                                .to_string_lossy()
                                .into_owned(),
                            error.span,
                        )
                    })
                    .collect::<Vec<_>>()
            })?;
            if *json {
                println!("{}", package.index().render_json());
            } else {
                print!("{}", package.index().render_text());
            }
            return Ok(());
        }
    }
    let path = match &command {
        Command::Compile { path, .. }
        | Command::Check { path }
        | Command::Api { path, .. }
        | Command::Run { path, .. }
        | Command::MemoryReport { path, .. }
        | Command::FailureReport { path }
        | Command::Fmt { path } => path,
    };
    if Path::new(path).is_dir()
        && Path::new(path).extension().and_then(|ext| ext.to_str()) != Some("cg")
    {
        let mode = match &command {
            Command::Check { .. } => "check",
            Command::Run { .. } => "run",
            _ => {
                return Err(vec![CliDiagnostic::new(
                    "CGC001",
                    HELP.into(),
                    path.to_string(),
                    None,
                )]);
            }
        };
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::process::CommandExt;
            let helper = env::current_exe()
                .map_err(|error| {
                    vec![CliDiagnostic::new(
                        "CGN001",
                        format!("cannot locate native helper: {error}"),
                        path.to_string(),
                        None,
                    )]
                })?
                .with_file_name("cgx2-native");
            let mut process = std::process::Command::new(helper);
            process.arg(mode).arg(path);
            if json {
                process.arg("--diagnostic-format=json");
            }
            if let Command::Run { args, .. } = &command {
                if !args.is_empty() {
                    process.arg("--").args(args);
                }
            }
            let error = process.exec();
            return Err(vec![CliDiagnostic::new(
                "CGN001",
                format!("cannot start native helper: {error}"),
                path.to_string(),
                None,
            )]);
        }
        #[cfg(not(target_os = "linux"))]
        return Err(vec![CliDiagnostic::new(
            "CGN001",
            "native C package execution requires Linux".into(),
            path.to_string(),
            None,
        )]);
    }
    let source = fs::read_to_string(path).map_err(|error| {
        vec![CliDiagnostic::new(
            "CGC002",
            format!("cannot read {path}: {error}"),
            path.clone(),
            None,
        )]
    })?;
    match command {
        Command::Compile { .. } => unreachable!("compiler command handled before source read"),
        Command::Check { path } => {
            let ordinary = parse_module(&source)
                .ok()
                .is_some_and(|m| cgx2_syntax::ordinary_owned_span(&m).is_some());
            (if ordinary {
                cgx2_cli::validate_owned_source(&source)
            } else {
                check_source(&source).map(|_| ())
            })
            .map_err(|errors| {
                errors
                    .into_iter()
                    .map(|error| CliDiagnostic::from_check(&path, error))
                    .collect::<Vec<_>>()
            })?;
        }
        Command::Api {
            path,
            json,
            syntax_only,
        } => {
            if syntax_only {
                let module = parse_module(&source).map_err(|errors| {
                    errors
                        .into_iter()
                        .map(|d| CliDiagnostic::new(d.code, d.message, path.clone(), Some(d.span)))
                        .collect::<Vec<_>>()
                })?;
                let index = cgx2_api_index::build_syntax_index(&module);
                if json {
                    println!("{}", index.render_json());
                } else {
                    print!("{}", index.render_text());
                }
                return Ok(());
            }
            let typed = check_source(&source).map_err(|errors| {
                errors
                    .into_iter()
                    .map(|error| CliDiagnostic::from_check(&path, error))
                    .collect::<Vec<_>>()
            })?;
            let module = parse_module(&source).expect("checked source parses");
            let index = build_index(&module, &typed).map_err(|error| {
                vec![CliDiagnostic::new(
                    "CGC001",
                    error.to_string(),
                    path.clone(),
                    None,
                )]
            })?;
            if json {
                println!("{}", index.render_json());
            } else {
                print!("{}", index.render_text());
            }
        }
        Command::Run { path, args } => {
            let (result, _) = run_checked_source(&source, &args).map_err(|errors| {
                errors
                    .into_iter()
                    .map(|error| CliDiagnostic::from_run(&path, error))
                    .collect::<Vec<_>>()
            })?;
            if result.value != Value::Unit {
                println!("{}", result.value);
            }
        }
        Command::MemoryReport { path, json } => {
            let (_, report) = run_checked_source(&source, &[]).map_err(|errors| {
                errors
                    .into_iter()
                    .map(|error| CliDiagnostic::from_run(&path, error))
                    .collect::<Vec<_>>()
            })?;
            let report = report.ok_or_else(|| {
                vec![CliDiagnostic::new(
                    "CGC001",
                    "memory report requires a memory script".into(),
                    path,
                    None,
                )]
            })?;
            if json {
                println!("{}", report.to_json());
            } else {
                print!("{}", report.to_text());
            }
        }
        Command::FailureReport { path } => match run_checked_source_with_failure(&source, &[]) {
            Ok((_, Some(_))) => (),
            Ok((_, None)) => {
                return Err(vec![CliDiagnostic::new(
                    "CGC001",
                    "failure report requires memory script".into(),
                    path,
                    None,
                )]);
            }
            Err(failures) => {
                let mut diagnostics = Vec::new();
                for failure in failures {
                    if let Some(report) = &failure.report {
                        let span = failure
                            .error
                            .span
                            .map(|s| serde_json::json!({"start": s.start, "end": s.end}));
                        if let Ok(memory) =
                            serde_json::from_str::<serde_json::Value>(&report.to_json())
                        {
                            let snapshot_json = |snapshot: &cgx2_cli::FailureSnapshot| {
                                serde_json::from_str::<serde_json::Value>(
                                    &snapshot.memory.to_json(),
                                )
                                .ok()
                                .map(|memory| {
                                    serde_json::json!({
                                        "rooted_handles": snapshot.rooted_handles,
                                        "rooted_edges": snapshot.rooted_edges,
                                        "runtime_rooted_handles": snapshot.runtime_rooted_handles,
                                        "runtime_root_registrations": snapshot.runtime_root_registrations,
                                        "runtime_root_census_error": snapshot.runtime_root_census_error,
                                        "memory": memory,
                                    })
                                })
                            };
                            println!(
                                "{}",
                                serde_json::json!({
                                    "failure_report_schema_version": 1,
                                    "status": "error",
                                    "diagnostic_code": failure.error.code,
                                    "file": path,
                                    "span": span,
                                    "rollback_preserved": failure.rollback_preserved,
                                    "memory": memory,
                                    "operation_failure": failure.operation_failure.as_deref().and_then(snapshot_json),
                                    "final_unwind": failure.final_unwind.as_deref().and_then(snapshot_json),
                                    "cleanup_errors": failure.cleanup_errors,
                                })
                            );
                        }
                    }
                    diagnostics.push(CliDiagnostic::from_run(&path, failure.error));
                }
                return Err(diagnostics);
            }
        },
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
    if matches!(filtered.first().map(String::as_str), Some("build" | "emit")) {
        return parse_compile(&filtered)
            .map_err(|message| CliDiagnostic::new("CGC001", message, "<command>".into(), None));
    }
    if filtered.first().map(String::as_str) == Some("api") {
        let json = filtered.iter().any(|arg| arg == "--json");
        let syntax_only = filtered.iter().any(|arg| arg == "--syntax-only");
        let paths: Vec<_> = filtered
            .iter()
            .skip(1)
            .filter(|arg| arg.as_str() != "--json" && arg.as_str() != "--syntax-only")
            .collect();
        if paths.len() != 1 {
            return Err(usage());
        }
        return Ok(Command::Api {
            path: paths[0].clone(),
            json,
            syntax_only,
        });
    }
    let (mode, rest) = match filtered.first().map(String::as_str) {
        Some("memory") if filtered.get(1).map(String::as_str) == Some("report") => {
            ("memory-report", &filtered[2..])
        }
        Some("memory") if filtered.get(1).map(String::as_str) == Some("failure-report") => {
            ("memory-failure-report", &filtered[2..])
        }
        Some("check") => ("check", &filtered[1..]),
        Some("api") => ("api", &filtered[1..]),
        Some("run") => ("run", &filtered[1..]),
        Some("fmt") if !json => ("fmt", &filtered[1..]),
        Some(path) if path.ends_with(".cg") || Path::new(path).is_dir() => ("run", &filtered[..]),
        _ => return Err(usage()),
    };
    let Some(path) = rest.first() else {
        return Err(usage());
    };
    if Path::new(path).extension().and_then(|ext| ext.to_str()) != Some("cg")
        && !Path::new(path).is_dir()
    {
        return Err(usage());
    }
    if mode == "memory-report" {
        let json = rest.len() == 2 && rest[1] == "--format=json";
        if rest.len() != 1 && !json {
            return Err(usage());
        }
        return Ok(Command::MemoryReport {
            path: path.clone(),
            json,
        });
    }
    if mode == "memory-failure-report" {
        if rest.len() != 1 || Path::new(path).is_dir() {
            return Err(usage());
        }
        return Ok(Command::FailureReport { path: path.clone() });
    }
    if mode == "api" {
        if rest.len() != 1 && !(rest.len() == 2 && rest[1] == "--json") {
            return Err(usage());
        }
        return Ok(Command::Api {
            path: path.clone(),
            json: rest.len() == 2,
            syntax_only: false,
        });
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

// The compiler remains the single owner of output validation, target checking,
// embedded runtime compilation, and publication. The product parser translates
// only its public spelling; it never invokes a checkout-relative executable.
fn parse_compile(args: &[String]) -> Result<Command, String> {
    let usage = || HELP.to_owned();
    let mut rest = Vec::new();
    let mut target = None;
    let mut index = 1;
    while index < args.len() {
        if args[index] == "--target" {
            if args[0] != "emit" || target.is_some() {
                return Err(usage());
            }
            target = Some(args.get(index + 1).ok_or_else(usage)?.as_str());
            index += 2;
        } else {
            rest.push(args[index].clone());
            // Flag values may themselves equal --target; leave their validation
            // to the compiler rather than interpreting them as product options.
            if matches!(
                args[index].as_str(),
                "--out" | "--rustc" | "--test-fail-reserve"
            ) {
                rest.push(args.get(index + 1).ok_or_else(usage)?.clone());
                index += 1;
            }
            index += 1;
        }
    }
    let mode = if args[0] == "build" {
        "build"
    } else {
        match target {
            Some("rust") => "emit-rust",
            Some("python") => "emit-python",
            Some(target) => {
                return Err(format!(
                    "unsupported emission target `{target}`; expected rust or python"
                ));
            }
            None => return Err(usage()),
        }
    };
    let path = rest.first().ok_or_else(usage)?.clone();
    if Path::new(&path).extension().and_then(|s| s.to_str()) != Some("cg")
        && !Path::new(&path).is_dir()
    {
        return Err(usage());
    }
    let mut compiler = vec![std::ffi::OsString::from(mode)];
    if Path::new(&path).is_dir() {
        compiler.push("--package".into());
    }
    compiler.extend(rest.into_iter().map(std::ffi::OsString::from));
    Ok(Command::Compile {
        path,
        args: compiler,
    })
}
