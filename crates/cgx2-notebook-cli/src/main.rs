//! File command for clean, pure-scalar CGx2 notebook execution.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use cgx2_notebook_format::{
    export_document, markdown_image_digests, open_notebook_bytes, parse_self_contained,
    referenced_asset_digests, save_asset, write_notebook, CellKind, Notebook, NotebookDocument,
    MAX_NOTEBOOK_BYTES,
};
use cgx2_notebook_kernel::{
    is_worker_argument, scalar_worker_main, validate_pure_sweep_notebook, ExecutionControl,
    NotebookKernel, SweepParameter, DEFAULT_CELL_TIMEOUT, MAX_SWEEP_VALUES,
};
use std::time::Duration;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const HELP: &str = "cgx2-notebook: clean, pure scalar notebook file runner\n\
Usage:\n\
cgx2-notebook run INPUT.cgnb --out OUTPUT.cgnb [--project ROOT] [--edit-cell ID --source CG_CODE] [--timeout-ms N] [--force]\n\
  cgx2-notebook restart INPUT.cgnb --out OUTPUT.cgnb [--force]\n\
  cgx2-notebook delete INPUT.cgnb CELL_ID --out OUTPUT.cgnb [--force]\n\
  cgx2-notebook inspect INPUT.cgnb\n\
  cgx2-notebook show INPUT.cgnb CELL_ID\n\
  cgx2-notebook export INPUT.cgnb --out PORTABLE.cgnb [--force]\n\
cgx2-notebook import PORTABLE.cgnb --out OUTPUT.cgnb [--force]\n\
cgx2-notebook sweep INPUT.cgnb CELL_ID --value LITERAL [--value LITERAL ...] --out-dir DIRECTORY [--timeout-ms N]\n\
\n\
Run starts a clean session, replaces saved code outputs, executes code cells in file order,\n\
and saves fresh text/plain outputs. Inspect reads cell IDs, kinds, saved provenance and status\n\
without executing. Only immutable bindings and pure scalar expressions are supported.\n\
With --edit-cell, run first establishes clean scalar state, then recomputes the edit and its\n\
transitive readers without rerunning independent cells. Show prints saved captured text and\n\
provenance without executing. There is no side-effecting rerun or implicit overwrite.\n";

enum Command {
    Help,
    Sweep {
        input: PathBuf,
        id: String,
        values: Vec<String>,
        output_dir: PathBuf,
        timeout: Duration,
    },
    Inspect(PathBuf),
    Show(PathBuf, String),
    Export {
        input: PathBuf,
        output: PathBuf,
        force: bool,
    },
    Import {
        input: PathBuf,
        output: PathBuf,
        force: bool,
    },
    Run {
        input: PathBuf,
        output: PathBuf,
        force: bool,
        edit_cell: Option<String>,
        source: Option<String>,
        timeout: Duration,
        project: Option<PathBuf>,
    },
    Restart {
        input: PathBuf,
        output: PathBuf,
        force: bool,
        project: Option<PathBuf>,
    },
    Delete {
        input: PathBuf,
        id: String,
        output: PathBuf,
        force: bool,
    },
}

#[derive(Debug)]
enum CommandError {
    Usage(String),
    Input(String),
    Cell(String),
    Output(String),
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) => write!(f, "command error: {message}"),
            Self::Input(message) => write!(f, "notebook input error: {message}"),
            Self::Cell(message) => write!(f, "cell execution error: {message}"),
            Self::Output(message) => write!(f, "notebook output error: {message}"),
        }
    }
}

