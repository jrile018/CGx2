//! Offline, non-loading preflight for explicitly pinned native binding packages.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use cgx2_package::load_checked_package;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
mod closure;
pub use closure::{
    verify_libtorch_closure, ClosureError, ClosureLock, LibraryPin, VerifiedClosure,
};

const MAX_LOCK_BYTES: u64 = 1024 * 1024;
const MAX_ARTIFACT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_TOTAL_ARTIFACT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_BINDINGS: usize = 32;
const MAX_SYMBOLS_PER_BINDING: usize = 128;
const MAX_INSPECT_OUTPUT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingLock {
    pub schema_version: u32,
    pub package: String,
    pub bindings: Vec<Binding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub id: String,
    pub route: String,
    pub library: String,
    pub version: String,
    pub artifact: String,
    pub sha256: String,
    pub os: String,
    pub arch: String,
    pub abi: String,
    pub calling_convention: String,
    pub layout: String,
    pub symbols: Vec<Symbol>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Symbol {
    pub name: String,
    pub parameters: Vec<String>,
    pub result: String,
    pub ownership: String,
    pub owner: Option<String>,
    pub release: Option<String>,
    pub error: String,
    pub effects: Vec<String>,
    pub capabilities: Vec<String>,
}

/// Exact C entry points of the version-pinned `torch/torch.h` facade ABI v1.
/// Pointer spellings describe the physical C signature, including output
/// parameters; `owned` means an output handle is released by the named entry.
pub fn libtorch_facade_symbols() -> Vec<Symbol> {
    fn symbol(
        name: &str,
        parameters: &[&str],
        result: &str,
        ownership: &str,
        release: Option<&str>,
        error: &str,
    ) -> Symbol {
        Symbol {
            name: name.into(),
            parameters: parameters.iter().map(|value| (*value).into()).collect(),
            result: result.into(),
            ownership: ownership.into(),
            owner: None,
            release: release.map(str::to_owned),
            error: error.into(),
            effects: vec!["foreign".into()],
            capabilities: vec!["native".into()],
        }
    }
    vec![
        symbol(
            "cgx2_library_identity",
            &[],
            "cstr_ptr",
            "none",
            None,
            "none",
        ),
        symbol(
            "cgx2_library_version",
            &[],
            "cstr_ptr",
            "none",
            None,
            "none",
        ),
        symbol("cgx2_abi_version", &[], "u32", "none", None, "none"),
        symbol(
            "cgx2_torch_last_error",
            &[],
            "cstr_ptr",
            "none",
            None,
            "none",
        ),
        symbol(
            "cgx2_torch_f64_cpu",
            &["f64_const_ptr", "u64", "handle_out_ptr"],
            "i32",
            "owned",
            Some("cgx2_torch_release"),
            "status",
        ),
        symbol(
            "cgx2_torch_add",
            &["handle_const_ptr", "handle_const_ptr", "handle_out_ptr"],
            "i32",
            "owned",
            Some("cgx2_torch_release"),
            "status",
        ),
        symbol(
            "cgx2_torch_length",
            &["handle_const_ptr", "u64_out_ptr"],
            "i32",
            "none",
            None,
            "status",
        ),
        symbol(
            "cgx2_torch_copy_f64",
            &["handle_const_ptr", "f64_mut_ptr", "u64"],
            "i32",
            "none",
            None,
            "status",
        ),
        symbol(
            "cgx2_torch_release",
            &["handle_mut_ptr"],
            "i32",
            "none",
            None,
            "status",
        ),
    ]
}

/// Stable serialization: bindings, symbols, effects and capabilities are sorted.
pub fn render_lock(lock: &BindingLock) -> Result<Vec<u8>, serde_json::Error> {
    let mut lock = lock.clone();
    lock.bindings.sort_by(|a, b| a.id.cmp(&b.id));
    for binding in &mut lock.bindings {
        binding.symbols.sort_by(|a, b| a.name.cmp(&b.name));
        for symbol in &mut binding.symbols {
            symbol.effects.sort();
            symbol.capabilities.sort();
        }
    }
    let mut bytes = serde_json::to_vec_pretty(&lock)?;
    bytes.push(b'\n');
    Ok(bytes)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub code: &'static str,
    pub binding: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub package: String,
    pub status: &'static str,
    pub bindings_checked: usize,
    pub diagnostics: Vec<Diagnostic>,
}

impl Report {
    fn new() -> Self {
        Self {
            schema_version: 1,
            package: String::new(),
            status: "rejected",
            bindings_checked: 0,
            diagnostics: Vec::new(),
        }
    }

    fn push(&mut self, code: &'static str, binding: &str, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            code,
            binding: binding.to_owned(),
            message: message.into(),
        });
    }

    pub fn passed(&self) -> bool {
        self.status == "ok"
    }

    pub fn render_text(&self) -> String {
        let mut out = format!(
            "package {}\nstatus {}\nbindings_checked {}\n",
            self.package, self.status, self.bindings_checked
        );
        for diagnostic in &self.diagnostics {
            out.push_str(&format!(
                "{} [{}] {}\n",
                diagnostic.code, diagnostic.binding, diagnostic.message
            ));
        }
        out
    }

    pub fn render_json(&self) -> String {
        serde_json::to_string(self).expect("report serialization cannot fail")
    }
}

