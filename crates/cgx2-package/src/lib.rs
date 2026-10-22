//! Strict local package discovery and checked public API indexing.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use cgx2_api_index::{build_index, ApiIndex, IndexError};
use cgx2_sema::check_module;
use cgx2_syntax::{parse_module, Span};

pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;
pub const MAX_SOURCES: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub file: PathBuf,
    /// Zero-based, half-open UTF-8 byte span; absent for filesystem errors.
    pub span: Option<Span>,
}

impl PackageDiagnostic {
    fn new(
        code: &'static str,
        message: impl Into<String>,
        file: &Path,
        span: Option<Span>,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            file: file.to_path_buf(),
            span,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleRecord {
    /// Path as listed in the manifest, relative to the package root.
    pub source: PathBuf,
    pub name: Option<String>,
    pub api: ApiIndex,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageIndex {
    pub schema_version: u32,
    pub name: String,
    /// Path as listed in `cgx2.toml`, relative to the package root.
    pub entry: PathBuf,
    pub imports_supported: bool,
    /// Sorted by declared module name, then source path.
    pub modules: Vec<ModuleRecord>,
}

impl PackageIndex {
    pub fn render_text(&self) -> String {
        let mut out = format!(
            "package {}\nschema_version {}\nentry {}\nimports_supported {}\n",
            self.name,
            self.schema_version,
            self.entry.display(),
            self.imports_supported
        );
        for module in &self.modules {
            out.push_str(&format!("source {}\n", module.source.display()));
            out.push_str(&module.api.render_text());
        }
        out
    }

    pub fn render_json(&self) -> String {
        let mut out = format!("{{\"schema_version\":{},\"name\":", self.schema_version);
        push_json_string(&mut out, &self.name);
        out.push_str(",\"entry\":");
        push_json_string(&mut out, &self.entry.to_string_lossy());
        out.push_str(",\"imports_supported\":");
        out.push_str(if self.imports_supported {
            "true"
        } else {
            "false"
        });
        out.push_str(",\"modules\":[");
        for (index, module) in self.modules.iter().enumerate() {
            if index != 0 {
                out.push(',');
            }
            out.push_str("{\"source\":");
            push_json_string(&mut out, &module.source.to_string_lossy());
            out.push_str(",\"api\":");
            out.push_str(&module.api.render_json());
            out.push('}');
        }
        out.push_str("]}");
        out
    }
}

fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch <= '\u{001f}' => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
}

#[derive(Clone)]
struct LocatedString {
    value: String,
    span: Span,
}

struct Manifest {
    name: String,
    entry: LocatedString,
    sources: Vec<LocatedString>,
}

/// Load only manifest-listed `.cg` files under a canonical package root.
/// No source is read until all listed paths have passed containment checks.
pub fn load_package(root: &Path) -> Result<PackageIndex, Vec<PackageDiagnostic>> {
    let canonical_root = fs::canonicalize(root).map_err(|error| {
        vec![PackageDiagnostic::new(
            "CGP005",
            format!("cannot access package root {}: {error}", root.display()),
            root,
            None,
        )]
    })?;
    let manifest_path = root.join("cgx2.toml");
    let canonical_manifest =
        canonicalize_contained(&manifest_path, &canonical_root).map_err(|d| vec![d])?;
    let manifest_bytes = read_limited(&canonical_manifest, &manifest_path, MAX_MANIFEST_BYTES)?;
    let manifest_text = std::str::from_utf8(&manifest_bytes).map_err(|error| {
        vec![PackageDiagnostic::new(
            "CGP004",
            format!("{} is not UTF-8: {error}", manifest_path.display()),
            &manifest_path,
            Some(Span {
                start: error.valid_up_to(),
                end: (error.valid_up_to() + error.error_len().unwrap_or(1))
                    .min(manifest_bytes.len()),
            }),
        )]
    })?;
    let manifest = parse_manifest(manifest_text, &manifest_path)?;

    let mut errors = Vec::new();
    if !manifest
        .sources
        .iter()
        .any(|source| source.value == manifest.entry.value)
    {
        errors.push(PackageDiagnostic::new(
            "CGP008",
            format!("entry {} is not listed in sources", manifest.entry.value),
            &manifest_path,
            Some(manifest.entry.span),
        ));
    }
    let mut seen_listed = BTreeSet::new();
    let mut seen_paths = BTreeSet::new();
    let mut validated = Vec::new();
    for source in &manifest.sources {
        if !seen_listed.insert(source.value.as_str()) {
            errors.push(PackageDiagnostic::new(
                "CGP007",
                format!("duplicate source path: {}", source.value),
                &manifest_path,
                Some(source.span),
            ));
            continue;
        }
        let relative = PathBuf::from(&source.value);
        if !valid_source_path(&relative, &source.value) {
            errors.push(PackageDiagnostic::new(
                "CGP011",
                format!(
                    "source path must be a relative .cg path without traversal: {}",
                    source.value
                ),
                &manifest_path,
                Some(source.span),
            ));
            continue;
        }
        let listed = root.join(&relative);
        let canonical = match canonicalize_contained(&listed, &canonical_root) {
            Ok(path) => path,
            Err(error) => {
                errors.push(error);
                continue;
            }
        };
        if !seen_paths.insert(canonical.clone()) {
            errors.push(PackageDiagnostic::new(
                "CGP007",
                format!(
                    "duplicate source path after canonicalization: {}",
                    source.value
                ),
                &manifest_path,
                Some(source.span),
            ));
            continue;
        }
        validated.push((relative, listed, canonical));
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let mut modules = Vec::new();
    let mut seen_modules: BTreeMap<String, PathBuf> = BTreeMap::new();
    for (relative, listed, canonical) in validated {
        let bytes = match read_limited(&canonical, &listed, MAX_SOURCE_BYTES) {
            Ok(bytes) => bytes,
            Err(mut diagnostics) => {
                errors.append(&mut diagnostics);
                continue;
            }
        };
        let source = match std::str::from_utf8(&bytes) {
            Ok(source) => source,
            Err(error) => {
                errors.push(PackageDiagnostic::new(
                    "CGP004",
                    format!("{} is not UTF-8: {error}", listed.display()),
                    &listed,
                    Some(Span {
                        start: error.valid_up_to(),
                        end: (error.valid_up_to() + error.error_len().unwrap_or(1))
                            .min(bytes.len()),
                    }),
                ));
                continue;
            }
        };
        let parsed = match parse_module(source) {
            Ok(module) => module,
            Err(diagnostics) => {
                errors.extend(
                    diagnostics
                        .into_iter()
                        .map(|d| PackageDiagnostic::new(d.code, d.message, &listed, Some(d.span))),
                );
                continue;
            }
        };
        let name = parsed.name.as_ref().map(|path| path.segments.join("."));
        if let Some(ref name) = name {
            if let Some(first) = seen_modules.insert(name.clone(), listed.clone()) {
                errors.push(PackageDiagnostic::new(
                    "CGP010",
                    format!(
                        "duplicate module {name}; first declared in {}",
                        first.display()
                    ),
                    &listed,
                    parsed.name.as_ref().map(|path| path.span),
                ));
                continue;
            }
        } else if manifest.sources.len() > 1 {
            errors.push(PackageDiagnostic::new(
                "CGP009",
                "multi-file package source lacks a module declaration",
                &listed,
                Some(Span { start: 0, end: 0 }),
            ));
            continue;
        }
        let typed = match check_module(&parsed) {
            Ok(typed) => typed,
            Err(diagnostics) => {
                errors.extend(
                    diagnostics
                        .into_iter()
                        .map(|d| PackageDiagnostic::new(d.code, d.message, &listed, Some(d.span))),
                );
                continue;
            }
        };
        let api = match build_index(&parsed, &typed) {
            Ok(api) => api,
            Err(IndexError::InvalidModule(diagnostics)) => {
                errors.extend(
                    diagnostics
                        .into_iter()
                        .map(|d| PackageDiagnostic::new(d.code, d.message, &listed, Some(d.span))),
                );
                continue;
            }
            Err(IndexError::MismatchedTypedModule) => {
                errors.push(PackageDiagnostic::new(
                    "CGP012",
                    "API index rejected a freshly checked module",
                    &listed,
                    None,
                ));
                continue;
            }
        };
        modules.push(ModuleRecord {
            source: relative,
            name,
            api,
        });
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    modules.sort_by(|a, b| a.name.cmp(&b.name).then(a.source.cmp(&b.source)));
    Ok(PackageIndex {
        schema_version: 1,
        name: manifest.name,
        entry: PathBuf::from(manifest.entry.value),
        imports_supported: false,
        modules,
    })
}

fn canonicalize_contained(path: &Path, root: &Path) -> Result<PathBuf, PackageDiagnostic> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        PackageDiagnostic::new(
            "CGP005",
            format!("cannot resolve {}: {error}", path.display()),
            path,
            None,
        )
    })?;
    if !canonical.starts_with(root) {
        return Err(PackageDiagnostic::new(
            "CGP011",
            format!("path escapes package root: {}", path.display()),
            path,
            None,
        ));
    }
    Ok(canonical)
}

fn valid_source_path(path: &Path, value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 240
        && !value.contains('\\')
        && !value.chars().any(char::is_control)
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && path.extension().is_some_and(|extension| extension == "cg")
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn read_limited(
    path: &Path,
    listed: &Path,
    limit: usize,
) -> Result<Vec<u8>, Vec<PackageDiagnostic>> {
    let file = fs::File::open(path).map_err(|error| {
        vec![PackageDiagnostic::new(
            "CGP005",
            format!("cannot open {}: {error}", listed.display()),
            listed,
            None,
        )]
    })?;
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            vec![PackageDiagnostic::new(
                "CGP005",
                format!("cannot read {}: {error}", listed.display()),
                listed,
                None,
            )]
        })?;
    if bytes.len() > limit {
        return Err(vec![PackageDiagnostic::new(
            "CGP013",
            format!("{} exceeds {limit} bytes", listed.display()),
            listed,
            None,
        )]);
    }
    Ok(bytes)
}