fn main() -> ExitCode {
    if env::args()
        .nth(1)
        .as_deref()
        .is_some_and(is_worker_argument)
    {
        return if scalar_worker_main() == 0 {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    let result = parse_command(env::args_os().skip(1)).and_then(execute);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn parse_command(mut args: impl Iterator<Item = OsString>) -> Result<Command, CommandError> {
    let Some(command) = args.next() else {
        return Err(CommandError::Usage(format!("missing command\n{HELP}")));
    };
    if command == "--help" || command == "-h" || command == "help" {
        if args.next().is_some() {
            return Err(CommandError::Usage("help takes no arguments".into()));
        }
        return Ok(Command::Help);
    }
    if command == "inspect" {
        let input = args
            .next()
            .ok_or_else(|| CommandError::Usage("inspect requires INPUT.cgnb".into()))?;
        if args.next().is_some() {
            return Err(CommandError::Usage(
                "inspect accepts exactly one input path".into(),
            ));
        }
        let input = PathBuf::from(input);
        require_extension(&input)?;
        return Ok(Command::Inspect(input));
    }
    if command == "show" {
        let input =
            PathBuf::from(args.next().ok_or_else(|| {
                CommandError::Usage("show requires INPUT.cgnb and CELL_ID".into())
            })?);
        require_extension(&input)?;
        let id = args
            .next()
            .ok_or_else(|| CommandError::Usage("show requires CELL_ID".into()))?
            .into_string()
            .map_err(|_| CommandError::Usage("CELL_ID must be UTF-8".into()))?;
        if args.next().is_some() {
            return Err(CommandError::Usage("show accepts one CELL_ID".into()));
        }
        return Ok(Command::Show(input, id));
    }
    if command == "delete" {
        let input =
            PathBuf::from(args.next().ok_or_else(|| {
                CommandError::Usage("delete requires INPUT.cgnb and CELL_ID".into())
            })?);
        require_extension(&input)?;
        let id = args
            .next()
            .ok_or_else(|| CommandError::Usage("delete requires CELL_ID".into()))?
            .into_string()
            .map_err(|_| CommandError::Usage("CELL_ID must be UTF-8".into()))?;
        let mut output = None;
        let mut force = false;
        while let Some(arg) = args.next() {
            if arg == "--out" && output.is_none() {
                let path = PathBuf::from(
                    args.next()
                        .ok_or_else(|| CommandError::Usage("--out requires OUTPUT.cgnb".into()))?,
                );
                require_extension(&path)?;
                output = Some(path);
            } else if arg == "--force" && !force {
                force = true;
            } else {
                return Err(CommandError::Usage(format!(
                    "unexpected delete argument: {}",
                    arg.to_string_lossy()
                )));
            }
        }
        return Ok(Command::Delete {
            input,
            id,
            output: output.ok_or_else(|| CommandError::Usage("delete requires --out".into()))?,
            force,
        });
    }
    if command == "sweep" {
        let input = PathBuf::from(
            args.next()
                .ok_or_else(|| CommandError::Usage("sweep requires INPUT.cgnb".into()))?,
        );
        require_extension(&input)?;
        let id = args
            .next()
            .ok_or_else(|| CommandError::Usage("sweep requires CELL_ID".into()))?
            .into_string()
            .map_err(|_| CommandError::Usage("cell ID must be UTF-8".into()))?;
        let mut values = Vec::new();
        let mut output_dir = None;
        let mut timeout = DEFAULT_CELL_TIMEOUT;
        let mut timeout_seen = false;
        while let Some(arg) = args.next() {
            if arg == "--value" {
                let value = args
                    .next()
                    .ok_or_else(|| CommandError::Usage("--value requires LITERAL".into()))?
                    .into_string()
                    .map_err(|_| CommandError::Usage("sweep value must be UTF-8".into()))?;
                values.push(value);
                if values.len() > MAX_SWEEP_VALUES {
                    return Err(CommandError::Usage(format!(
                        "at most {MAX_SWEEP_VALUES} sweep values allowed"
                    )));
                }
            } else if arg == "--out-dir" && output_dir.is_none() {
                output_dir = Some(PathBuf::from(args.next().ok_or_else(|| {
                    CommandError::Usage("--out-dir requires DIRECTORY".into())
                })?));
            } else if arg == "--timeout-ms" && !timeout_seen {
                timeout_seen = true;
                let millis = args
                    .next()
                    .ok_or_else(|| CommandError::Usage("--timeout-ms requires N".into()))?
                    .into_string()
                    .map_err(|_| CommandError::Usage("timeout must be UTF-8".into()))?
                    .parse::<u64>()
                    .map_err(|_| CommandError::Usage("timeout must be an integer".into()))?;
                if !(1..=60_000).contains(&millis) {
                    return Err(CommandError::Usage("timeout must be 1..=60000 ms".into()));
                }
                timeout = Duration::from_millis(millis);
            } else {
                return Err(CommandError::Usage(format!(
                    "unexpected sweep argument: {}",
                    arg.to_string_lossy()
                )));
            }
        }
        if values.is_empty() {
            return Err(CommandError::Usage(
                "sweep requires at least one --value".into(),
            ));
        }
        return Ok(Command::Sweep {
            input,
            id,
            values,
            output_dir: output_dir
                .ok_or_else(|| CommandError::Usage("sweep requires --out-dir".into()))?,
            timeout,
        });
    }
    if command == "run" || command == "restart" || command == "export" || command == "import" {
        let input = PathBuf::from(args.next().ok_or_else(|| {
            CommandError::Usage(format!("{} requires INPUT.cgnb", command.to_string_lossy()))
        })?);
        require_extension(&input)?;
        let mut output = None;
        let mut force = false;
        let mut edit_cell = None;
        let mut source = None;
        let mut timeout = DEFAULT_CELL_TIMEOUT;
        let mut timeout_seen = false;
        let mut project = None;
        while let Some(arg) = args.next() {
            if arg == "--out" {
                if output.is_some() {
                    return Err(CommandError::Usage("--out may appear only once".into()));
                }
                let path = args
                    .next()
                    .ok_or_else(|| CommandError::Usage("--out requires OUTPUT.cgnb".into()))?;
                let path = PathBuf::from(path);
                require_extension(&path)?;
                output = Some(path);
            } else if arg == "--force" {
                if force {
                    return Err(CommandError::Usage("--force may appear only once".into()));
                }
                force = true;
            } else if (command == "run" || command == "restart") && arg == "--project" {
                if project.is_some() {
                    return Err(CommandError::Usage("--project may appear only once".into()));
                }
                project = Some(PathBuf::from(args.next().ok_or_else(|| {
                    CommandError::Usage("--project requires ROOT".into())
                })?));
            } else if command == "run" && arg == "--edit-cell" {
                if edit_cell.is_some() {
                    return Err(CommandError::Usage(
                        "--edit-cell may appear only once".into(),
                    ));
                }
                edit_cell = Some(
                    args.next()
                        .ok_or_else(|| CommandError::Usage("--edit-cell requires ID".into()))?
                        .into_string()
                        .map_err(|_| CommandError::Usage("cell ID must be UTF-8".into()))?,
                );
            } else if command == "run" && arg == "--source" {
                if source.is_some() {
                    return Err(CommandError::Usage("--source may appear only once".into()));
                }
                source = Some(
                    args.next()
                        .ok_or_else(|| CommandError::Usage("--source requires CG_CODE".into()))?
                        .into_string()
                        .map_err(|_| CommandError::Usage("source must be UTF-8".into()))?,
                );
            } else if command == "run" && arg == "--timeout-ms" {
                if timeout_seen {
                    return Err(CommandError::Usage(
                        "--timeout-ms may appear only once".into(),
                    ));
                }
                timeout_seen = true;
                let millis = args
                    .next()
                    .ok_or_else(|| CommandError::Usage("--timeout-ms requires N".into()))?
                    .into_string()
                    .map_err(|_| CommandError::Usage("timeout must be UTF-8".into()))?
                    .parse::<u64>()
                    .map_err(|_| CommandError::Usage("timeout must be an integer".into()))?;
                if !(1..=60_000).contains(&millis) {
                    return Err(CommandError::Usage("timeout must be 1..=60000 ms".into()));
                }
                timeout = Duration::from_millis(millis);
            } else {
                return Err(CommandError::Usage(format!(
                    "unexpected argument: {}",
                    arg.to_string_lossy()
                )));
            }
        }
        let output = output.ok_or_else(|| {
            CommandError::Usage(format!("{} requires --out", command.to_string_lossy()))
        })?;
        if edit_cell.is_some() != source.is_some() {
            return Err(CommandError::Usage(
                "--edit-cell and --source must appear together".into(),
            ));
        }
        return Ok(if command == "run" {
            Command::Run {
                input,
                output,
                force,
                edit_cell,
                source,
                timeout,
                project,
            }
        } else if command == "restart" {
            Command::Restart {
                input,
                output,
                force,
                project,
            }
        } else if command == "export" {
            Command::Export {
                input,
                output,
                force,
            }
        } else {
            Command::Import {
                input,
                output,
                force,
            }
        });
    }
    Err(CommandError::Usage(format!(
        "unknown command: {}\n{HELP}",
        command.to_string_lossy()
    )))
}

fn require_extension(path: &Path) -> Result<(), CommandError> {
    if path.extension() == Some(OsStr::new("cgnb")) {
        Ok(())
    } else {
        Err(CommandError::Usage(format!(
            "expected a .cgnb path: {}",
            path.display()
        )))
    }
}

fn execute(command: Command) -> Result<(), CommandError> {
    match command {
        Command::Help => {
            print!("{HELP}");
            Ok(())
        }
        Command::Sweep {
            input,
            id,
            values,
            output_dir,
            timeout,
        } => execute_sweep(&input, &id, &values, &output_dir, timeout),
        Command::Inspect(input) => {
            let notebook = read_notebook(&input)?;
            let kernel = NotebookKernel::open(notebook)
                .map_err(|error| CommandError::Input(error.to_string()))?;
            for cell in &kernel.notebook().cells {
                match cell.kind {
                    CellKind::Markdown => println!("{}\tmarkdown\t-", cell.id),
                    CellKind::Code => {
                        let status = kernel
                            .status(&cell.id)
                            .map_err(|error| CommandError::Input(error.to_string()))?;
                        if let Some(p) = &cell.provenance {
                            println!("{}\tcode\t{}\tschema={}\tordinal={}\toutput_ordinal={}\tevaluator={}\tsource_sha256={}\tprefix_sha256={}\tresult={}",
                                cell.id, status_name(status), p.schema_version, p.ordinal,
                                p.output_ordinal.map_or_else(|| "none".into(), |ordinal| ordinal.to_string()),
                                p.evaluator, p.source_sha256, p.prefix_sha256,
                                provenance_result_name(p.result));
                        } else {
                            println!("{}\tcode\t{}", cell.id, status_name(status));
                        }
                    }
                }
            }
            Ok(())
        }
        Command::Show(input, id) => {
            let notebook = read_notebook(&input)?;
            let cell = notebook
                .cells
                .iter()
                .find(|cell| cell.id == id)
                .ok_or_else(|| CommandError::Input(format!("unknown notebook cell ID: {id}")))?;
            if let Some(p) = &cell.provenance {
                println!("cell={id} schema={} execution_revision={} source_sha256={} dependency_sha256={} result={}",
                    p.schema_version, p.ordinal, p.source_sha256, p.prefix_sha256,
                    provenance_result_name(p.result));
            }
            for output in &cell.outputs {
                if let cgx2_notebook_format::Output::Text {
                    mime,
                    ordinal,
                    body,
                } = output
                {
                    println!("mime={mime} output_ordinal={ordinal} text={:?}", body);
                }
            }
            Ok(())
        }
        Command::Run {
            input,
            output,
            force,
            edit_cell,
            source,
            timeout,
            project,
        } => {
            check_output_collision(&output, force)?;
            let document = read_document(&input)?;
            let mut kernel =
                NotebookKernel::open_with_project(document.notebook, project.as_deref())
                    .map_err(|error| CommandError::Input(error.to_string()))?;
            kernel.set_execution_control(ExecutionControl::new(
                env::current_exe().map_err(|error| CommandError::Cell(error.to_string()))?,
                timeout,
            ));
            let run_result = kernel.restart_and_run_all();
            if let Err(error) = run_result {
                if error.code == "CGNK010" {
                    let text = portable_text(kernel.notebook(), &document.assets)
                        .map_err(|error| CommandError::Output(error.to_string()))?;
                    atomic_write(&output, text.as_bytes(), force)?;
                }
                return Err(CommandError::Cell(error.to_string()));
            }
            if let (Some(id), Some(source)) = (edit_cell, source) {
                kernel
                    .replace_cell(&id, source)
                    .map_err(|error| CommandError::Cell(error.to_string()))?;
                kernel
                    .rerun_affected(&id)
                    .map_err(|error| CommandError::Cell(error.to_string()))?;
            }
            let text = portable_text(kernel.notebook(), &document.assets)
                .map_err(|error| CommandError::Output(error.to_string()))?;
            atomic_write(&output, text.as_bytes(), force)
        }
        Command::Delete {
            input,
            id,
            output,
            force,
        } => {
            check_output_collision(&output, force)?;
            let document = read_document(&input)?;
            let mut kernel = NotebookKernel::open(document.notebook)
                .map_err(|error| CommandError::Input(error.to_string()))?;
            kernel
                .remove_cell(&id)
                .map_err(|error| CommandError::Cell(error.to_string()))?;
            let text = portable_text(kernel.notebook(), &document.assets)
                .map_err(|error| CommandError::Output(error.to_string()))?;
            atomic_write(&output, text.as_bytes(), force)
        }
        Command::Restart {
            input,
            output,
            force,
            project,
        } => {
            check_output_collision(&output, force)?;
            let document = read_document(&input)?;
            let mut kernel =
                NotebookKernel::open_with_project(document.notebook, project.as_deref())
                    .map_err(|error| CommandError::Input(error.to_string()))?;
            kernel
                .restart()
                .map_err(|error| CommandError::Cell(error.to_string()))?;
            let text = portable_text(kernel.notebook(), &document.assets)
                .map_err(|error| CommandError::Output(error.to_string()))?;
            atomic_write(&output, text.as_bytes(), force)
        }
        Command::Export {
            input,
            output,
            force,
        } => {
            check_output_collision(&output, force)?;
            let document = read_document(&input)?;
            let text = export_document(&document.notebook, &document.assets)
                .map_err(|error| CommandError::Input(error.to_string()))?;
            atomic_write(&output, text.as_bytes(), force)
        }
        Command::Import {
            input,
            output,
            force,
        } => {
            check_output_collision(&output, force)?;
            let bytes = read_limited_bytes(&input)?;
            let text = std::str::from_utf8(&bytes)
                .map_err(|_| CommandError::Input("portable notebook is not UTF-8".into()))?;
            let document = parse_self_contained(text)
                .map_err(|error| CommandError::Input(error.to_string()))?;
            let native = write_notebook(&document.notebook)
                .map_err(|error| CommandError::Output(error.to_string()))?;
            import_document(&output, &document, native.as_bytes(), force)
        }
    }
}

fn reject_symlink_components(path: &Path) -> Result<(), CommandError> {
    use std::path::Component;
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(CommandError::Input(format!(
            "path traversal is not allowed: {}",
            path.display()
        )));
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(CommandError::Input(format!(
                    "symlink is not allowed: {}",
                    current.display()
                )))
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(input_io(&current, error)),
        }
    }
    Ok(())
}

fn json_string(value: &str) -> String {
    let mut result = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            ch if ch <= '\u{1f}' => result.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => result.push(ch),
        }
    }
    result.push('"');
    result
}

