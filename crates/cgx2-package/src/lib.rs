//! Strict local package discovery and checked public API indexing.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use cgx2_api_index::{build_index_with_imports, ApiIndex, IndexError};
use cgx2_sema::{collect_public_signatures_with_imports, ImportedSignatures, TypedModule};
use cgx2_syntax::{parse_module, Expr, ExprKind, Module, Span, Stmt, StmtKind};
use sha2::{Digest, Sha256};

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
    pub related: Option<Box<RelatedLocation>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelatedLocation {
    pub file: PathBuf,
    pub span: Span,
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
            related: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleRecord {
    /// Path as listed in the manifest, relative to the package root.
    pub source: PathBuf,
    pub name: Option<String>,
    pub api: ApiIndex,
    pub imports: Vec<String>,
    pub resolved_public_calls: Vec<ResolvedCall>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedCall {
    pub name: String,
    pub span: Span,
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
    /// Dependency-first declared module names, with lexical tie breaking.
    pub topological_order: Vec<String>,
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
            for import in &module.imports {
                out.push_str(&format!("import {import}\n"));
            }
            for call in &module.resolved_public_calls {
                out.push_str(&format!(
                    "call {} [{}, {})\n",
                    call.name, call.span.start, call.span.end
                ));
            }
            out.push_str(&module.api.render_text());
        }
        out.push_str(&format!(
            "topological_order {}\n",
            self.topological_order.join(", ")
        ));
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
            out.push_str(",\"imports\":[");
            for (i, import) in module.imports.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_json_string(&mut out, import);
            }
            out.push_str("],\"resolved_public_calls\":[");
            for (i, call) in module.resolved_public_calls.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str("{\"name\":");
                push_json_string(&mut out, &call.name);
                out.push_str(&format!(
                    ",\"span\":{{\"start\":{},\"end\":{}}}}}",
                    call.span.start, call.span.end
                ));
            }
            out.push(']');
            out.push('}');
        }
        out.push_str("],\"topological_order\":[");
        for (i, name) in self.topological_order.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            push_json_string(&mut out, name);
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
    binding_lock: Option<PathBuf>,
}

struct ParsedSource {
    source: PathBuf,
    listed: PathBuf,
    canonical: PathBuf,
    text: String,
    sha256: String,
    module: Module,
    name: Option<String>,
}

/// Source bytes and typed syntax that were checked together by the package loader.
#[derive(Clone, Debug)]
pub struct CheckedModule {
    source: PathBuf,
    canonical_path: PathBuf,
    source_sha256: String,
    text: String,
    syntax: Module,
    typed: TypedModule,
}

impl CheckedModule {
    pub fn source(&self) -> &Path {
        &self.source
    }
    pub fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }
    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn syntax(&self) -> &Module {
        &self.syntax
    }
    pub fn typed(&self) -> &TypedModule {
        &self.typed
    }
}

/// An immutable local package view. Call `verify_unchanged` before publishing output.
#[derive(Clone, Debug)]
pub struct CheckedPackage {
    root: PathBuf,
    manifest_canonical_path: PathBuf,
    manifest_sha256: String,
    binding_lock: Option<PathBuf>,
    index: PackageIndex,
    modules: Vec<CheckedModule>,
}