fn parse_manifest(source: &str, file: &Path) -> Result<Manifest, Vec<PackageDiagnostic>> {
    let mut errors = Vec::new();
    let mut fields: BTreeMap<&str, (&str, Span)> = BTreeMap::new();
    let mut offset = 0;
    for raw_line in source.split_inclusive('\n') {
        let line = raw_line.trim_end_matches(['\n', '\r']);
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            offset += raw_line.len();
            continue;
        }
        let span = Span {
            start: offset,
            end: offset + line.len(),
        };
        let Some((raw_key, raw_value)) = line.split_once('=') else {
            errors.push(PackageDiagnostic::new(
                "CGP003",
                "expected key = value",
                file,
                Some(span),
            ));
            offset += raw_line.len();
            continue;
        };
        let key = raw_key.trim();
        if !matches!(key, "schema_version" | "name" | "entry" | "sources") {
            errors.push(PackageDiagnostic::new(
                "CGP001",
                format!("unknown manifest key {key}"),
                file,
                Some(span),
            ));
        } else if fields.insert(key, (raw_value.trim(), span)).is_some() {
            errors.push(PackageDiagnostic::new(
                "CGP002",
                format!("duplicate manifest key {key}"),
                file,
                Some(span),
            ));
        }
        offset += raw_line.len();
    }
    for key in ["schema_version", "name", "entry", "sources"] {
        if !fields.contains_key(key) {
            errors.push(PackageDiagnostic::new(
                "CGP006",
                format!("missing required manifest key {key}"),
                file,
                Some(Span {
                    start: source.len(),
                    end: source.len(),
                }),
            ));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let (schema, schema_span) = fields["schema_version"];
    if schema != "1" {
        errors.push(PackageDiagnostic::new(
            "CGP003",
            "schema_version must be the integer 1",
            file,
            Some(schema_span),
        ));
    }
    let (name_raw, name_span) = fields["name"];
    let name = parse_string(name_raw).filter(|(value, rest)| rest.is_empty() && valid_name(value));
    if name.is_none() {
        errors.push(PackageDiagnostic::new(
            "CGP003",
            "name must be a quoted package identifier",
            file,
            Some(name_span),
        ));
    }
    let (entry_raw, entry_span) = fields["entry"];
    let entry = parse_string(entry_raw)
        .filter(|(_, rest)| rest.is_empty())
        .map(|(value, _)| LocatedString {
            value,
            span: entry_span,
        });
    if entry.is_none() {
        errors.push(PackageDiagnostic::new(
            "CGP003",
            "entry must be one quoted path",
            file,
            Some(entry_span),
        ));
    }
    let (sources_raw, sources_span) = fields["sources"];
    let sources = parse_array(sources_raw);
    if sources.is_none() {
        errors.push(PackageDiagnostic::new(
            "CGP003",
            "sources must be a one-line array of quoted paths",
            file,
            Some(sources_span),
        ));
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let sources = sources
        .unwrap()
        .into_iter()
        .map(|value| LocatedString {
            value,
            span: sources_span,
        })
        .collect();
    Ok(Manifest {
        name: name.unwrap().0,
        entry: entry.unwrap(),
        sources,
    })
}

fn valid_name(name: &str) -> bool {
    name.len() <= 64
        && name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// A deliberate subset of TOML basic strings: UTF-8, and \", \\, \n, \r, \t escapes.
fn parse_string(input: &str) -> Option<(String, &str)> {
    let mut chars = input.char_indices();
    if chars.next()?.1 != '"' {
        return None;
    }
    let mut value = String::new();
    while let Some((index, ch)) = chars.next() {
        match ch {
            '"' => return Some((value, input[index + 1..].trim_start())),
            '\\' => {
                let (_, escaped) = chars.next()?;
                value.push(match escaped {
                    '"' => '"',
                    '\\' => '\\',
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    _ => return None,
                });
            }
            ch if ch <= '\u{001f}' => return None,
            ch => value.push(ch),
        }
    }
    None
}

fn parse_array(input: &str) -> Option<Vec<String>> {
    let mut rest = input.strip_prefix('[')?.trim_start();
    let mut values = Vec::new();
    if let Some(tail) = rest.strip_prefix(']') {
        return tail.trim().is_empty().then_some(values);
    }
    loop {
        let (value, tail) = parse_string(rest)?;
        values.push(value);
        if values.len() > MAX_SOURCES {
            return None;
        }
        if let Some(after) = tail.strip_prefix(']') {
            return after.trim().is_empty().then_some(values);
        }
        rest = tail.strip_prefix(',')?.trim_start();
        if rest.starts_with(']') {
            return None;
        }
    }
}