/// Checks local bytes and declared contracts without loading any foreign artifact.
pub fn check_package(root: &Path) -> Report {
    inspect_package(root).0
}

fn inspect_package(root: &Path) -> (Report, Option<BindingLock>) {
    let mut report = Report::new();
    let checked = match load_checked_package(root) {
        Ok(package) => package,
        Err(diagnostics) => {
            for diagnostic in diagnostics {
                report.push(diagnostic.code, "", diagnostic.message);
            }
            return (report, None);
        }
    };
    report.package = checked.index().name.clone();
    let Some(lock_path) = checked.binding_lock() else {
        report.push("CGB001", "", "cgx2.toml schema 2 binding_lock is required");
        return (report, None);
    };
    let root = checked.root();
    let lock_file = match confined_file(root, lock_path) {
        Ok(path) => path,
        Err(message) => {
            report.push("CGB002", "", message);
            return (report, None);
        }
    };
    let bytes = match read_bounded(&lock_file, MAX_LOCK_BYTES) {
        Ok(bytes) => bytes,
        Err(message) => {
            report.push("CGB002", "", message);
            return (report, None);
        }
    };
    let lock: BindingLock = match serde_json::from_slice(&bytes) {
        Ok(lock) => lock,
        Err(error) => {
            report.push(
                "CGB003",
                "",
                format!("invalid strict binding lock: {error}"),
            );
            return (report, None);
        }
    };
    if !matches!(lock.schema_version, 1 | 2)
        || lock.package != report.package
        || lock.bindings.is_empty()
        || lock.bindings.len() > MAX_BINDINGS
        || lock
            .bindings
            .iter()
            .any(|binding| binding.symbols.len() > MAX_SYMBOLS_PER_BINDING)
    {
        report.push(
            "CGB004",
            "",
            "lock schema, package identity, or binding/symbol count is invalid",
        );
        return (report, None);
    }
    let mut bindings = lock.bindings.clone();
    bindings.sort_by(|a, b| a.id.cmp(&b.id));
    let mut seen = BTreeSet::new();
    let mut remaining_artifact_bytes = MAX_TOTAL_ARTIFACT_BYTES;
    for binding in &bindings {
        if !seen.insert(binding.id.as_str()) {
            report.push("CGB004", &binding.id, "duplicate binding id");
        }
        check_binding(root, binding, &mut report, &mut remaining_artifact_bytes);
        if lock.schema_version == 2 && binding.route == "libtorch" {
            if let Err(error) = verify_libtorch_closure(root, binding) {
                report.push("CGB019", &binding.id, error.to_string());
            }
        }
        report.bindings_checked += 1;
    }
    report
        .diagnostics
        .sort_by(|a, b| (&a.binding, a.code, &a.message).cmp(&(&b.binding, b.code, &b.message)));
    if report.diagnostics.is_empty() {
        report.status = "ok";
    }
    (report, Some(lock))
}

