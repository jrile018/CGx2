//! Checked native compilation and source publication shared by `cgx2` and `cgx2c`.
use cgx2_emit_rust::{emit_package_rust, emit_rust, is_memory_script};
use cgx2_package::{CheckedPackage, load_checked_package};
use cgx2_sema::check_module;
use cgx2_syntax::parse_module;
mod python_archive;
use sha2::{Digest, Sha256};
use std::{
    env,
    ffi::OsString,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const USAGE: &str = "usage: cgx2c build FILE.cg --out BINARY [--release] [--force] [--rustc PATH] [--test-fail-reserve N]\n       cgx2c emit-rust FILE.cg --out FILE.rs [--force]\n       cgx2c build --package ROOT --out BINARY [--release] [--force] [--rustc PATH]\n       cgx2c emit-rust --package ROOT --out FILE.rs [--force]\n       cgx2c emit-python FILE.cg --out FILE.py [--force]\n       cgx2c emit-python --package ROOT --out FILE.pyz [--force]\n--test-fail-reserve is an explicit test-only host allocation fault for memory scripts";
const MAX_SOURCE: u64 = 4 * 1024 * 1024;
const MAX_GENERATED: usize = 32 * 1024 * 1024;
const MAX_STDERR: usize = 8 * 1024;
const RUNTIME_SOURCES: [(&str, &str); 9] = [
    ("lib.rs", include_str!("../../cgx2-runtime/src/lib.rs")),
    (
        "budget.rs",
        include_str!("../../cgx2-runtime/src/budget.rs"),
    ),
    ("heap.rs", include_str!("../../cgx2-runtime/src/heap.rs")),
    ("graph.rs", include_str!("../../cgx2-runtime/src/graph.rs")),
    ("scope.rs", include_str!("../../cgx2-runtime/src/scope.rs")),
    ("fault.rs", include_str!("../../cgx2-runtime/src/fault.rs")),
    ("owned.rs", include_str!("../../cgx2-runtime/src/owned.rs")),
    (
        "region.rs",
        include_str!("../../cgx2-runtime/src/region.rs"),
    ),
    (
        "report.rs",
        include_str!("../../cgx2-runtime/src/report.rs"),
    ),
];
static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Build,
    Emit,
    EmitPython,
}

struct Options {
    mode: Mode,
    source: PathBuf,
    package: bool,
    output: PathBuf,
    force: bool,
    release: bool,
    test_fail_reserve: Option<usize>,
    rustc: OsString,
}

#[derive(Debug)]
pub struct CliError {
    pub code: String,
    pub message: String,
    pub file: Option<PathBuf>,
    pub span: Option<cgx2_syntax::Span>,
}

impl CliError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            file: None,
            span: None,
        }
    }
}
impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(path) = &self.file {
            if let Some(span) = self.span {
                return write!(
                    f,
                    "{}: {}:{}..{}: {}",
                    self.code,
                    path.display(),
                    span.start,
                    span.end,
                    self.message
                );
            }
            return write!(f, "{}: {}: {}", self.code, path.display(), self.message);
        }
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for CliError {}