fn execute_sweep(
    input: &Path,
    id: &str,
    values: &[String],
    output_dir: &Path,
    timeout: Duration,
) -> Result<(), CommandError> {
    reject_symlink_components(input)?;
    reject_symlink_components(&sidecar(input))?;
    reject_symlink_components(output_dir)?;
    let parent = output_dir
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = parent
        .canonicalize()
        .map_err(|error| output_io(parent, error))?;
    let name = output_dir
        .file_name()
        .ok_or_else(|| CommandError::Output("output directory needs a name".into()))?;
    let destination = parent.join(name);
    if fs::symlink_metadata(&destination).is_ok() {
        return Err(CommandError::Output(format!(
            "{} already exists",
            destination.display()
        )));
    }
    let document = read_document(input)?;
    let mut notebook = document.notebook;
    let mut markdown_digests = BTreeSet::new();
    for cell in notebook
        .cells
        .iter()
        .filter(|cell| cell.kind == CellKind::Markdown)
    {
        markdown_digests.extend(
            markdown_image_digests(&cell.source)
                .map_err(|error| CommandError::Input(error.to_string()))?,
        );
    }
    let markdown_assets = document
        .assets
        .into_iter()
        .filter(|(digest, _)| markdown_digests.contains(digest))
        .collect();
    validate_pure_sweep_notebook(&notebook)
        .map_err(|error| CommandError::Input(error.to_string()))?;
    let parameter_cell = notebook
        .cells
        .iter()
        .find(|cell| cell.id == id && cell.kind == CellKind::Code)
        .ok_or_else(|| CommandError::Input(format!("unknown code cell ID: {id}")))?;
    let parameter = SweepParameter::parse(&parameter_cell.source)
        .map_err(|error| CommandError::Input(error.to_string()))?;
    let mut replacements = Vec::new();
    let mut seen = BTreeSet::new();
    for value in values {
        let (source, key) = parameter
            .replacement(value)
            .map_err(|error| CommandError::Input(error.to_string()))?;
        if !seen.insert(key) {
            return Err(CommandError::Input(format!(
                "duplicate sweep value: {value}"
            )));
        }
        let mut candidate = notebook.clone();
        candidate
            .cells
            .iter_mut()
            .find(|cell| cell.id == id)
            .expect("validated cell")
            .source = source.clone();
        validate_pure_sweep_notebook(&candidate)
            .map_err(|error| CommandError::Input(error.to_string()))?;
        NotebookKernel::open(candidate).map_err(|error| CommandError::Input(error.to_string()))?;
        replacements.push(source);
    }
    for cell in &mut notebook.cells {
        cell.outputs.clear();
        cell.provenance = None;
    }
    let stage = (0..16)
        .find_map(|_| {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(".cgx2-sweep-{}-{sequence}.tmp", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => Some(Ok(TemporaryDirectory(path))),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(output_io(&destination, error))),
            }
        })
        .ok_or_else(|| {
            CommandError::Output("could not allocate sweep staging directory".into())
        })??;
    let worker = env::current_exe().map_err(|error| CommandError::Cell(error.to_string()))?;
    let mut summary = format!(
        "{{\"schema_version\":1,\"input\":{},\"parameter_cell\":{},\"timeout_ms\":{},\"trials\":[",
        json_string(&input.display().to_string()),
        json_string(id),
        timeout.as_millis()
    );
    for (index, (value, source)) in values.iter().zip(replacements).enumerate() {
        let mut trial = notebook.clone();
        trial
            .cells
            .iter_mut()
            .find(|cell| cell.id == id)
            .expect("validated cell")
            .source = source;
        let mut kernel =
            NotebookKernel::open(trial).map_err(|error| CommandError::Cell(error.to_string()))?;
        kernel.set_execution_control(ExecutionControl::new(worker.clone(), timeout));
        let result = kernel.restart_and_run_all();
        let filename = format!("trial-{:02}.cgnb", index + 1);
        let text = export_document(kernel.notebook(), &markdown_assets)
            .map_err(|error| CommandError::Output(error.to_string()))?;
        fs::write(stage.0.join(&filename), text).map_err(|error| output_io(&destination, error))?;
        if index != 0 {
            summary.push(',');
        }
        summary.push_str(&format!(
            "{{\"index\":{},\"value\":{},\"file\":{},\"error\":{},\"cells\":[",
            index + 1,
            json_string(value),
            json_string(&filename),
            result
                .as_ref()
                .err()
                .map_or_else(|| "null".into(), |error| json_string(&error.to_string()))
        ));
        for (cell_index, cell) in kernel
            .notebook()
            .cells
            .iter()
            .filter(|cell| cell.kind == CellKind::Code)
            .enumerate()
        {
            if cell_index != 0 {
                summary.push(',');
            }
            let status = kernel
                .status(&cell.id)
                .map_err(|error| CommandError::Cell(error.to_string()))?;
            summary.push_str(&format!(
                "{{\"id\":{},\"status\":{}}}",
                json_string(&cell.id),
                json_string(status_name(status))
            ));
        }
        summary.push_str("]}");
    }
    summary.push_str("]}\n");
    fs::write(stage.0.join("summary.json"), summary)
        .map_err(|error| output_io(&destination, error))?;
    if fs::symlink_metadata(&destination).is_ok() {
        return Err(CommandError::Output(format!(
            "{} already exists",
            destination.display()
        )));
    }
    publish_directory(&stage.0, &destination).map_err(|error| output_io(&destination, error))?;
    println!("{}", destination.display());
    Ok(())
}

