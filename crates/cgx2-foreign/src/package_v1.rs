//! Versioned, explicit pinned pure-Python package route (trusted Linux embedder).
//! This API does not extend the source grammar or the scalar child protocol.
use crate::call::InputBudget;
use cgx2_python_bridge::{
    BridgeError, BridgeValue, MAX_KEYWORDS, MAX_VALUE_DEPTH, PinnedInvocationV1, PythonBridge,
    PythonSourceV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Take};
use std::path::{Component, Path, PathBuf};

pub const MAX_PACKAGE_LOCK_BYTES: usize = 1_048_576;
pub const MAX_SOURCE_BYTES: usize = 1_048_576;
pub const MAX_SNAPSHOT_BYTES: usize = 16_777_216;
pub const MAX_PACKAGE_FILES: usize = 4096;
pub const MAX_ARTIFACT_BYTES: u64 = 268_435_456;

#[derive(Debug)]
pub enum PackageErrorV1 {
    Manifest(String),
    Identity(String),
    Authorization(String),
    Argument(String),
    Result(String),
    Limit(String),
    Python(BridgeError),
}
impl std::fmt::Display for PackageErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manifest(s) => write!(f, "package manifest: {s}"),
            Self::Identity(s) => write!(f, "package identity: {s}"),
            Self::Authorization(s) => write!(f, "package authorization: {s}"),
            Self::Argument(s) => write!(f, "package argument: {s}"),
            Self::Result(s) => write!(f, "package result: {s}"),
            Self::Limit(s) => write!(f, "package limit: {s}"),
            Self::Python(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for PackageErrorV1 {}
type Result<T> = std::result::Result<T, PackageErrorV1>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactPinV1 {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimePinV1 {
    pub interpreter: ArtifactPinV1,
    pub library: ArtifactPinV1,
    pub version: String,
    pub prefix: PathBuf,
    pub base_prefix: PathBuf,
    /// Each directory is tree-hashed; site packages and bytecode caches are excluded.
    /// Empty means no filesystem stdlib search. Built-in/frozen modules remain available.
    pub stdlib: Vec<ArtifactPinV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePinV1 {
    pub module: String,
    /// Relative to the distribution root; symlink components are rejected.
    pub path: PathBuf,
    pub sha256: String,
    pub is_package: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistributionPinV1 {
    pub name: String,
    pub version: String,
    pub root: PathBuf,
    /// Relative path of wheel METADATA (or an equivalent local fixture metadata file).
    pub metadata: ArtifactPinV1,
    /// Exact locked distribution names, including transitive dependencies.
    pub dependencies: Vec<String>,
    pub sources: Vec<SourcePinV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "items",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum CopiedTypeV1 {
    None,
    Bool,
    I64,
    F64,
    String,
    Bytes,
    List(Box<CopiedTypeV1>),
    Tuple(Vec<CopiedTypeV1>),
    Dict(Box<CopiedTypeV1>),
    F64Array1dCopyV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallableV1 {
    pub module: String,
    /// One public module attribute; attribute chains/descriptors are not admitted.
    pub attribute: String,
    pub positional: Vec<CopiedTypeV1>,
    /// All listed keywords are required and no others are permitted.
    pub keywords: BTreeMap<String, CopiedTypeV1>,
    pub returns: CopiedTypeV1,
    pub effect: String,
    pub capability: String,
    pub ownership: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageEnvironmentV1 {
    pub schema_version: u32,
    pub environment_id: String,
    pub os: String,
    pub arch: String,
    pub runtime: RuntimePinV1,
    pub distributions: Vec<DistributionPinV1>,
    pub calls: BTreeMap<String, CallableV1>,
}

/// The embedding application's explicit grant. This is an allowlist, not OS confinement.
/// S2 must construct it only from checked Foreign/ForeignCall source capabilities.
pub struct PackageAuthorizationV1 {
    pub environment_id: String,
    pub allowed_calls: BTreeSet<String>,
}

impl PackageEnvironmentV1 {
    /// Authenticate every artifact before isolated CPython initialization. Linux v1.
    /// Refuses an already initialized interpreter; never falls back to `PythonBridge::new`.
    ///
    /// # Safety
    /// The embedding application must call this once at startup before any thread or
    /// library uses CPython/PyO3. See `PythonBridge::initialize_isolated_v1`.
    #[cfg(target_os = "linux")]
    pub unsafe fn start_isolated(&self) -> Result<PythonBridge> {
        self.validate()?;
        if self.os != std::env::consts::OS || self.arch != std::env::consts::ARCH {
            return Err(PackageErrorV1::Identity("OS/architecture mismatch".into()));
        }
        let (_, stdlib) = self.snapshot()?;
        for pin in &self.runtime.stdlib {
            // Startup's encodings import precedes our scoped source-only importer.
            // Therefore isolated startup requires cache-free sealed stdlib copies.
            ensure_cache_free(&pin.path)?;
        }
        // SAFETY: forwarded startup exclusivity requirement is the caller's contract.
        let bridge = unsafe {
            PythonBridge::initialize_isolated_v1(
                &self.runtime.interpreter.path,
                &self.runtime.library.path,
                &self.runtime.version,
                &stdlib,
            )
        }
        .map_err(PackageErrorV1::Python)?;
        self.preflight(&bridge)?;
        Ok(bridge)
    }

    /// Parsing is Python-free. Unknown/duplicate struct fields and invalid schemas reject.
    /// The caller must authenticate the lock itself (for example, a package-lock digest).
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_PACKAGE_LOCK_BYTES {
            return Err(PackageErrorV1::Limit("lock exceeds 1 MiB".into()));
        }
        let strict: crate::declaration::StrictValue =
            serde_json::from_slice(bytes).map_err(|e| PackageErrorV1::Manifest(e.to_string()))?;
        let lock: Self = serde_json::from_value(strict.0)
            .map_err(|e| PackageErrorV1::Manifest(e.to_string()))?;
        lock.validate()?;
        Ok(lock)
    }

    pub fn validate(&self) -> Result<()> {
        let bad = |s: &str| PackageErrorV1::Manifest(s.into());
        if self.schema_version != 1 || self.environment_id.is_empty() {
            return Err(bad("schema_version must be 1 and environment_id nonempty"));
        }
        if self.distributions.is_empty() || self.calls.is_empty() {
            return Err(bad("distributions and calls must be nonempty"));
        }
        let mut names = BTreeSet::new();
        let mut modules = BTreeMap::new();
        let mut roots = BTreeMap::new();
        for package in &self.distributions {
            if package.name.is_empty()
                || normalized_name(&package.name) != package.name
                || package.version.is_empty()
                || !names.insert(&package.name)
            {
                return Err(bad("empty or duplicate distribution identity"));
            }
            canonical_absolute(&package.root)?;
            check_hash(&package.metadata.sha256)?;
            relative(&package.metadata.path)?;
            if package.sources.is_empty() {
                return Err(bad("each distribution needs pinned sources"));
            }
            for source in &package.sources {
                if !module_name(&source.module) {
                    return Err(bad("invalid module name"));
                }
                relative(&source.path)?;
                if source.path.extension().is_none_or(|s| s != "py") {
                    return Err(bad(
                        "v1 admits .py source only, no extensions/bytecode/resources",
                    ));
                }
                check_hash(&source.sha256)?;
                if modules.insert(&source.module, source.is_package).is_some() {
                    return Err(bad("duplicate module"));
                }
                let root = source.module.split('.').next().expect("nonempty");
                if let Some(owner) = roots.insert(root, &package.name)
                    && owner != &package.name
                {
                    return Err(bad("a top-level namespace cannot span distributions"));
                }
            }
        }
        if modules.len() > MAX_PACKAGE_FILES {
            return Err(PackageErrorV1::Limit("too many package modules".into()));
        }
        for name in modules.keys() {
            if let Some((parent, _)) = name.rsplit_once('.')
                && modules.get(&parent.to_string()) != Some(&true)
            {
                return Err(bad("submodule requires a pinned package parent"));
            }
        }
        for package in &self.distributions {
            let mut dependencies = BTreeSet::new();
            for name in &package.dependencies {
                if !names.contains(name) || name == &package.name || !dependencies.insert(name) {
                    return Err(bad("missing, duplicate or self distribution dependency"));
                }
            }
        }
        for (key, call) in &self.calls {
            if key.is_empty()
                || !modules.contains_key(&call.module)
                || !identifier(&call.attribute)
                || call.attribute.starts_with('_')
                || call.effect != "Foreign"
                || call.capability != "ForeignCall"
                || call.ownership != "copied"
            {
                return Err(bad(
                    "call requires a pinned module, public attribute, Foreign/ForeignCall and copied ownership",
                ));
            }
            if call.keywords.len() > MAX_KEYWORDS
                || call.positional.len() > 4096
                || call.keywords.keys().any(|k| !identifier(k))
            {
                return Err(bad("invalid signature arity or keyword"));
            }
            for shape in call
                .positional
                .iter()
                .chain(call.keywords.values())
                .chain([&call.returns])
            {
                shape.validate(0)?;
            }
        }
        for artifact in [&self.runtime.interpreter, &self.runtime.library]
            .into_iter()
            .chain(&self.runtime.stdlib)
        {
            canonical_absolute(&artifact.path)?;
            check_hash(&artifact.sha256)?;
        }
        canonical_absolute(&self.runtime.prefix)?;
        canonical_absolute(&self.runtime.base_prefix)?;
        Ok(())
    }

    /// Rechecks identity and creates fresh immutable source snapshots for EVERY call.
    /// No imports of pinned packages occur until all grants, arguments and pins pass.
    pub fn invoke(
        &self,
        bridge: &PythonBridge,
        grant: &PackageAuthorizationV1,
        key: &str,
        positional: &[BridgeValue],
        keyword: &[(&str, BridgeValue)],
    ) -> Result<BridgeValue> {
        self.validate()?;
        if grant.environment_id != self.environment_id || !grant.allowed_calls.contains(key) {
            return Err(PackageErrorV1::Authorization(
                "environment or call not granted".into(),
            ));
        }
        let call = self
            .calls
            .get(key)
            .ok_or_else(|| PackageErrorV1::Authorization("undeclared call".into()))?;
        if positional.len() != call.positional.len() || keyword.len() != call.keywords.len() {
            return Err(PackageErrorV1::Argument("signature arity mismatch".into()));
        }
        let mut budget = InputBudget::default();
        let mut keys = BTreeSet::new();
        for (value, shape) in positional.iter().zip(&call.positional) {
            check_input(shape, value, &mut budget)?;
        }
        for (name, value) in keyword {
            let shape = call
                .keywords
                .get(*name)
                .ok_or_else(|| PackageErrorV1::Argument("undeclared keyword".into()))?;
            if !keys.insert(name) {
                return Err(PackageErrorV1::Argument("duplicate keyword".into()));
            }
            budget
                .add_bytes(name.len())
                .map_err(|e| PackageErrorV1::Limit(e.to_string()))?;
            check_input(shape, value, &mut budget)?;
        }
        let (sources, stdlib) = self.preflight(bridge)?;
        let copied = bridge
            .call_pinned_copied_v1(&PinnedInvocationV1 {
                sources: &sources,
                stdlib_paths: &stdlib,
                module: &call.module,
                attribute: &call.attribute,
                positional,
                keyword,
                array_result: call.returns == CopiedTypeV1::F64Array1dCopyV1,
                input_bytes: budget.bytes + budget.array_bytes,
            })
            .map_err(PackageErrorV1::Python)?;
        if !call.returns.matches(&copied) {
            return Err(PackageErrorV1::Result(
                "value does not match declared copied result".into(),
            ));
        }
        Ok(copied)
    }

    fn preflight(&self, bridge: &PythonBridge) -> Result<(Vec<PythonSourceV1>, Vec<String>)> {
        if self.os != std::env::consts::OS || self.arch != std::env::consts::ARCH {
            return Err(PackageErrorV1::Identity("OS/architecture mismatch".into()));
        }
        let actual = bridge
            .embedded_identity_v1()
            .map_err(PackageErrorV1::Python)?;
        if actual.executable != self.runtime.interpreter.path
            || actual.library != self.runtime.library.path
            || actual.version != self.runtime.version
            || actual.prefix != self.runtime.prefix
            || actual.base_prefix != self.runtime.base_prefix
        {
            return Err(PackageErrorV1::Identity(
                "linked interpreter, library, version or environment prefix mismatch".into(),
            ));
        }
        self.snapshot()
    }

    fn snapshot(&self) -> Result<(Vec<PythonSourceV1>, Vec<String>)> {
        verify_file(&self.runtime.interpreter)?;
        verify_file(&self.runtime.library)?;
        let mut stdlib = Vec::new();
        for pin in &self.runtime.stdlib {
            if pin.path.parent().is_none()
                || pin.path.components().any(|c| {
                    matches!(
                        c.as_os_str().to_str(),
                        Some("site-packages" | "dist-packages" | "__pycache__")
                    )
                })
                || hash_stdlib_tree_v1(&pin.path)? != pin.sha256
            {
                return Err(PackageErrorV1::Identity(
                    "stdlib directory/hash mismatch".into(),
                ));
            }
            stdlib.push(
                pin.path
                    .to_str()
                    .ok_or_else(|| PackageErrorV1::Manifest("stdlib path must be UTF-8".into()))?
                    .to_owned(),
            );
        }
        let mut sources = Vec::new();
        let mut total = 0usize;
        for package in &self.distributions {
            let metadata = package_file(&package.root, &package.metadata.path)?;
            let bytes = read_snapshot(&metadata, MAX_SOURCE_BYTES)?;
            verify_bytes(&bytes, &package.metadata.sha256)?;
            let text = std::str::from_utf8(&bytes)
                .map_err(|_| PackageErrorV1::Identity("METADATA is not UTF-8".into()))?;
            let field = |name: &str| {
                text.lines()
                    .filter_map(|l| l.strip_prefix(name))
                    .map(str::trim)
                    .collect::<Vec<_>>()
            };
            if field("Name:") != [&package.name] || field("Version:") != [&package.version] {
                return Err(PackageErrorV1::Identity(
                    "distribution name/version mismatch".into(),
                ));
            }
            // No silent marker/extras resolver: v1 conservatively locks EVERY named
            // Requires-Dist dependency, including optional and platform-marked rows.
            let required: BTreeSet<_> = field("Requires-Dist:")
                .iter()
                .map(|row| {
                    normalized_name(
                        &row.chars()
                            .take_while(|c| {
                                c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
                            })
                            .collect::<String>(),
                    )
                })
                .collect();
            if required != package.dependencies.iter().cloned().collect() {
                return Err(PackageErrorV1::Identity("METADATA dependency closure differs from lock (v1 requires all conditional/extra dependencies too)".into()));
            }
            for pin in &package.sources {
                let path = package_file(&package.root, &pin.path)?;
                let source = read_snapshot(&path, MAX_SOURCE_BYTES)?;
                verify_bytes(&source, &pin.sha256)?;
                total = total
                    .checked_add(source.len())
                    .ok_or_else(|| PackageErrorV1::Limit("snapshot overflow".into()))?;
                if total > MAX_SNAPSHOT_BYTES {
                    return Err(PackageErrorV1::Limit(
                        "package snapshots exceed 16 MiB".into(),
                    ));
                }
                sources.push(PythonSourceV1 {
                    module: pin.module.clone(),
                    filename: path.to_string_lossy().into_owned(),
                    is_package: pin.is_package,
                    source,
                });
            }
        }
        Ok((sources, stdlib))
    }
}

impl CopiedTypeV1 {
    fn validate(&self, depth: usize) -> Result<()> {
        if depth > MAX_VALUE_DEPTH {
            return Err(PackageErrorV1::Limit("signature nesting exceeds 16".into()));
        }
        match self {
            Self::F64Array1dCopyV1 if depth != 0 => {
                Err(PackageErrorV1::Manifest("arrays must be top-level".into()))
            }
            Self::List(t) | Self::Dict(t) => t.validate(depth + 1),
            Self::Tuple(ts) => {
                if ts.len() > 4096 {
                    return Err(PackageErrorV1::Limit("tuple signature exceeds 4096".into()));
                }
                for t in ts {
                    t.validate(depth + 1)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn matches(&self, value: &BridgeValue) -> bool {
        match (self, value) {
            (Self::None, BridgeValue::None)
            | (Self::Bool, BridgeValue::Bool(_))
            | (Self::I64, BridgeValue::I64(_))
            | (Self::String, BridgeValue::String(_))
            | (Self::Bytes, BridgeValue::Bytes(_))
            | (Self::F64Array1dCopyV1, BridgeValue::F64Array1DCopyV1(_)) => true,
            (Self::F64, BridgeValue::F64(v)) => v.is_finite(),
            (Self::List(t), BridgeValue::List(v)) => v.iter().all(|v| t.matches(v)),
            (Self::Dict(t), BridgeValue::Dict(v)) => v.values().all(|v| t.matches(v)),
            (Self::Tuple(ts), BridgeValue::Tuple(v)) => {
                ts.len() == v.len() && ts.iter().zip(v).all(|(t, v)| t.matches(v))
            }
            _ => false,
        }
    }
}

fn check_input(shape: &CopiedTypeV1, value: &BridgeValue, budget: &mut InputBudget) -> Result<()> {
    // Budget traversal MUST precede recursive shape matching on caller-provided values.
    budget
        .check(value, 0)
        .map_err(|e| PackageErrorV1::Limit(e.to_string()))?;
    if !shape.matches(value) {
        return Err(PackageErrorV1::Argument(
            "value does not match copied signature".into(),
        ));
    }
    Ok(())
}
fn identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
fn module_name(s: &str) -> bool {
    s.split('.').all(identifier)
}
fn normalized_name(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        if matches!(c, '-' | '_' | '.') {
            if !result.ends_with('-') {
                result.push('-');
            }
        } else {
            result.push(c.to_ascii_lowercase());
        }
    }
    result
}
fn check_hash(s: &str) -> Result<()> {
    if s.len() != 64
        || !s
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(PackageErrorV1::Manifest(
            "sha256 must be 64 lowercase hex digits".into(),
        ));
    }
    Ok(())
}
fn canonical_absolute(path: &Path) -> Result<()> {
    if !path.is_absolute() || fs::canonicalize(path).ok().as_deref() != Some(path) {
        return Err(PackageErrorV1::Identity(format!(
            "path must exist and be canonical absolute: {}",
            path.display()
        )));
    }
    Ok(())
}
fn relative(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(PackageErrorV1::Manifest(
            "file paths must be nonempty relative normal components".into(),
        ));
    }
    Ok(())
}
fn package_file(root: &Path, relative_path: &Path) -> Result<PathBuf> {
    relative(relative_path)?;
    let mut path = root.to_path_buf();
    for component in relative_path.components() {
        path.push(component);
        let metadata =
            fs::symlink_metadata(&path).map_err(|e| PackageErrorV1::Identity(e.to_string()))?;
        if metadata.file_type().is_symlink() {
            return Err(PackageErrorV1::Identity(
                "symlink in package file path".into(),
            ));
        }
    }
    Ok(path)
}
fn read_snapshot(path: &Path, max: usize) -> Result<Vec<u8>> {
    let file = File::open(path).map_err(|e| PackageErrorV1::Identity(e.to_string()))?;
    if !file.metadata().is_ok_and(|m| m.is_file()) {
        return Err(PackageErrorV1::Identity(
            "artifact must be a regular file".into(),
        ));
    }
    let mut bytes = Vec::new();
    file.take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| PackageErrorV1::Identity(e.to_string()))?;
    if bytes.len() > max {
        return Err(PackageErrorV1::Limit(
            "source/metadata file too large".into(),
        ));
    }
    Ok(bytes)
}
fn verify_bytes(bytes: &[u8], expected: &str) -> Result<()> {
    if format!("{:x}", Sha256::digest(bytes)) != expected {
        return Err(PackageErrorV1::Identity("artifact sha256 mismatch".into()));
    }
    Ok(())
}
fn verify_file(pin: &ArtifactPinV1) -> Result<()> {
    if hash_file_v1(&pin.path)? != pin.sha256 {
        return Err(PackageErrorV1::Identity(format!(
            "artifact sha256 mismatch: {}",
            pin.path.display()
        )));
    }
    Ok(())
}

fn ensure_cache_free(root: &Path) -> Result<()> {
    let mut pending = vec![root.to_path_buf()];
    let mut entries = 0usize;
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path).map_err(|e| PackageErrorV1::Identity(e.to_string()))? {
            let path = entry
                .map_err(|e| PackageErrorV1::Identity(e.to_string()))?
                .path();
            if matches!(
                path.file_name().and_then(|s| s.to_str()),
                Some("site-packages" | "dist-packages")
            ) {
                continue;
            }
            entries += 1;
            if entries > 16384 {
                return Err(PackageErrorV1::Limit("stdlib entry limit".into()));
            }
            if path.file_name().is_some_and(|n| n == "__pycache__")
                || matches!(
                    path.extension().and_then(|s| s.to_str()),
                    Some("pyc" | "pyo")
                )
            {
                return Err(PackageErrorV1::Identity(
                    "isolated startup requires a cache-free stdlib snapshot".into(),
                ));
            }
            if path.is_dir() {
                pending.push(path);
            }
        }
    }
    Ok(())
}

/// Bounded, streaming SHA-256 helper for operator lock preparation. Never executes Python.
pub fn hash_file_v1(path: &Path) -> Result<String> {
    let file = File::open(path).map_err(|e| PackageErrorV1::Identity(e.to_string()))?;
    if !file.metadata().is_ok_and(|m| m.is_file()) {
        return Err(PackageErrorV1::Identity(
            "artifact must be a regular file".into(),
        ));
    }
    let mut input: Take<File> = file.take(MAX_ARTIFACT_BYTES + 1);
    let mut hasher = Sha256::new();
    let mut total = 0;
    let mut buf = [0; 65536];
    loop {
        let n = input
            .read(&mut buf)
            .map_err(|e| PackageErrorV1::Identity(e.to_string()))?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > MAX_ARTIFACT_BYTES {
            return Err(PackageErrorV1::Limit("artifact exceeds 256 MiB".into()));
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Tree digest v1: sorted UTF-8 relative filename + NUL + file SHA256 hex + LF.
/// Excludes __pycache__, site-packages and dist-packages; rejects all other symlinks.
/// Only .py/.so/runtime data are used; .pyc/.pyo reject (except excluded caches).
pub fn hash_stdlib_tree_v1(root: &Path) -> Result<String> {
    canonical_absolute(root)?;
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    let mut entries = 0usize;
    let mut total = 0u64;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).map_err(|e| PackageErrorV1::Identity(e.to_string()))? {
            let entry = entry.map_err(|e| PackageErrorV1::Identity(e.to_string()))?;
            if matches!(
                entry.file_name().to_str(),
                Some("__pycache__" | "site-packages" | "dist-packages")
            ) {
                continue;
            }
            entries += 1;
            if entries > 16384 {
                return Err(PackageErrorV1::Limit(
                    "stdlib tree exceeds 16384 entries".into(),
                ));
            }
            let path = entry.path();
            let metadata =
                fs::symlink_metadata(&path).map_err(|e| PackageErrorV1::Identity(e.to_string()))?;
            if metadata.is_dir() {
                pending.push(path);
                continue;
            }
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || matches!(
                    path.extension().and_then(|s| s.to_str()),
                    Some("pyc" | "pyo")
                )
            {
                return Err(PackageErrorV1::Identity(
                    "unsupported stdlib tree entry (symlink/bytecode/special file)".into(),
                ));
            }
            total = total
                .checked_add(metadata.len())
                .ok_or_else(|| PackageErrorV1::Limit("stdlib size overflow".into()))?;
            if total > MAX_ARTIFACT_BYTES {
                return Err(PackageErrorV1::Limit("stdlib tree exceeds 256 MiB".into()));
            }
            let name = path
                .strip_prefix(root)
                .expect("descendant")
                .to_str()
                .ok_or_else(|| PackageErrorV1::Manifest("stdlib path must be UTF-8".into()))?
                .to_owned();
            if name.contains(['\0', '\n']) {
                return Err(PackageErrorV1::Manifest("invalid stdlib filename".into()));
            }
            files.insert(name, hash_file_v1(&path)?);
        }
    }
    let mut hasher = Sha256::new();
    for (name, digest) in files {
        hasher.update(format!("{name}\0{digest}\n"));
    }
    Ok(format!("{:x}", hasher.finalize()))
}