/// Execute the compiler command without invoking a sibling executable.
/// Build metadata is printed only after successful atomic publication.
pub fn run(args: Vec<OsString>) -> Result<(), CliError> {
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let options = parse_args(args)?;
    validate_output(&options)?;
    if options.package {
        return run_package(&options);
    }
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
    let needs_runtime = is_memory_script(&module);
    if options.test_fail_reserve.is_some() && !needs_runtime {
        return Err(CliError::new(
            "CGXC001",
            "--test-fail-reserve requires a memory script",
        ));
    }
    let generated = if options.mode == Mode::EmitPython {
        cgx2_emit_python::emit_python(&module, &checked).map_err(|errors| {
            let d = &errors[0];
            source_error(
                &options.source,
                d.code,
                &d.message,
                d.span.start,
                d.span.end,
            )
        })?
    } else {
        emit_rust(&module, &checked).map_err(|errors| {
            let d = &errors[0];
            source_error(
                &options.source,
                d.code,
                &d.message,
                d.span.start,
                d.span.end,
            )
        })?
    };
    if generated.len() > MAX_GENERATED {
        return Err(CliError::new("CGXC006", "generated source exceeds 32 MiB"));
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
    if options.mode != Mode::Build {
        validate_output(&options)?;
        install(&generated_path, &options.output, options.force)?;
        return Ok(());
    }
    let binary_path = temporary.path.join("program");
    let runtime = if needs_runtime {
        Some(compile_runtime(
            &options.rustc,
            &temporary.path,
            options.release,
            options.test_fail_reserve.is_some(),
        )?)
    } else {
        None
    };
    compile(
        &options.rustc,
        &generated_path,
        &binary_path,
        options.release,
        runtime.as_ref().map(|(path, _)| path.as_path()),
        Some(&options.source),
        options.test_fail_reserve,
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
        "cgx2c build metadata: source_sha256={hash:x} rustc={} toolchain={} target={} profile={} runtime_sha256={} output={}",
        options.rustc.to_string_lossy(),
        toolchain,
        target,
        if options.release { "release" } else { "debug" },
        runtime.as_ref().map_or("none", |(_, hash)| hash.as_str()),
        options.output.display()
    );
    Ok(())
}

fn package_error(root: &Path, errors: Vec<cgx2_package::PackageDiagnostic>) -> CliError {
    let first = &errors[0];
    let mut error = CliError::new(first.code, &first.message);
    error.file = Some(package_path(root, &first.file));
    error.span = first.span;
    error
}

fn package_path(root: &Path, file: &Path) -> PathBuf {
    if let Ok(relative) = file.strip_prefix(root) {
        return relative.to_owned();
    }
    // Revalidation uses the checked canonical root even when the user supplied
    // a relative path or a symlink to the package directory.
    if let Ok(canonical) = fs::canonicalize(root)
        && let Ok(relative) = file.strip_prefix(canonical)
    {
        return relative.to_owned();
    }
    file.to_owned()
}

fn run_package(options: &Options) -> Result<(), CliError> {
    let package =
        load_checked_package(&options.source).map_err(|e| package_error(&options.source, e))?;
    validate_package_output(options, &package)?;
    if options.mode == Mode::EmitPython {
        return emit_python_package(options, &package);
    }
    let generated = emit_package_rust(&package).map_err(|errors| {
        let first = &errors[0];
        source_error(
            &first.file,
            first.diagnostic.code,
            &first.diagnostic.message,
            first.diagnostic.span.start,
            first.diagnostic.span.end,
        )
    })?;
    if generated.len() > MAX_GENERATED {
        return Err(CliError::new("CGXC006", "generated source exceeds 32 MiB"));
    }
    package
        .verify_unchanged()
        .map_err(|e| package_error(&options.source, e))?;
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
        package
            .verify_unchanged()
            .map_err(|e| package_error(&options.source, e))?;
        validate_output(options)?;
        validate_package_output(options, &package)?;
        install(&generated_path, &options.output, options.force)?;
        return Ok(());
    }
    let binary_path = temporary.path.join("program");
    compile(
        &options.rustc,
        &generated_path,
        &binary_path,
        options.release,
        None,
        None,
        None,
    )?;
    let built = fs::metadata(&binary_path)
        .map_err(|e| CliError::new("CGXC005", format!("rustc succeeded without a binary: {e}")))?;
    if !built.is_file() {
        return Err(CliError::new(
            "CGXC005",
            "rustc did not produce a regular binary",
        ));
    }
    package
        .verify_unchanged()
        .map_err(|e| package_error(&options.source, e))?;
    validate_output(options)?;
    validate_package_output(options, &package)?;
    install(&binary_path, &options.output, options.force)?;
    let (toolchain, target) = rustc_version(&options.rustc);
    let source_hashes = package
        .modules()
        .iter()
        .map(|m| format!("{}:{}", m.source().display(), m.source_sha256()))
        .collect::<Vec<_>>();
    println!(
        "cgx2c build metadata: package_manifest_sha256={} package_sources_sha256={source_hashes:?} rustc={} toolchain={} target={} profile={} output={}",
        package.manifest_sha256(),
        options.rustc.to_string_lossy(),
        toolchain,
        target,
        if options.release { "release" } else { "debug" },
        options.output.display()
    );
    Ok(())
}

