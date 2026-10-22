//! Linux ELF closure preflight for schema-2 LibTorch bindings.
use crate::Binding;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

const LOCK_NAME: &str = "cgx2-libtorch-closure.lock.json";
const MAX_LOCK_BYTES: u64 = 1024 * 1024;
const MAX_LIBRARIES: usize = 128;
const MAX_LIBRARY_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_CLOSURE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_PATH_BYTES: usize = 4096;
const MAX_INSPECT_BYTES: usize = 1024 * 1024;
const MAX_MAPS_BYTES: u64 = 8 * 1024 * 1024;
const MAX_MAPPED_ELF_FILES: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClosureLock {
    pub schema_version: u32,
    pub libraries: Vec<LibraryPin>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryPin {
    /// `facade` or `dependency`.
    pub role: String,
    /// ELF DT_NEEDED name, or the facade file name.
    pub name: String,
    pub path: PathBuf,
    pub sha256: String,
    pub os: String,
    pub arch: String,
    /// Exact ELF DT_SONAME, if the ELF file declares one.
    pub soname: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClosureError {
    Invalid(String),
    UnverifiedLoader(String),
}

impl fmt::Display for ClosureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(f, "invalid LibTorch closure: {message}"),
            Self::UnverifiedLoader(message) => write!(f, "unverified loader: {message}"),
        }
    }
}
impl std::error::Error for ClosureError {}

fn invalid(message: impl Into<String>) -> ClosureError {
    ClosureError::Invalid(message.into())
}

pub struct VerifiedClosure {
    pins: Vec<(LibraryPin, File)>,
    facade: PathBuf,
}

impl VerifiedClosure {
    pub fn facade_path(&self) -> &Path {
        &self.facade
    }

    /// Reject already mapped conflicting modules before dlopen can run constructors.
    pub fn verify_preloaded(&self) -> Result<(), ClosureError> {
        self.verify_loader_environment()?;
        self.verify_modules(false)
    }

