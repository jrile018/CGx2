use cgx2_emit_rust::emit_rust;
use cgx2_sema::check_module;
use cgx2_syntax::parse_module;
use sha2::{Digest, Sha256};
use std::{
    env,
    ffi::OsString,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const USAGE: &str = "usage: cgx2c build FILE.cg --out BINARY [--release] [--force] [--rustc PATH]\n       cgx2c emit-rust FILE.cg --out FILE.rs [--force]";
const MAX_SOURCE: u64 = 4 * 1024 * 1024;
const MAX_GENERATED: usize = 32 * 1024 * 1024;
const MAX_STDERR: usize = 8 * 1024;
static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Build,
    Emit,
}

struct Options {
    mode: Mode,
    source: PathBuf,
    output: PathBuf,
    force: bool,
    release: bool,
    rustc: OsString,
}

struct CliError {
    code: String,
    message: String,
}

impl CliError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}
impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

fn main() -> ExitCode {
    match run(env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<OsString>) -> Result<(), CliError> {
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let options = parse_args(args)?;
    validate_output(&options)?;
    let metadata = fs::metadata(&options.source).map_err(|e| {
        CliError::new(
            "CGXC002",
            format!("cannot read {}: {e}", options.source.display()),
        )
    })?;
    if !metadata.is_file() || metadata.len() > MAX_SOURCE {
        return Err(CliError::new(
            "CGXC002",
            "source must be a regular UTF-8 file of at most 4 MiB",
        ));
    }
    let source = fs::read_to_string(&options.source).map_err(|e| {
        CliError::new(
            "CGXC002",
            format!("cannot read {}: {e}", options.source.display()),
        )
    })?;
    if source.len() as u64 > MAX_SOURCE {
        return Err(CliError::new("CGXC002", "source exceeds 4 MiB"));
    }
    let module = parse_module(&source).map_err(|errors| {
        let d = &errors[0];
        source_error(
            &options.source,
            d.code,
            &d.message,
            d.span.start,
            d.span.end,
        )
    })?;
    let checked = check_module(&module).map_err(|errors| {
        let d = &errors[0];
        source_error(
            &options.source,
            d.code,
            &d.message,
            d.span.start,
            d.span.end,
        )
    })?;
    let generated = emit_rust(&module, &checked).map_err(|errors| {
        let d = &errors[0];
        source_error(
            &options.source,
            d.code,
            &d.message,
            d.span.start,
            d.span.end,
        )
    })?;
    if generated.len() > MAX_GENERATED {
        return Err(CliError::new("CGXC006", "generated Rust exceeds 32 MiB"));
    }
    let parent = options
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temporary = PrivateDir::new(parent)?;
    let generated_path = temporary.path.join("generated.rs");
    fs::write(&generated_path, generated.as_bytes()).map_err(|e| {
        CliError::new(
            "CGXC006",
            format!("cannot write temporary Rust source: {e}"),
        )
    })?;
    if options.mode == Mode::Emit {
        install(&generated_path, &options.output, options.force)?;
        return Ok(());
    }
    let binary_path = temporary.path.join("program");
    compile(
        &options.rustc,
        &generated_path,
        &binary_path,
        options.release,
    )?;
    let built = fs::metadata(&binary_path)
        .map_err(|e| CliError::new("CGXC005", format!("rustc succeeded without a binary: {e}")))?;
    if !built.is_file() {
        return Err(CliError::new(
            "CGXC005",
            "rustc did not produce a regular binary",
        ));
    }
    // Recheck immediately before installation; Unix rename replaces the entry, not its symlink target.
    validate_output(&options)?;
    install(&binary_path, &options.output, options.force)?;
    let (toolchain, target) = rustc_version(&options.rustc);
    let hash = Sha256::digest(source.as_bytes());
    println!(
        "cgx2c build metadata: source_sha256={hash:x} rustc={} toolchain={} target={} profile={} output={}",
        options.rustc.to_string_lossy(),
        toolchain,
        target,
        if options.release { "release" } else { "debug" },
        options.output.display()
    );
    Ok(())
}

fn source_error(path: &Path, code: &str, message: &str, start: usize, end: usize) -> CliError {
    CliError::new(
        code,
        format!("{}:{start}..{end}: {message}", path.display()),
    )
}

fn parse_args(args: Vec<OsString>) -> Result<Options, CliError> {
    let usage = || CliError::new("CGXC001", USAGE);
    if args.len() < 4 {
        return Err(usage());
    }
    let mode = match args[0].to_str() {
        Some("build") => Mode::Build,
        Some("emit-rust") => Mode::Emit,
        _ => return Err(usage()),
    };
    let source = PathBuf::from(&args[1]);
    if source.extension().is_none_or(|ext| ext != "cg") {
        return Err(usage());
    }
    let mut output = None;
    let mut force = false;
    let mut release = false;
    let mut rustc = None;
    let mut index = 2;
    while index < args.len() {
        match args[index].to_str() {
            Some("--out") if output.is_none() && index + 1 < args.len() => {
                output = Some(PathBuf::from(&args[index + 1]));
                index += 2;
            }
            Some("--force") if !force => {
                force = true;
                index += 1;
            }
            Some("--release") if mode == Mode::Build && !release => {
                release = true;
                index += 1;
            }
            Some("--rustc") if mode == Mode::Build && rustc.is_none() && index + 1 < args.len() => {
                rustc = Some(args[index + 1].clone());
                index += 2;
            }
            _ => return Err(usage()),
        }
    }
    let output = output.ok_or_else(usage)?;
    if output.as_os_str().is_empty() {
        return Err(usage());
    }
    let rustc = rustc
        .or_else(|| env::var_os("CGX2C_RUSTC"))
        .unwrap_or_else(|| OsString::from("rustc"));
    if rustc.is_empty() {
        return Err(usage());
    }
    Ok(Options {
        mode,
        source,
        output,
        force,
        release,
        rustc,
    })
}

fn validate_output(options: &Options) -> Result<(), CliError> {
    let source = fs::canonicalize(&options.source).ok();
    let output = fs::canonicalize(&options.output).ok();
    if source.is_some() && source == output {
        return Err(CliError::new("CGXC004", "output is the source file"));
    }
    #[cfg(unix)]
    if let (Ok(a), Ok(b)) = (fs::metadata(&options.source), fs::metadata(&options.output)) {
        use std::os::unix::fs::MetadataExt;
        if a.dev() == b.dev() && a.ino() == b.ino() {
            return Err(CliError::new("CGXC004", "output is the source file"));
        }
    }
    if let Ok(info) = fs::symlink_metadata(&options.output) {
        if info.file_type().is_symlink() || !info.is_file() {
            return Err(CliError::new(
                "CGXC004",
                "existing output must be a regular file, not a symlink or directory",
            ));
        }
        if !options.force {
            return Err(CliError::new(
                "CGXC004",
                "output exists; pass --force to replace it",
            ));
        }
    }
    Ok(())
}

struct PrivateDir {
    path: PathBuf,
}
impl PrivateDir {
    fn new(parent: &Path) -> Result<Self, CliError> {
        for _ in 0..16 {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            let name = format!(
                ".cgx2c-{}-{now}-{}",
                std::process::id(),
                NEXT_DIR.fetch_add(1, Ordering::Relaxed)
            );
            let path = parent.join(name);
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => {
                    return Err(CliError::new(
                        "CGXC006",
                        format!("cannot create private build directory: {e}"),
                    ));
                }
            }
        }
        Err(CliError::new(
            "CGXC006",
            "cannot allocate private build directory",
        ))
    }
}
impl Drop for PrivateDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn compile(rustc: &OsString, source: &Path, output: &Path, release: bool) -> Result<(), CliError> {
    let mut command = Command::new(rustc);
    command
        .arg("--edition=2024")
        .arg(source)
        .arg("-o")
        .arg(output);
    if release {
        command.arg("-O");
    }
    let mut child = command
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| CliError::new("CGXC005", format!("cannot start rustc: {e}")))?;
    let mut stderr = child.stderr.take().expect("piped stderr");
    let reader = std::thread::spawn(move || {
        let mut retained = Vec::new();
        let mut chunk = [0; 4096];
        let mut truncated = false;
        loop {
            match stderr.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    let available = MAX_STDERR.saturating_sub(retained.len());
                    retained.extend_from_slice(&chunk[..n.min(available)]);
                    truncated |= n > available;
                }
                Err(_) => break,
            }
        }
        (retained, truncated)
    });
    let status = child
        .wait()
        .map_err(|e| CliError::new("CGXC005", format!("cannot wait for rustc: {e}")))?;
    let (bytes, truncated) = reader
        .join()
        .map_err(|_| CliError::new("CGXC005", "cannot read rustc stderr"))?;
    if !status.success() {
        let message: String = String::from_utf8_lossy(&bytes)
            .chars()
            .map(|c| {
                if c.is_control() && c != '\n' && c != '\t' {
                    '�'
                } else {
                    c
                }
            })
            .collect();
        return Err(CliError::new(
            "CGXC005",
            format!(
                "rustc exited with {status}: {message}{}",
                if truncated { " [stderr truncated]" } else { "" }
            ),
        ));
    }
    Ok(())
}

fn install(source: &Path, output: &Path, force: bool) -> Result<(), CliError> {
    let result = if force {
        fs::rename(source, output)
    } else {
        fs::hard_link(source, output)
    };
    result.map_err(|e| {
        CliError::new(
            "CGXC006",
            format!("cannot install {}: {e}", output.display()),
        )
    })
}

fn rustc_version(rustc: &OsString) -> (String, String) {
    match Command::new(rustc).arg("-vV").output() {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let version = text.lines().next().unwrap_or("unavailable").to_owned();
            let target = text
                .lines()
                .find_map(|line| line.strip_prefix("host: "))
                .unwrap_or("unavailable")
                .to_owned();
            (version, target)
        }
        _ => ("unavailable".into(), "unavailable".into()),
    }
}