fn validate_package_output(options: &Options, package: &CheckedPackage) -> Result<(), CliError> {
    let Ok(info) = fs::metadata(&options.output) else {
        return Ok(());
    };
    let output = fs::canonicalize(&options.output).ok();
    let paths = std::iter::once(package.root().join("cgx2.toml"))
        .chain(package.binding_lock().map(|lock| package.root().join(lock)))
        .chain(
            package
                .modules()
                .iter()
                .map(|m| m.canonical_path().to_path_buf()),
        );
    for path in paths {
        if output.as_ref().is_some_and(|o| *o == path) {
            return Err(CliError::new("CGXC004", "output is a package input file"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if let Ok(source) = fs::metadata(&path)
                && source.dev() == info.dev()
                && source.ino() == info.ino()
            {
                return Err(CliError::new("CGXC004", "output is a package input file"));
            }
        }
    }
    Ok(())
}

fn emit_python_package(options: &Options, package: &CheckedPackage) -> Result<(), CliError> {
    if options.output.extension().is_none_or(|ext| ext != "pyz") {
        return Err(CliError::new(
            "CGXC001",
            "Python packages require --out FILE.pyz (a standard Python source zipapp)",
        ));
    }
    let files = cgx2_emit_python::emit_package_python(package).map_err(|errors| {
        let d = &errors[0];
        let mut error = CliError::new(d.code, &d.message);
        error.file = Some(package_path(&options.source, &d.file));
        error.span = d.span;
        error
    })?;
    let bytes =
        python_archive::encode(&files, MAX_GENERATED).map_err(|e| CliError::new("CGXC006", e))?;
    let parent = options
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temporary = PrivateDir::new(parent)?;
    let staged = temporary.path.join("package.pyz");
    fs::write(&staged, bytes).map_err(|e| {
        CliError::new(
            "CGXC006",
            format!("cannot stage Python source archive: {e}"),
        )
    })?;
    package
        .verify_unchanged()
        .map_err(|e| package_error(&options.source, e))?;
    validate_output(options)?;
    validate_package_output(options, package)?;
    install(&staged, &options.output, options.force)
}

fn source_error(path: &Path, code: &str, message: &str, start: usize, end: usize) -> CliError {
    let mut error = CliError::new(code, message);
    error.file = Some(path.to_owned());
    error.span = Some(cgx2_syntax::Span { start, end });
    error
}

fn parse_args(args: Vec<OsString>) -> Result<Options, CliError> {
    let usage = || CliError::new("CGXC001", USAGE);
    if args.len() < 4 {
        return Err(usage());
    }
    let mode = match args[0].to_str() {
        Some("build") => Mode::Build,
        Some("emit-rust") => Mode::Emit,
        Some("emit-python") => Mode::EmitPython,
        _ => return Err(usage()),
    };
    let package = args[1] == "--package";
    let source_index = if package { 2 } else { 1 };
    let Some(source_arg) = args.get(source_index) else {
        return Err(usage());
    };
    let source = PathBuf::from(source_arg);
    if !package && source.extension().is_none_or(|ext| ext != "cg") {
        return Err(usage());
    }
    let mut output = None;
    let mut force = false;
    let mut release = false;
    let mut test_fail_reserve = None;
    let mut rustc = None;
    let mut index = source_index + 1;
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
            Some("--test-fail-reserve")
                if mode == Mode::Build
                    && !package
                    && test_fail_reserve.is_none()
                    && index + 1 < args.len() =>
            {
                let index_value = args[index + 1]
                    .to_str()
                    .and_then(|value| value.parse::<usize>().ok());
                test_fail_reserve = Some(
                    index_value
                        .filter(|value| *value > 0 && *value <= 1_000_000)
                        .ok_or_else(usage)?,
                );
                index += 2;
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
        package,
        output,
        force,
        release,
        test_fail_reserve,
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

fn compile_runtime(
    rustc: &OsString,
    staging: &Path,
    release: bool,
    fault_injection: bool,
) -> Result<(PathBuf, String), CliError> {
    let source_dir = staging.join("runtime-src");
    fs::create_dir(&source_dir).map_err(|e| {
        CliError::new(
            "CGXC006",
            format!("cannot stage embedded cgx2-runtime: {e}"),
        )
    })?;
    let mut hasher = Sha256::new();
    for (name, contents) in RUNTIME_SOURCES {
        hasher.update(name.as_bytes());
        hasher.update([0]);
        hasher.update(contents.as_bytes());
        hasher.update([0]);
        fs::write(source_dir.join(name), contents).map_err(|e| {
            CliError::new(
                "CGXC006",
                format!("cannot stage embedded cgx2-runtime: {e}"),
            )
        })?;
    }
    let hash = format!("{:x}", hasher.finalize());
    let source = source_dir.join("lib.rs");
    let output = staging.join("libcgx2_runtime.rlib");
    let mut command = Command::new(rustc);
    command
        .arg("--edition=2021")
        .arg("--crate-name")
        .arg("cgx2_runtime")
        .arg("--crate-type=lib")
        .arg(&source)
        .arg("-C")
        .arg(if release {
            "opt-level=3"
        } else {
            "opt-level=0"
        })
        .arg("-o")
        .arg(&output);
    if fault_injection {
        command.arg("--cfg").arg("feature=\"fault-injection\"");
    }
    run_rustc(command)?;
    Ok((output, hash))
}

fn compile(
    rustc: &OsString,
    source: &Path,
    output: &Path,
    release: bool,
    runtime: Option<&Path>,
    source_file: Option<&Path>,
    test_fail_reserve: Option<usize>,
) -> Result<(), CliError> {
    let mut command = Command::new(rustc);
    if let Some(source_file) = source_file {
        command.env("CGX2_SOURCE_FILE", source_file.to_string_lossy().as_ref());
    }
    if let Some(index) = test_fail_reserve {
        command.arg("--cfg").arg("feature=\"fault-injection\"");
        command.env("CGX2_TEST_FAIL_RESERVE", index.to_string());
    }
    command
        .arg("--edition=2024")
        .arg(source)
        .arg("-o")
        .arg(output);
    if let Some(runtime) = runtime {
        command
            .arg("--extern")
            .arg(format!("cgx2_runtime={}", runtime.display()));
    }
    command.arg("-C").arg(if release {
        "opt-level=3"
    } else {
        "opt-level=0"
    });
    run_rustc(command)
}

fn run_rustc(mut command: Command) -> Result<(), CliError> {
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