/// Return a lock only after the complete, offline package preflight succeeds.
pub fn checked_lock(root: &Path) -> Result<BindingLock, Report> {
    let (report, lock) = inspect_package(root);
    if report.passed() {
        Ok(lock.expect("successful preflight parsed lock"))
    } else {
        Err(report)
    }
}

/// Hold the checked artifact inode while the native loader opens it.
/// Call this after `checked_lock`; it repeats the hash and confinement check.
pub fn checked_artifact(root: &Path, binding: &Binding) -> Result<File, String> {
    let path = confined_file(root, Path::new(&binding.artifact))?;
    let mut file = File::open(&path).map_err(|error| format!("cannot open artifact: {error}"))?;
    let size = file
        .metadata()
        .map_err(|error| format!("cannot stat artifact: {error}"))?
        .len();
    if size > MAX_ARTIFACT_BYTES {
        return Err("artifact exceeds size limit".into());
    }
    let mut bytes = Vec::with_capacity(size as usize);
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read artifact: {error}"))?;
    if format!("{:x}", Sha256::digest(&bytes)) != binding.sha256.to_ascii_lowercase() {
        return Err("artifact SHA-256 mismatch".into());
    }
    Ok(file)
}

fn check_binding(
    root: &Path,
    binding: &Binding,
    report: &mut Report,
    remaining_artifact_bytes: &mut u64,
) {
    let id = &binding.id;
    if !identifier(id)
        || (!identifier(&binding.library)
            && !(binding.route == "libtorch" && binding.library == "cgx2.libtorch.cpu"))
        || !exact_version(&binding.version)
    {
        report.push(
            "CGB005",
            id,
            "binding id, library or exact version is invalid",
        );
    }
    if !matches!(binding.route.as_str(), "native-c" | "libtorch")
        || !matches!(binding.abi.as_str(), "c" | "cgx2-libtorch-facade-v1")
        || (binding.route == "native-c" && binding.abi != "c")
        || (binding.route == "libtorch" && binding.abi != "cgx2-libtorch-facade-v1")
        || binding.calling_convention != "c"
        || !matches!(
            binding.layout.as_str(),
            "scalar-v1" | "status-i64-out-v1" | "tensor-f64-cpu-v1"
        )
        || (binding.route == "libtorch" && binding.layout != "tensor-f64-cpu-v1")
        || (binding.route == "native-c" && binding.layout == "tensor-f64-cpu-v1")
        || (binding.layout == "status-i64-out-v1" && binding.route != "native-c")
    {
        report.push(
            "CGB006",
            id,
            "route, ABI, calling convention and layout must be an approved native combination",
        );
    }
    if binding.os != std::env::consts::OS || binding.arch != std::env::consts::ARCH {
        report.push(
            "CGB007",
            id,
            format!(
                "target {}/{} differs from host {}/{}",
                binding.os,
                binding.arch,
                std::env::consts::OS,
                std::env::consts::ARCH
            ),
        );
    }
    if binding.sha256.len() != 64 || !binding.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        report.push("CGB008", id, "sha256 must be 64 hexadecimal digits");
    }
    if binding.symbols.is_empty() {
        report.push("CGB009", id, "at least one symbol is required");
    }
    if binding.route == "libtorch" {
        let mut declared = binding.symbols.clone();
        let mut expected = libtorch_facade_symbols();
        declared.sort_by(|left, right| left.name.cmp(&right.name));
        expected.sort_by(|left, right| left.name.cmp(&right.name));
        if binding.library != "cgx2.libtorch.cpu" || declared != expected {
            report.push("CGB018", id, "LibTorch route requires exact cgx2.libtorch.cpu facade v1 exports and C signatures");
        }
    } else {
        let mut names = BTreeSet::new();
        for symbol in &binding.symbols {
            if binding.layout == "status-i64-out-v1"
                && (symbol.parameters != ["i64"]
                    || symbol.result != "i64"
                    || symbol.ownership != "none"
                    || symbol.owner.is_some()
                    || symbol.release.is_some()
                    || symbol.error != "status"
                    || symbol.effects != ["foreign"]
                    || symbol.capabilities != ["native"])
            {
                report.push(
                    "CGB017",
                    id,
                    format!("{} is not the status-i64-out-v1 C shape", symbol.name),
                );
            }
            if !identifier(&symbol.name) || !names.insert(symbol.name.as_str()) {
                report.push(
                    "CGB009",
                    id,
                    format!("invalid or duplicate symbol {}", symbol.name),
                );
            }
            if !symbol.parameters.iter().all(|t| supported_type(t, false))
                || !supported_type(&symbol.result, true)
            {
                report.push(
                    "CGB010",
                    id,
                    format!("unsupported type or layout for {}", symbol.name),
                );
            }
            if !matches!(symbol.ownership.as_str(), "none" | "borrowed" | "owned")
                || (symbol.ownership == "owned"
                    && (symbol.result != "opaque_handle"
                        || symbol.owner.is_some()
                        || symbol.release.as_ref().is_none_or(|name| !identifier(name))))
                || (symbol.ownership == "borrowed"
                    && (symbol.result != "opaque_handle"
                        || symbol.owner.as_deref() != Some("caller")))
                || (symbol.ownership != "owned" && symbol.release.is_some())
                || (symbol.ownership == "none" && symbol.owner.is_some())
            {
                report.push(
                    "CGB011",
                    id,
                    format!("invalid ownership/release for {}", symbol.name),
                );
            }
            if !matches!(symbol.error.as_str(), "none" | "status") {
                report.push(
                    "CGB012",
                    id,
                    format!("unsupported error convention for {}", symbol.name),
                );
            }
            if symbol.effects.is_empty()
                || symbol.effects.iter().collect::<BTreeSet<_>>().len() != symbol.effects.len()
                || !symbol
                    .effects
                    .iter()
                    .all(|e| matches!(e.as_str(), "foreign" | "io" | "allocation" | "gpu"))
                || !symbol.effects.iter().any(|e| e == "foreign")
                || symbol.capabilities.is_empty()
                || symbol.capabilities.iter().collect::<BTreeSet<_>>().len()
                    != symbol.capabilities.len()
                || !symbol
                    .capabilities
                    .iter()
                    .all(|c| matches!(c.as_str(), "native" | "gpu"))
                || !symbol.capabilities.iter().any(|c| c == "native")
                || (symbol.effects.iter().any(|e| e == "gpu")
                    != symbol.capabilities.iter().any(|c| c == "gpu"))
            {
                report.push(
                    "CGB013",
                    id,
                    format!("invalid effects/capabilities for {}", symbol.name),
                );
            }
        }
        for symbol in &binding.symbols {
            if let Some(release) = &symbol.release {
                let valid = binding.symbols.iter().any(|candidate| {
                    candidate.name == *release
                        && candidate.parameters == ["opaque_handle"]
                        && candidate.result == "void"
                        && candidate.ownership == "none"
                });
                if !valid {
                    report.push(
                        "CGB011",
                        id,
                        format!("release symbol {release} is absent or has an incompatible shape"),
                    );
                }
            }
        }
    }
    let artifact = match confined_file(root, Path::new(&binding.artifact)) {
        Ok(path) => path,
        Err(message) => {
            report.push("CGB014", id, message);
            return;
        }
    };
    let bytes = match read_bounded(&artifact, MAX_ARTIFACT_BYTES.min(*remaining_artifact_bytes)) {
        Ok(bytes) => bytes,
        Err(message) => {
            report.push("CGB014", id, message);
            return;
        }
    };
    *remaining_artifact_bytes -= bytes.len() as u64;
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if actual != binding.sha256.to_ascii_lowercase() {
        report.push("CGB008", id, "artifact SHA-256 mismatch");
        return;
    }
    if binding.os == "linux" && std::env::consts::OS == "linux" {
        match exported_symbols(&artifact) {
            Ok(exports) => {
                if binding.layout == "status-i64-out-v1" || binding.route == "libtorch" {
                    for metadata in [
                        "cgx2_library_identity",
                        "cgx2_library_version",
                        "cgx2_abi_version",
                    ] {
                        if !exports.contains(metadata) {
                            report.push(
                                "CGB015",
                                id,
                                format!("missing exported symbol {metadata}"),
                            );
                        }
                    }
                }
                for symbol in &binding.symbols {
                    if !exports.contains(symbol.name.as_str()) {
                        report.push(
                            "CGB015",
                            id,
                            format!("missing exported symbol {}", symbol.name),
                        );
                    }
                }
            }
            Err(message) => report.push("CGB016", id, message),
        }
    } else {
        report.push(
            "CGB016",
            id,
            "static exported-symbol inspection unavailable on this platform",
        );
    }
}