#[cfg(target_os = "linux")]
fn publish_directory(source: &Path, destination: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" {
        fn renameat2(
            old_dirfd: i32,
            old_path: *const i8,
            new_dirfd: i32,
            new_path: *const i8,
            flags: u32,
        ) -> i32;
    }
    let old = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in staging path"))?;
    let new = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in output path"))?;
    // RENAME_NOREPLACE avoids replacing a destination created after validation.
    let result = unsafe { renameat2(-100, old.as_ptr(), -100, new.as_ptr(), 1) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(target_os = "linux"))]
fn publish_directory(source: &Path, destination: &Path) -> io::Result<()> {
    // Other platforms retain the explicit collision check immediately before rename.
    fs::rename(source, destination)
}

fn status_name(status: cgx2_notebook_session::CellStatus) -> &'static str {
    use cgx2_notebook_session::CellStatus;
    match status {
        CellStatus::NeverRun => "never-run",
        CellStatus::Cached => "cached",
        CellStatus::Current => "current",
        CellStatus::Stale => "stale",
        CellStatus::Failed => "failed",
    }
}

fn provenance_result_name(result: cgx2_notebook_format::ProvenanceResult) -> &'static str {
    use cgx2_notebook_format::ProvenanceResult;
    match result {
        ProvenanceResult::Success => "success",
        ProvenanceResult::Failed => "failed",
    }
}

