//! File command for clean, pure-scalar CGx2 notebook execution.

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
    export_document, open_notebook_bytes, parse_self_contained, save_asset, write_notebook,
    CellKind, Notebook, NotebookDocument, MAX_NOTEBOOK_BYTES,
};
use cgx2_notebook_kernel::NotebookKernel;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const HELP: &str = "cgx2-notebook: clean, pure scalar notebook file runner\n\
Usage:\n\
  cgx2-notebook run INPUT.cgnb --out OUTPUT.cgnb [--force]\n\
  cgx2-notebook inspect INPUT.cgnb\n\
  cgx2-notebook export INPUT.cgnb --out PORTABLE.cgnb [--force]\n\
  cgx2-notebook import PORTABLE.cgnb --out OUTPUT.cgnb [--force]\n\
\n\
Run starts a clean session, replaces saved code outputs, executes code cells in file order,\n\
and saves fresh text/plain outputs. Inspect reads cell IDs, kinds, saved provenance and status\n\
without executing. Only immutable bindings and pure scalar expressions are supported.\n\
There is no live UI, individual side-effecting rerun, or implicit overwrite.\n";

enum Command {
    Help,
    Inspect(PathBuf),
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
    if command == "run" || command == "export" || command == "import" {
        let input = PathBuf::from(args.next().ok_or_else(|| {
            CommandError::Usage(format!("{} requires INPUT.cgnb", command.to_string_lossy()))
        })?);
        require_extension(&input)?;
        let mut output = None;
        let mut force = false;
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
        return Ok(if command == "run" {
            Command::Run {
                input,
                output,
                force,
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
                                if p.result == cgx2_notebook_format::ProvenanceResult::Success { "success" } else { "failed" });
                        } else {
                            println!("{}\tcode\t{}", cell.id, status_name(status));
                        }
                    }
                }
            }
            Ok(())
        }
        Command::Run {
            input,
            output,
            force,
        } => {
            check_output_collision(&output, force)?;
            let notebook = read_notebook(&input)?;
            let mut kernel = NotebookKernel::open(notebook)
                .map_err(|error| CommandError::Input(error.to_string()))?;
            kernel
                .restart_and_run_all()
                .map_err(|error| CommandError::Cell(error.to_string()))?;
            let text = write_notebook(kernel.notebook())
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

fn read_notebook(path: &Path) -> Result<Notebook, CommandError> {
    Ok(read_document(path)?.notebook)
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