impl CheckedPackage {
    /// Relative lock path explicitly named by a schema 2 manifest.
    pub fn binding_lock(&self) -> Option<&Path> {
        self.binding_lock.as_deref()
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }
    pub fn index(&self) -> &PackageIndex {
        &self.index
    }
    pub fn modules(&self) -> &[CheckedModule] {
        &self.modules
    }
    /// Reject a changed manifest, source bytes, or canonical target after validation.
    pub fn verify_unchanged(&self) -> Result<(), Vec<PackageDiagnostic>> {
        let manifest = self.root.join("cgx2.toml");
        let manifest_path = canonicalize_contained(&manifest, &self.root).map_err(|d| vec![d])?;
        if manifest_path != self.manifest_canonical_path {
            return Err(vec![PackageDiagnostic::new(
                "CGP018",
                "manifest target changed after package validation",
                &manifest,
                None,
            )]);
        }
        let bytes = read_limited(&manifest_path, &manifest, MAX_MANIFEST_BYTES)?;
        if digest(&bytes) != self.manifest_sha256 {
            return Err(vec![PackageDiagnostic::new(
                "CGP018",
                "manifest changed after package validation",
                &manifest,
                None,
            )]);
        }
        for module in &self.modules {
            let listed = self.root.join(&module.source);
            let canonical = canonicalize_contained(&listed, &self.root).map_err(|d| vec![d])?;
            if canonical != module.canonical_path {
                return Err(vec![PackageDiagnostic::new(
                    "CGP018",
                    "source target changed after package validation",
                    &listed,
                    None,
                )]);
            }
            let bytes = read_limited(&canonical, &listed, MAX_SOURCE_BYTES)?;
            if digest(&bytes) != module.source_sha256 {
                return Err(vec![PackageDiagnostic::new(
                    "CGP018",
                    "source changed after package validation",
                    &listed,
                    None,
                )]);
            }
        }
        Ok(())
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Match syntax canonicalization: declaration binders shadow local nominals,
/// including inside nested type arguments. Explicit qualified names still resolve.
fn generic_type_scope(module: &Module, parameters: &[cgx2_syntax::TypeParameter]) -> Module {
    let mut scope = module.clone();
    scope.records.retain(|record| {
        !parameters
            .iter()
            .any(|parameter| parameter.name == record.name)
    });
    scope.variants.retain(|variant| {
        !parameters
            .iter()
            .any(|parameter| parameter.name == variant.name)
    });
    scope
}

/// Load only manifest-listed `.cg` files under a canonical package root.
/// No source is read until all listed paths have passed containment checks.
pub fn load_package(root: &Path) -> Result<PackageIndex, Vec<PackageDiagnostic>> {
    load_checked_package(root).map(|checked| checked.index)
}

/// Load and statically check a package, retaining the exact checked source bytes.
pub fn load_checked_package(root: &Path) -> Result<CheckedPackage, Vec<PackageDiagnostic>> {
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
    let manifest_sha256 = digest(&manifest_bytes);
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

    let mut parsed_sources = Vec::new();
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
        if manifest.binding_lock.is_none() {
            for function in &parsed.functions {
                if function
                    .foreign
                    .as_ref()
                    .is_some_and(|binding| binding.route == "native")
                {
                    errors.push(PackageDiagnostic::new(
                        "CGP019",
                        "native foreign declaration requires schema 2 binding_lock",
                        &listed,
                        Some(function.span),
                    ));
                }
            }
        }
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
        parsed_sources.push(ParsedSource {
            source: relative,
            listed,
            canonical,
            text: source.to_owned(),
            sha256: digest(&bytes),
            module: parsed,
            name,
        });
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let named: BTreeMap<String, usize> = parsed_sources
        .iter()
        .enumerate()
        .filter_map(|(i, source)| source.name.as_ref().map(|name| (name.clone(), i)))
        .collect();
    let mut imported = ImportedSignatures::default();
    imported.modules.extend(named.keys().cloned());
    for source in &parsed_sources {
        if let Some(name) = &source.name {
            for variant in &source.module.variants {
                let identity = format!("{name}.{}", variant.name);
                if variant.public {
                    let scope = generic_type_scope(&source.module, &variant.type_parameters);
                    let mut variant = variant.clone();
                    variant.name = identity.clone();
                    for case in &mut variant.cases {
                        for payload in &mut case.payloads {
                            cgx2_syntax::qualify_type(payload, &scope);
                        }
                    }
                    imported.public_variants.insert(identity, variant);
                } else {
                    imported.private_variants.insert(identity);
                }
            }
            for record in &source.module.records {
                let identity = format!("{name}.{}", record.name);
                if record.public {
                    let scope = generic_type_scope(&source.module, &record.type_parameters);
                    let mut record = record.clone();
                    record.name = identity.clone();
                    for field in &mut record.fields {
                        cgx2_syntax::qualify_type(&mut field.ty, &scope);
                    }
                    imported.public_records.insert(identity, record);
                } else {
                    imported.private_records.insert(identity);
                }
            }
        }
    }
    for source in &parsed_sources {
        if let Some(name) = &source.name {
            match collect_public_signatures_with_imports(&source.module, &imported) {
                Ok(signatures) => {
                    for (function, signature) in signatures {
                        imported
                            .public_functions
                            .insert(format!("{name}.{function}"), signature);
                    }
                }
                Err(diagnostics) => errors.extend(diagnostics.into_iter().map(|d| {
                    PackageDiagnostic::new(d.code, d.message, &source.listed, Some(d.span))
                })),
            }
            for function in &source.module.functions {
                if !function.public {
                    imported
                        .private_functions
                        .insert(format!("{name}.{}", function.name));
                }
            }
        }
    }
    let mut dependencies: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for source in &parsed_sources {
        let Some(name) = &source.name else {
            continue;
        };
        let mut edges = Vec::new();
        for import in &source.module.imports {
            let target = import.segments.join(".");
            if target == *name {
                errors.push(PackageDiagnostic::new(
                    "CGP015",
                    format!("module `{name}` imports itself"),
                    &source.listed,
                    Some(import.span),
                ));
            } else if !named.contains_key(&target) {
                errors.push(PackageDiagnostic::new(
                    "CGP014",
                    format!("imported module `{target}` is not listed in this package"),
                    &source.listed,
                    Some(import.span),
                ));
            } else if contains_memory_script(&parsed_sources[named[&target]].module) {
                errors.push(PackageDiagnostic::new(
                    "CGP017",
                    format!("importing memory-script module `{target}` is unsupported"),
                    &source.listed,
                    Some(import.span),
                ));
            } else {
                edges.push(target);
            }
        }
        edges.sort();
        dependencies.insert(name.clone(), edges);
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let topological_order = match topo_order(&dependencies) {
        Ok(order) => order,
        Err(cycle) => {
            let from = &cycle[cycle.len() - 2];
            let to = cycle.last().expect("cycle has closing node");
            let source = &parsed_sources[named[from]];
            let span = source
                .module
                .imports
                .iter()
                .find(|p| p.segments.join(".") == *to)
                .map(|p| p.span);
            return Err(vec![PackageDiagnostic::new(
                "CGP016",
                format!("import cycle: {}", cycle.join(" -> ")),
                &source.listed,
                span,
            )]);
        }
    };

    let declarations: BTreeMap<String, RelatedLocation> = imported
        .public_functions
        .iter()
        .filter_map(|(name, signature)| {
            let module = name.rsplit_once('.')?.0;
            let source = &parsed_sources[*named.get(module)?];
            Some((
                name.clone(),
                RelatedLocation {
                    file: source.listed.clone(),
                    span: signature.span,
                },
            ))
        })
        .collect();
    let public_names: BTreeSet<String> = declarations.keys().cloned().collect();
    let mut modules = Vec::new();
    let mut checked_modules = Vec::new();
    // All declarations are known, but effect evidence is published only after a
    // dependency body passes checking. Keep unnamed single-source packages working.
    let source_order: BTreeMap<_, _> = parsed_sources
        .iter()
        .enumerate()
        .map(|(index, source)| (source.source.clone(), index))
        .collect();
    let ranks: BTreeMap<_, _> = topological_order
        .iter()
        .enumerate()
        .map(|(rank, name)| (name.as_str(), rank))
        .collect();
    parsed_sources.sort_by_key(|source| {
        source
            .name
            .as_deref()
            .and_then(|name| ranks.get(name).copied())
            .unwrap_or(0)
    });
    for source in parsed_sources {
        if !source.module.imports.is_empty()
            && contains_memory_script(&source.module)
            && !cgx2_sema::default_managed::explicit(&source.module)
        {
            errors.push(PackageDiagnostic::new(
                "CGM001",
                "imports in memory scripts are unsupported",
                &source.listed,
                Some(source.module.imports[0].span),
            ));
            continue;
        }
        let typed = match imported.check_and_record_module(&source.module) {
            Ok(typed) => typed,
            Err(diagnostics) => {
                errors.extend(diagnostics.into_iter().map(|d| {
                    let mut diagnostic =
                        PackageDiagnostic::new(d.code, d.message, &source.listed, Some(d.span));
                    if matches!(
                        d.code,
                        "CGT003"
                            | "CGT005"
                            | "CGT010"
                            | "CGT012"
                            | "CGT013"
                            | "CGT031"
                            | "CGT039"
                            | "CGT040"
                    ) {
                        let mut calls = Vec::new();
                        collect_calls(&source.module, &public_names, &mut calls);
                        if let Some(call) = calls
                            .iter()
                            .filter(|call| {
                                call.span.start <= d.span.start && call.span.end >= d.span.end
                            })
                            .min_by_key(|call| call.span.end - call.span.start)
                        {
                            diagnostic.related =
                                declarations.get(&call.name).cloned().map(Box::new);
                        }
                    }
                    diagnostic
                }));
                continue;
            }
        };
        let api = match build_index_with_imports(&source.module, &typed, &imported) {
            Ok(api) => api,
            Err(IndexError::InvalidModule(diagnostics)) => {
                errors.extend(diagnostics.into_iter().map(|d| {
                    PackageDiagnostic::new(d.code, d.message, &source.listed, Some(d.span))
                }));
                continue;
            }
            Err(IndexError::MismatchedTypedModule) => {
                errors.push(PackageDiagnostic::new(
                    "CGP012",
                    "API index rejected a freshly checked module",
                    &source.listed,
                    None,
                ));
                continue;
            }
        };
        let mut calls = Vec::new();
        collect_calls(&source.module, &public_names, &mut calls);
        modules.push(ModuleRecord {
            source: source.source.clone(),
            name: source.name,
            api,
            imports: source
                .module
                .imports
                .iter()
                .map(|p| p.segments.join("."))
                .collect(),
            resolved_public_calls: calls,
        });
        checked_modules.push(CheckedModule {
            source: source.source,
            canonical_path: source.canonical,
            source_sha256: source.sha256,
            text: source.text,
            syntax: source.module,
            typed,
        });
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    modules.sort_by(|a, b| a.name.cmp(&b.name).then(a.source.cmp(&b.source)));
    checked_modules.sort_by_key(|module| source_order[&module.source]);
    if checked_modules
        .iter()
        .any(|m| cgx2_sema::default_managed::profile(m.syntax()))
    {
        let entry = checked_modules
            .iter()
            .find(|m| m.source() == Path::new(&manifest.entry.value))
            .expect("validated entry");
        let name = entry
            .syntax()
            .name
            .as_ref()
            .map(|p| p.segments.join("."))
            .unwrap_or_default();
        let merged = cgx2_sema::merge_modules(
            &checked_modules
                .iter()
                .map(|m| m.syntax())
                .collect::<Vec<_>>(),
            &name,
        );
        cgx2_sema::check_module(&merged).map_err(|ds| {
            ds.into_iter()
                .map(|d| PackageDiagnostic::new(d.code, d.message, entry.source(), Some(d.span)))
                .collect::<Vec<_>>()
        })?;
        let text = checked_modules
            .iter()
            .map(|m| m.text())
            .collect::<Vec<_>>()
            .join("\n");
        // Existing type/API snapshots may contain foreign declarations and
        // target-specific invalid escapes. Neither is an executable managed
        // certificate: emitters retain their per-file target preflights.
        let foreign = checked_modules
            .iter()
            .any(|m| m.syntax().functions.iter().any(|f| f.foreign.is_some()));
        if !foreign {
            if let Err(ds) = cgx2_sema::default_managed::compile_module(&merged, &text) {
                let ds = ds
                    .into_iter()
                    .filter(|d| d.code != "CGRS003")
                    .map(|d| {
                        PackageDiagnostic::new(d.code, d.message, entry.source(), Some(d.span))
                    })
                    .collect::<Vec<_>>();
                if !ds.is_empty() {
                    return Err(ds);
                }
            }
        } else if checked_modules
            .iter()
            .any(|m| cgx2_sema::default_managed::resolved_explicit(m.syntax(), m.typed()))
        {
            return Err(vec![PackageDiagnostic::new("CGT090","managed observables with foreign execution require the later typed foreign integration",entry.source(),Some(entry.syntax().span))]);
        }
    }
    Ok(CheckedPackage {
        root: canonical_root,
        manifest_canonical_path: canonical_manifest,
        manifest_sha256,
        binding_lock: manifest.binding_lock,
        index: PackageIndex {
            schema_version: 2,
            name: manifest.name,
            entry: PathBuf::from(manifest.entry.value),
            imports_supported: true,
            modules,
            topological_order,
        },
        modules: checked_modules,
    })
}

fn topo_order(graph: &BTreeMap<String, Vec<String>>) -> Result<Vec<String>, Vec<String>> {
    fn visit(
        name: &str,
        graph: &BTreeMap<String, Vec<String>>,
        states: &mut BTreeMap<String, u8>,
        stack: &mut Vec<String>,
        order: &mut Vec<String>,
    ) -> Result<(), Vec<String>> {
        match states.get(name).copied().unwrap_or(0) {
            2 => return Ok(()),
            1 => {
                let start = stack.iter().position(|node| node == name).unwrap_or(0);
                let mut cycle = stack[start..].to_vec();
                cycle.push(name.to_owned());
                return Err(cycle);
            }
            _ => {}
        }
        states.insert(name.to_owned(), 1);
        stack.push(name.to_owned());
        for dependency in &graph[name] {
            visit(dependency, graph, states, stack, order)?;
        }
        stack.pop();
        states.insert(name.to_owned(), 2);
        order.push(name.to_owned());
        Ok(())
    }
    let mut states = BTreeMap::new();
    let mut stack = Vec::new();
    let mut order = Vec::new();
    for name in graph.keys() {
        visit(name, graph, &mut states, &mut stack, &mut order)?;
    }
    Ok(order)
}

fn contains_memory_script(module: &Module) -> bool {
    fn expr(node: &Expr) -> bool {
        match &node.kind {
            ExprKind::Constructor { arguments, .. } => arguments.iter().any(|arg| expr(&arg.value)),
            ExprKind::Collection(values) => values.iter().any(expr),
            ExprKind::Index { collection, index } => expr(collection) || expr(index),
            ExprKind::Name(path) => path.segments.first().is_some_and(|name| name == "memory"),
            ExprKind::Call { callee, arguments } => {
                expr(callee) || arguments.iter().any(|arg| expr(&arg.value))
            }
            ExprKind::Binary { left, right, .. } => expr(left) || expr(right),
            _ => false,
        }
    }
    fn stmt(node: &Stmt) -> bool {
        match &node.kind {
            StmtKind::Match { scrutinee, arms } => {
                expr(scrutinee) || arms.iter().any(|arm| arm.body.iter().any(stmt))
            }
            StmtKind::LoopContract { expression, .. } => expr(expression),
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => expr(condition) || then_body.iter().chain(else_body).any(stmt),
            StmtKind::NoGc { .. } => true,
            StmtKind::Let { value, .. }
            | StmtKind::Assign { value, .. }
            | StmtKind::Expr(value) => expr(value),
            StmtKind::Return(Some(value)) => expr(value),
            StmtKind::For { iter, body, .. } => expr(iter) || body.iter().any(stmt),
            StmtKind::Return(None) => false,
        }
    }
    module.statements.iter().any(stmt) || module.functions.iter().any(|f| f.body.iter().any(stmt))
}

fn collect_calls(module: &Module, public: &BTreeSet<String>, out: &mut Vec<ResolvedCall>) {
    fn expr(node: &Expr, public: &BTreeSet<String>, out: &mut Vec<ResolvedCall>) {
        match &node.kind {
            ExprKind::Constructor {
                ty,
                case,
                arguments,
            } => {
                let name = ty.path.segments.join(".");
                if case.is_empty() && public.contains(&name) {
                    out.push(ResolvedCall {
                        name,
                        span: node.span,
                    });
                }
                for arg in arguments {
                    expr(&arg.value, public, out);
                }
            }
            ExprKind::Collection(values) => {
                for value in values {
                    expr(value, public, out);
                }
            }
            ExprKind::Index { collection, index } => {
                expr(collection, public, out);
                expr(index, public, out);
            }
            ExprKind::Call { callee, arguments } => {
                if let ExprKind::Name(path) = &callee.kind {
                    let name = path.segments.join(".");
                    if public.contains(&name) {
                        out.push(ResolvedCall {
                            name,
                            span: node.span,
                        });
                    }
                }
                for arg in arguments {
                    expr(&arg.value, public, out);
                }
            }
            ExprKind::Binary { left, right, .. } => {
                expr(left, public, out);
                expr(right, public, out);
            }
            _ => {}
        }
    }
    fn stmt(node: &Stmt, public: &BTreeSet<String>, out: &mut Vec<ResolvedCall>) {
        match &node.kind {
            StmtKind::Match { scrutinee, arms } => {
                expr(scrutinee, public, out);
                for arm in arms {
                    for nested in &arm.body {
                        stmt(nested, public, out);
                    }
                }
            }
            StmtKind::LoopContract { expression, .. } => expr(expression, public, out),
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                expr(condition, public, out);
                for nested in then_body.iter().chain(else_body) {
                    stmt(nested, public, out);
                }
            }
            StmtKind::NoGc { budget, body } => {
                expr(budget, public, out);
                for nested in body {
                    stmt(nested, public, out);
                }
            }
            StmtKind::Let { value, .. }
            | StmtKind::Assign { value, .. }
            | StmtKind::Expr(value) => expr(value, public, out),
            StmtKind::Return(Some(value)) => expr(value, public, out),
            StmtKind::For { iter, body, .. } => {
                expr(iter, public, out);
                for nested in body {
                    stmt(nested, public, out);
                }
            }
            StmtKind::Return(None) => {}
        }
    }
    for function in &module.functions {
        for statement in &function.body {
            stmt(statement, public, out);
        }
    }
    for statement in &module.statements {
        stmt(statement, public, out);
    }
    out.sort_by_key(|call| (call.span.start, call.span.end));
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

fn valid_binding_path(path: &Path, value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 240
        && !value.contains('\\')
        && !value.chars().any(char::is_control)
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && path
            .extension()
            .is_some_and(|extension| extension == "json")
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
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
        if !matches!(
            key,
            "schema_version" | "name" | "entry" | "sources" | "binding_lock"
        ) {
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
    if !matches!(schema, "1" | "2") {
        errors.push(PackageDiagnostic::new(
            "CGP003",
            "schema_version must be the integer 1 or 2",
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
    let binding_lock = fields.get("binding_lock").and_then(|(raw, span)| {
        let Some((value, rest)) = parse_string(raw) else {
            errors.push(PackageDiagnostic::new(
                "CGP003",
                "binding_lock must be a quoted path",
                file,
                Some(*span),
            ));
            return None;
        };
        let path = PathBuf::from(&value);
        if !rest.is_empty() || schema != "2" || !valid_binding_path(&path, &value) {
            errors.push(PackageDiagnostic::new(
                "CGP011",
                "binding_lock requires schema_version 2 and a contained .json path",
                file,
                Some(*span),
            ));
            return None;
        }
        Some(path)
    });
    if schema == "2" && !fields.contains_key("binding_lock") {
        errors.push(PackageDiagnostic::new(
            "CGP006",
            "schema_version 2 requires binding_lock",
            file,
            Some(schema_span),
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
        binding_lock,
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