fn read_notebook(path: &Path) -> Result<Notebook, CommandError> {
    Ok(read_document(path)?.notebook)
}

fn portable_text(
    notebook: &Notebook,
    verified_assets: &BTreeMap<String, Vec<u8>>,
) -> Result<String, cgx2_notebook_format::NotebookError> {
    let references = referenced_asset_digests(notebook)?;
    let assets = verified_assets
        .iter()
        .filter(|(digest, _)| references.contains(*digest))
        .map(|(digest, bytes)| (digest.clone(), bytes.clone()))
        .collect();
    export_document(notebook, &assets)
}

fn read_document(path: &Path) -> Result<NotebookDocument, CommandError> {
    let bytes = read_limited_bytes(path)?;
    open_notebook_bytes(&bytes, &sidecar(path))
        .map_err(|error| CommandError::Input(format!("{}: {error}", path.display())))
}

fn read_limited_bytes(path: &Path) -> Result<Vec<u8>, CommandError> {
    let file = File::open(path).map_err(|error| input_io(path, error))?;
    let mut bytes = Vec::new();
    file.take((MAX_NOTEBOOK_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| input_io(path, error))?;
    Ok(bytes)
}

fn sidecar(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".assets");
    PathBuf::from(name)
}

fn input_io(path: &Path, error: io::Error) -> CommandError {
    CommandError::Input(format!("{}: {error}", path.display()))
}