fn identifier(value: &str) -> bool {
    value.len() <= 128
        && value
            .as_bytes()
            .first()
            .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn exact_version(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn supported_type(value: &str, result: bool) -> bool {
    matches!(
        value,
        "i8" | "u8"
            | "i16"
            | "u16"
            | "i32"
            | "u32"
            | "i64"
            | "u64"
            | "f32"
            | "f64"
            | "opaque_handle"
    ) || (result && value == "void")
}

fn confined_file(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let value = path.to_string_lossy();
    if value.is_empty()
        || value.len() > 240
        || value.contains('\\')
        || value.chars().any(char::is_control)
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(format!("invalid relative path {value}"));
    }
    let target = root.join(path);
    let canonical = fs::canonicalize(&target)
        .map_err(|error| format!("artifact unavailable {}: {error}", target.display()))?;
    if !canonical.starts_with(root) || !canonical.is_file() {
        return Err(format!(
            "path escapes package root or is not a file: {value}"
        ));
    }
    Ok(canonical)
}

fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    let file =
        File::open(path).map_err(|error| format!("cannot open {}: {error}", path.display()))?;
    if file.metadata().map_err(|error| error.to_string())?.len() > max {
        return Err(format!("{} exceeds {max} bytes", path.display()));
    }
    let mut bytes = Vec::new();
    file.take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > max {
        return Err(format!("{} exceeds {max} bytes", path.display()));
    }
    Ok(bytes)
}

fn exported_symbols(path: &Path) -> Result<BTreeSet<String>, String> {
    // `nm` inspects ELF bytes; it never links or runs artifact code.
    let mut child = Command::new("nm")
        .arg("-D")
        .arg("--defined-only")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("static symbol inspector unavailable: {error}"))?;
    let mut bytes = Vec::new();
    let read_result = child
        .stdout
        .take()
        .ok_or_else(|| "static symbol inspector has no stdout".to_owned())?
        .take(MAX_INSPECT_OUTPUT_BYTES + 1)
        .read_to_end(&mut bytes);
    if read_result.is_err() || bytes.len() as u64 > MAX_INSPECT_OUTPUT_BYTES {
        let _ = child.kill();
        let _ = child.wait();
        return Err("static symbol inspection exceeded output limit or failed".to_owned());
    }
    if !child.wait().map_err(|error| error.to_string())?.success() {
        return Err("static symbol inspection failed".to_owned());
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| "static symbol inspector returned non UTF-8 output".to_owned())?;
    Ok(text
        .lines()
        .filter_map(|line| line.split_whitespace().last())
        .map(|name| name.split('@').next().unwrap_or(name).to_owned())
        .collect())
}