    fn verify_loader_environment(&self) -> Result<(), ClosureError> {
        for variable in ["LD_PRELOAD", "LD_AUDIT"] {
            if std::env::var_os(variable).is_some_and(|value| !value.is_empty()) {
                return Err(ClosureError::UnverifiedLoader(format!(
                    "{variable} can interpose or redirect library loading"
                )));
            }
        }
        if let Some(value) = std::env::var_os("LD_LIBRARY_PATH") {
            for directory in std::env::split_paths(&value) {
                if !directory.is_absolute() || directory.as_os_str().is_empty() {
                    return Err(ClosureError::UnverifiedLoader(
                        "relative or empty LD_LIBRARY_PATH entry".into(),
                    ));
                }
                for (pin, _) in &self.pins {
                    for name in [
                        pin.path.file_name().expect("validated basename"),
                        std::ffi::OsStr::new(&pin.name),
                    ] {
                        let candidate = directory.join(name);
                        match fs::symlink_metadata(&candidate) {
                            Ok(_) => {
                                let actual = fs::canonicalize(&candidate)
                                    .map_err(|error| invalid(error.to_string()))?;
                                if actual != pin.path {
                                    return Err(invalid(format!(
                                        "loader search path substitutes {}",
                                        pin.name
                                    )));
                                }
                            }
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                            Err(error) => {
                                return Err(ClosureError::UnverifiedLoader(format!(
                                    "cannot inspect loader search path: {error}"
                                )))
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Compare mapped file identity with every pinned inode after dlopen and before calls.
    pub fn verify_loaded(&self) -> Result<(), ClosureError> {
        self.verify_modules(true)
    }

    fn verify_modules(&self, require_all: bool) -> Result<(), ClosureError> {
        #[cfg(not(target_os = "linux"))]
        {
            return Err(ClosureError::UnverifiedLoader(
                "loaded module paths are not observable on this platform".into(),
            ));
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::MetadataExt;
            let maps = read_maps()?;
            let mut observed = BTreeSet::new();
            let mut other_elf = BTreeSet::new();
            let mut expected = BTreeMap::new();
            for (index, (pin, _)) in self.pins.iter().enumerate() {
                let basename = pin
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| invalid("non-UTF8 pinned basename"))?;
                for name in [basename, pin.name.as_str()] {
                    if let Some(previous) = expected.insert(name.to_owned(), index) {
                        if previous != index {
                            return Err(invalid(format!("ambiguous mapped module name: {name}")));
                        }
                    }
                }
            }
            for line in maps.lines() {
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields.len() < 6 || !fields[5].starts_with('/') {
                    continue;
                }
                if fields.len() > 7
                    || (fields.len() == 7 && fields[6] != "(deleted)")
                    || fields[5].contains("\\040")
                {
                    return Err(ClosureError::UnverifiedLoader(
                        "ambiguous /proc/self/maps path".into(),
                    ));
                }
                let path = Path::new(fields[5]);
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    return Err(ClosureError::UnverifiedLoader(
                        "non-UTF8 mapped path".into(),
                    ));
                };
                if let Some(&index) = expected.get(name) {
                    let (pin, file) = &self.pins[index];
                    if fields.get(6) == Some(&"(deleted)") {
                        return Err(invalid(format!("mapped library was deleted: {name}")));
                    }
                    let metadata = file
                        .metadata()
                        .map_err(|error| invalid(error.to_string()))?;
                    let mapped_inode: u64 =
                        fields[4].parse().map_err(|_| invalid("bad maps inode"))?;
                    let mapped_dev = fields[3];
                    let dev = metadata.dev();
                    let major = ((dev >> 8) & 0xfff) | ((dev >> 32) & 0xfffff000);
                    let minor = (dev & 0xff) | ((dev >> 12) & 0xffffff00);
                    let expected_dev = format!("{major:02x}:{minor:02x}");
                    if path != pin.path
                        || mapped_inode != metadata.ino()
                        || mapped_dev != expected_dev
                    {
                        return Err(invalid(format!(
                            "loader substituted {} with {}",
                            name,
                            path.display()
                        )));
                    }
                    observed.insert(index);
                } else if name.starts_with("libtorch") || name.starts_with("libc10") {
                    return Err(invalid(format!("unlocked LibTorch module loaded: {name}")));
                } else if fields[1].as_bytes().get(2) == Some(&b'x') {
                    // A preloaded dependency can have any filename. Its ELF SONAME,
                    // rather than a `.so` suffix on the mapped path, can satisfy
                    // the facade's DT_NEEDED before post-load verification runs.
                    other_elf.insert(path.to_path_buf());
                    if other_elf.len() > MAX_MAPPED_ELF_FILES {
                        return Err(ClosureError::UnverifiedLoader(
                            "too many mapped ELF files".into(),
                        ));
                    }
                }
            }
            for path in other_elf {
                let (soname, _) = elf_dynamic(&path, false).map_err(|error| {
                    ClosureError::UnverifiedLoader(format!(
                        "cannot identify mapped ELF {}: {error}",
                        path.display()
                    ))
                })?;
                if let Some(soname) = soname {
                    if expected.contains_key(&soname)
                        || soname.starts_with("libtorch")
                        || soname.starts_with("libc10")
                    {
                        return Err(invalid(format!(
                            "mapped ELF has conflicting SONAME {soname}: {}",
                            path.display()
                        )));
                    }
                }
            }
            if require_all {
                for (index, (pin, _)) in self.pins.iter().enumerate() {
                    if !observed.contains(&index) {
                        return Err(invalid(format!("required module not loaded: {}", pin.name)));
                    }
                }
            }
            for (pin, file) in &self.pins {
                validate_path(&pin.path)?;
                let path_metadata =
                    fs::metadata(&pin.path).map_err(|error| invalid(error.to_string()))?;
                let held_metadata = file
                    .metadata()
                    .map_err(|error| invalid(error.to_string()))?;
                if path_metadata.dev() != held_metadata.dev()
                    || path_metadata.ino() != held_metadata.ino()
                {
                    return Err(invalid(format!("pinned path changed inode: {}", pin.name)));
                }
                let mut handle = file
                    .try_clone()
                    .map_err(|error| invalid(error.to_string()))?;
                if hash_file(&mut handle)? != pin.sha256.to_ascii_lowercase() {
                    return Err(invalid(format!("mapped inode changed bytes: {}", pin.name)));
                }
            }
            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
fn read_maps() -> Result<String, ClosureError> {
    let file = File::open("/proc/self/maps").map_err(|error| {
        ClosureError::UnverifiedLoader(format!("cannot open /proc/self/maps: {error}"))
    })?;
    let mut bytes = Vec::new();
    file.take(MAX_MAPS_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            ClosureError::UnverifiedLoader(format!("cannot read /proc/self/maps: {error}"))
        })?;
    if bytes.len() as u64 > MAX_MAPS_BYTES {
        return Err(ClosureError::UnverifiedLoader(
            "/proc/self/maps exceeds byte limit".into(),
        ));
    }
    String::from_utf8(bytes)
        .map_err(|_| ClosureError::UnverifiedLoader("non-UTF8 /proc/self/maps".into()))
}

/// Verify the complete ELF DT_NEEDED graph before opening the facade.
/// The returned file handles keep the checked inodes alive until loader observation.
pub fn verify_libtorch_closure(
    root: &Path,
    binding: &Binding,
) -> Result<VerifiedClosure, ClosureError> {
    if binding.route != "libtorch" {
        return Err(invalid("binding route must be libtorch"));
    }
    if !cfg!(target_os = "linux") {
        return Err(ClosureError::UnverifiedLoader(
            "ELF closure inspection requires Linux".into(),
        ));
    }
    let artifact = Path::new(&binding.artifact);
    if artifact.is_absolute()
        || artifact
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(invalid("facade artifact must be a confined relative path"));
    }
    let root = fs::canonicalize(root).map_err(|error| invalid(format!("package root: {error}")))?;
    let lock_path = root.join(LOCK_NAME);
    let lock_bytes = read_limited(&lock_path, MAX_LOCK_BYTES)?;
    let lock: ClosureLock = serde_json::from_slice(&lock_bytes)
        .map_err(|error| invalid(format!("strict closure lock JSON: {error}")))?;
    if lock.schema_version != 1 || lock.libraries.is_empty() || lock.libraries.len() > MAX_LIBRARIES
    {
        return Err(invalid("schema or library count"));
    }
    let facade_path = fs::canonicalize(root.join(artifact))
        .map_err(|error| invalid(format!("facade unavailable: {error}")))?;
    if !facade_path.starts_with(&root) {
        return Err(invalid("facade artifact escapes package root"));
    }
    let mut files = Vec::with_capacity(lock.libraries.len());
    let mut needed = BTreeMap::<String, Vec<String>>::new();
    let mut names = BTreeSet::new();
    let mut basenames = BTreeSet::new();
    let mut total_bytes = 0u64;
    let mut facade_name = None;
    for pin in lock.libraries {
        if pin.os != std::env::consts::OS || pin.arch != std::env::consts::ARCH {
            return Err(invalid(format!("target mismatch for {}", pin.name)));
        }
        validate_path(&pin.path)?;
        let basename = pin
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("non-UTF8 library basename"))?;
        if !basenames.insert(basename.to_owned()) {
            return Err(invalid(format!("duplicate library basename: {basename}")));
        }
        if pin.name.is_empty() || pin.name.contains('/') || !names.insert(pin.name.clone()) {
            return Err(invalid(format!(
                "duplicate or invalid logical name: {}",
                pin.name
            )));
        }
        if pin.sha256.len() != 64 || !pin.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(invalid(format!("invalid SHA-256 for {}", pin.name)));
        }
        let mut file =
            File::open(&pin.path).map_err(|error| invalid(format!("{}: {error}", pin.name)))?;
        let size = file
            .metadata()
            .map_err(|error| invalid(error.to_string()))?
            .len();
        total_bytes = total_bytes
            .checked_add(size)
            .ok_or_else(|| invalid("closure size overflow"))?;
        if size > MAX_LIBRARY_BYTES || total_bytes > MAX_CLOSURE_BYTES {
            return Err(invalid("library or closure exceeds byte limit"));
        }
        let actual_hash = hash_file(&mut file)?;
        if actual_hash != pin.sha256.to_ascii_lowercase() {
            return Err(invalid(format!("SHA-256 mismatch for {}", pin.name)));
        }
        let (actual_soname, dependencies) = elf_dynamic(&pin.path, true)?;
        if actual_soname != pin.soname {
            return Err(invalid(format!("SONAME mismatch for {}", pin.name)));
        }
        if pin.name != pin.soname.as_deref().unwrap_or(basename) {
            return Err(invalid(format!(
                "logical name does not match ELF identity: {}",
                pin.name
            )));
        }
        match pin.role.as_str() {
            "facade"
                if facade_name.is_none()
                    && pin.path == facade_path
                    && pin.sha256.eq_ignore_ascii_case(&binding.sha256) =>
            {
                facade_name = Some(pin.name.clone());
            }
            "dependency" if pin.path != facade_path => {}
            _ => {
                return Err(invalid(format!(
                    "invalid role, facade path or facade hash for {}",
                    pin.name
                )))
            }
        }
        needed.insert(pin.name.clone(), dependencies);
        files.push((pin, file));
    }
    let facade_name = facade_name.ok_or_else(|| invalid("facade entry missing"))?;
    let mut reachable = BTreeSet::new();
    let mut pending = vec![facade_name];
    while let Some(name) = pending.pop() {
        if !reachable.insert(name.clone()) {
            continue;
        }
        for dependency in &needed[&name] {
            if !names.contains(dependency) {
                return Err(invalid(format!(
                    "missing DT_NEEDED dependency {dependency} from {name}"
                )));
            }
            pending.push(dependency.clone());
        }
    }
    if reachable != names {
        return Err(invalid("extra unreachable closure entry"));
    }
    Ok(VerifiedClosure {
        pins: files,
        facade: facade_path,
    })
}

fn validate_path(path: &Path) -> Result<(), ClosureError> {
    if !path.is_absolute()
        || path.as_os_str().len() > MAX_PATH_BYTES
        || path
            .components()
            .any(|c| !matches!(c, Component::RootDir | Component::Normal(_)))
    {
        return Err(invalid(format!(
            "path is not canonical absolute: {}",
            path.display()
        )));
    }
    let canonical =
        fs::canonicalize(path).map_err(|error| invalid(format!("{}: {error}", path.display())))?;
    if canonical != path {
        return Err(invalid(format!(
            "symlink or noncanonical path: {}",
            path.display()
        )));
    }
    Ok(())
}

fn read_limited(path: &Path, max: u64) -> Result<Vec<u8>, ClosureError> {
    if fs::symlink_metadata(path)
        .map_err(|error| invalid(format!("{}: {error}", path.display())))?
        .file_type()
        .is_symlink()
    {
        return Err(invalid("closure lock symlink"));
    }
    let file = File::open(path).map_err(|error| invalid(format!("{}: {error}", path.display())))?;
    if file
        .metadata()
        .map_err(|error| invalid(error.to_string()))?
        .len()
        > max
    {
        return Err(invalid("closure lock exceeds byte limit"));
    }
    let mut bytes = Vec::new();
    file.take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| invalid(error.to_string()))?;
    if bytes.len() as u64 > max {
        return Err(invalid("closure lock exceeds byte limit"));
    }
    Ok(bytes)
}

fn hash_file(file: &mut File) -> Result<String, ClosureError> {
    file.seek(SeekFrom::Start(0))
        .map_err(|error| invalid(error.to_string()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| invalid(error.to_string()))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn elf_dynamic(
    path: &Path,
    enforce_search_policy: bool,
) -> Result<(Option<String>, Vec<String>), ClosureError> {
    let mut child = Command::new("readelf")
        .arg("-d")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| invalid(format!("readelf unavailable: {error}")))?;
    let mut bytes = Vec::new();
    let read = child
        .stdout
        .take()
        .expect("piped stdout")
        .take(MAX_INSPECT_BYTES as u64 + 1)
        .read_to_end(&mut bytes);
    if read.is_err() || bytes.len() > MAX_INSPECT_BYTES {
        let _ = child.kill();
        let _ = child.wait();
        return Err(invalid("readelf output exceeds inspection limit"));
    }
    if !child
        .wait()
        .map_err(|error| invalid(error.to_string()))?
        .success()
    {
        return Err(invalid(format!(
            "cannot inspect ELF dynamic section: {}",
            path.display()
        )));
    }
    let output = String::from_utf8(bytes).map_err(|_| invalid("non-UTF8 readelf output"))?;
    if !output.contains("Dynamic section") {
        return Err(invalid(format!(
            "no ELF dynamic section: {}",
            path.display()
        )));
    }
    let mut soname = None;
    let mut needed = Vec::new();
    for line in output.lines() {
        let value = line
            .split_once('[')
            .and_then(|(_, rest)| rest.split_once(']').map(|(value, _)| value));
        if line.contains("(SONAME)") {
            soname = value.map(str::to_owned);
        }
        if line.contains("(NEEDED)") {
            needed.push(
                value
                    .ok_or_else(|| invalid("malformed DT_NEEDED"))?
                    .to_owned(),
            );
        }
        if enforce_search_policy && line.contains("(RPATH)") {
            return Err(invalid(format!(
                "legacy ELF RPATH is not allowed: {}",
                path.display()
            )));
        }
        if enforce_search_policy && line.contains("(RUNPATH)") {
            let runpath = value.ok_or_else(|| invalid("malformed DT_RUNPATH"))?;
            for entry in runpath.split(':') {
                let suffix = entry
                    .strip_prefix("$ORIGIN")
                    .or_else(|| entry.strip_prefix("${ORIGIN}"))
                    .ok_or_else(|| invalid(format!("untrusted DT_RUNPATH: {entry}")))?;
                if !suffix.is_empty() {
                    let relative = suffix
                        .strip_prefix('/')
                        .ok_or_else(|| invalid("bad DT_RUNPATH separator"))?;
                    if relative.is_empty()
                        || Path::new(relative)
                            .components()
                            .any(|part| !matches!(part, Component::Normal(_)))
                    {
                        return Err(invalid(format!("untrusted DT_RUNPATH: {entry}")));
                    }
                }
            }
        }
    }
    Ok((soname, needed))
}