fn output_io(path: &Path, error: io::Error) -> CommandError {
    CommandError::Output(format!("{}: {error}", path.display()))
}

fn check_output_collision(path: &Path, force: bool) -> Result<(), CommandError> {
    match fs::symlink_metadata(path) {
        Ok(_) if !force => Err(CommandError::Output(format!(
            "{} already exists; pass --force to replace it",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(output_io(path, error)),
    }
}

struct TemporaryFile(PathBuf);

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn atomic_write(path: &Path, bytes: &[u8], force: bool) -> Result<(), CommandError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for _ in 0..16 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let name = format!(
            ".cgx2-notebook-{}-{stamp}-{sequence}.tmp",
            std::process::id()
        );
        let temporary_path = parent.join(name);
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(output_io(path, error)),
        };
        let temporary = TemporaryFile(temporary_path);
        file.write_all(bytes)
            .map_err(|error| output_io(path, error))?;
        file.sync_all().map_err(|error| output_io(path, error))?;
        drop(file);
        if force {
            fs::rename(&temporary.0, path).map_err(|error| output_io(path, error))?;
        } else {
            fs::hard_link(&temporary.0, path).map_err(|error| {
                if error.kind() == io::ErrorKind::AlreadyExists {
                    CommandError::Output(format!(
                        "{} already exists; pass --force to replace it",
                        path.display()
                    ))
                } else {
                    output_io(path, error)
                }
            })?;
        }
        return Ok(());
    }
    Err(CommandError::Output(format!(
        "{}: could not allocate a temporary sibling file",
        path.display()
    )))
}

struct TemporaryDirectory(PathBuf);

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn import_document(
    output: &Path,
    document: &NotebookDocument,
    native: &[u8],
    force: bool,
) -> Result<(), CommandError> {
    if document.assets.is_empty() {
        return atomic_write(output, native, force);
    }
    let target = sidecar(output);
    let existing = match fs::symlink_metadata(&target) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(CommandError::Output(format!(
                "{} is not a regular asset directory",
                target.display()
            )));
        }
        Ok(_) if !force => {
            return Err(CommandError::Output(format!(
                "{} already exists; pass --force to replace it",
                target.display()
            )));
        }
        Ok(_) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(output_io(&target, error)),
    };
    let parent = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut stage = None;
    for _ in 0..16 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".cgx2-notebook-assets-{}-{stamp}-{sequence}.tmp",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => {
                stage = Some(TemporaryDirectory(path));
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(output_io(&target, error)),
        }
    }
    let stage = stage.ok_or_else(|| {
        CommandError::Output(format!(
            "{}: cannot allocate staging directory",
            target.display()
        ))
    })?;
    for bytes in document.assets.values() {
        save_asset(&stage.0, bytes).map_err(|error| CommandError::Output(error.to_string()))?;
    }
    let mut backup_name = stage.0.as_os_str().to_os_string();
    backup_name.push(".backup");
    let backup = PathBuf::from(backup_name);
    if existing {
        fs::rename(&target, &backup).map_err(|error| output_io(&target, error))?;
    }
    if let Err(error) = fs::rename(&stage.0, &target) {
        if existing {
            let _ = fs::rename(&backup, &target);
        }
        return Err(output_io(&target, error));
    }
    if let Err(error) = atomic_write(output, native, force) {
        fs::remove_dir_all(&target).map_err(|rollback| output_io(&target, rollback))?;
        if existing {
            fs::rename(&backup, &target).map_err(|rollback| output_io(&target, rollback))?;
        }
        return Err(error);
    }
    if existing {
        fs::remove_dir_all(&backup).map_err(|error| output_io(&backup, error))?;
    }
    Ok(())
}
