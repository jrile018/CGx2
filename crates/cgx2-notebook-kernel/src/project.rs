//! Narrow, source-preserving lowering of checked local package scalar functions.
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use cgx2_notebook_format::PackageProvenance;
use cgx2_package::{load_checked_package, CheckedPackage};
use cgx2_syntax::{
    Expr, ExprKind, Function, Module, Path as SyntaxPath, Record, Span, StmtKind, TypeSyntax,
};

#[derive(Clone, Debug)]
pub(crate) struct Project {
    checked: CheckedPackage,
    modules: BTreeMap<String, Result<PureModule, String>>,
    records: BTreeMap<String, Record>,
}

#[derive(Clone, Debug)]
pub(crate) struct PureModule {
    pub source: String,
    pub dependencies: BTreeSet<String>,
    pub hash: String,
    pub public: BTreeSet<String>,
}

/// Source plus inverse offsets for diagnostics after qualified-name expansion.
pub(crate) struct LoweredCell {
    pub source: String,
    edits: Vec<(usize, usize, String)>,
}

impl LoweredCell {
    fn original_offset(&self, offset: usize, end: bool) -> usize {
        let mut delta = 0isize;
        for (start, stop, replacement) in &self.edits {
            let lowered_start = start.saturating_add_signed(delta);
            let lowered_stop = lowered_start + replacement.len();
            if offset < lowered_start {
                break;
            }
            if offset < lowered_stop {
                return if end { *stop } else { *start };
            }
            delta += replacement.len() as isize - (stop - start) as isize;
        }
        offset.saturating_add_signed(-delta)
    }

    pub fn diagnostic(&self, mut message: String) -> String {
        let mut spans = Vec::new();
        for (index, _) in message.match_indices(" at ") {
            let start = index + 4;
            let digits = message[start..]
                .bytes()
                .take_while(u8::is_ascii_digit)
                .count();
            let Some(tail) = message[start + digits..].strip_prefix("..") else {
                continue;
            };
            let end_digits = tail.bytes().take_while(u8::is_ascii_digit).count();
            if let (Ok(first), Ok(last)) = (
                message[start..start + digits].parse::<usize>(),
                tail[..end_digits].parse::<usize>(),
            ) {
                spans.push((
                    start,
                    start + digits + 2 + end_digits,
                    format!(
                        "{}..{}",
                        self.original_offset(first, false),
                        self.original_offset(last, true)
                    ),
                ));
            }
        }
        for (start, end, replacement) in spans.into_iter().rev() {
            message.replace_range(start..end, &replacement);
        }
        message
    }
}

impl Project {
    pub fn load(root: &Path) -> Result<Self, String> {
        let checked = load_checked_package(root).map_err(|errors| {
            errors
                .iter()
                .map(|d| {
                    format!(
                        "{}: {}: {}{}",
                        d.code,
                        d.file.display(),
                        d.message,
                        d.span
                            .map_or_else(String::new, |s| format!(" at {}..{}", s.start, s.end))
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        let records: BTreeMap<_, _> = checked
            .modules()
            .iter()
            .flat_map(|module| {
                let name = module
                    .syntax()
                    .name
                    .as_ref()
                    .map(|p| p.segments.join("."))
                    .unwrap_or_default();
                module
                    .syntax()
                    .records
                    .iter()
                    .filter(|record| record.public)
                    .map(move |record| (format!("{name}.{}", record.name), record.clone()))
            })
            .collect();
        let mut modules = BTreeMap::new();
        for module in checked.modules() {
            let Some(name) = module.syntax().name.as_ref().map(|p| p.segments.join(".")) else {
                continue;
            };
            let public = checked
                .index()
                .modules
                .iter()
                .find(|record| record.name.as_deref() == Some(&name))
                .map(|record| {
                    record
                        .api
                        .functions
                        .iter()
                        .map(|f| f.name.clone())
                        .collect()
                })
                .unwrap_or_default();
            modules.insert(
                name.clone(),
                lower_module(
                    &name,
                    module.syntax(),
                    module.text(),
                    module.source_sha256(),
                    public,
                    &records,
                ),
            );
        }
        Ok(Self {
            checked,
            modules,
            records,
        })
    }

    pub fn manifest_hash(&self) -> &str {
        self.checked.manifest_sha256()
    }

    pub fn verify_unchanged(&self) -> Result<(), String> {
        self.checked.verify_unchanged().map_err(|errors| {
            errors
                .iter()
                .map(|d| format!("{}: {}: {}", d.code, d.file.display(), d.message))
                .collect::<Vec<_>>()
                .join("\n")
        })
    }

    /// Called only after the frozen package boundary rejects the current snapshot.
    /// Manifest/path failures affect all package users; source failures can be selective.
    pub fn changed_modules(&self) -> BTreeSet<String> {
        let errors = self.checked.verify_unchanged().err().unwrap_or_default();
        if errors
            .iter()
            .any(|error| error.file == self.checked.root().join("cgx2.toml"))
        {
            return self.modules.keys().cloned().collect();
        }
        let mut changed = BTreeSet::new();
        for module in self.checked.modules() {
            let path = self.checked.root().join(module.source());
            let matches = path.canonicalize().is_ok_and(|canonical| {
                if canonical != module.canonical_path() {
                    return false;
                }
                let mut source = String::new();
                std::fs::File::open(canonical)
                    .and_then(|file| {
                        file.take((cgx2_package::MAX_SOURCE_BYTES + 1) as u64)
                            .read_to_string(&mut source)
                    })
                    .is_ok()
                    && source.len() <= cgx2_package::MAX_SOURCE_BYTES
                    && cgx2_notebook_format::source_hash(&source) == module.source_sha256()
            });
            if !matches {
                if let Some(name) = &module.syntax().name {
                    changed.insert(name.segments.join("."));
                }
            }
        }
        // A racing restoration or unclassified boundary error must still discard cached imports.
        if changed.is_empty() {
            self.modules.keys().cloned().collect()
        } else {
            changed
        }
    }

    pub fn resolve_import(&self, path: &SyntaxPath) -> Result<(String, String), String> {
        let name = path.segments.join(".");
        if !name.starts_with(&format!("{}.", self.checked.index().name)) {
            return Err(format!(
                "CGNK012: import {name} at {}..{} is outside package {}",
                path.span.start,
                path.span.end,
                self.checked.index().name
            ));
        }
        self.module(&name).map_err(|reason| {
            format!(
                "CGNK012: import {name} at {}..{}: {reason}",
                path.span.start, path.span.end
            )
        })?;
        Ok((path.segments.last().cloned().unwrap_or_default(), name))
    }

    pub fn module(&self, name: &str) -> Result<&PureModule, String> {
        match self.modules.get(name) {
            Some(Ok(module)) => Ok(module),
            Some(Err(reason)) => Err(reason.clone()),
            None => Err(format!("module {name} is not listed in cgx2.toml")),
        }
    }

    pub fn functions_for(&self, imported: &BTreeMap<String, String>) -> Result<String, String> {
        let mut source = String::new();
        for name in imported.values() {
            let module = self.module(name)?;
            for dependency in &module.dependencies {
                if !imported.values().any(|candidate| candidate == dependency) {
                    return Err(format!("CGNK012: module {name} calls {dependency}; import {dependency} directly before use"));
                }
            }
            source.push_str(&module.source);
        }
        Ok(source)
    }

    pub fn provenance(
        &self,
        names: &BTreeSet<String>,
    ) -> Result<Option<PackageProvenance>, String> {
        if names.is_empty() {
            return Ok(None);
        }
        let mut modules = BTreeMap::new();
        for name in names {
            let module = self.module(name)?;
            modules.insert(name.clone(), module.hash.clone());
            for dependency in &module.dependencies {
                modules.insert(dependency.clone(), self.module(dependency)?.hash.clone());
            }
        }
        Ok(Some(PackageProvenance {
            manifest_sha256: self.manifest_hash().to_owned(),
            root_sha256: Some(cgx2_notebook_format::source_hash(&format!(
                "CGX2-PACKAGE-ROOT-v1:{}",
                self.checked
                    .root()
                    .as_os_str()
                    .as_encoded_bytes()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            ))),
            modules,
        }))
    }

    pub fn public_call(&self, module: &str, function: &str, span: Span) -> Result<String, String> {
        let selected = self.module(module)?;
        if !selected.public.contains(function)
            && !self.records.contains_key(&format!("{module}.{function}"))
        {
            return Err(format!(
                "CGNK012: call {module}.{function} at {}..{} is not a public pure scalar function",
                span.start, span.end
            ));
        }
        Ok(mangle(module, function))
    }

    pub fn canonical_identity(&self, name: &str) -> String {
        canonical_worker_identity(name)
            .filter(|identity| {
                self.records.contains_key(identity) && lowered_identity(identity) == name
            })
            .unwrap_or_else(|| name.to_owned())
    }

    pub fn record(&self, identity: &str) -> Option<&Record> {
        self.records.get(identity)
    }

    pub fn lowered_record(&self, identity: &str) -> Option<String> {
        self.records
            .contains_key(identity)
            .then(|| lowered_identity(identity))
    }

    fn cell_type(
        &self,
        ty: &TypeSyntax,
        imported: &BTreeMap<String, String>,
        edits: &mut Vec<(usize, usize, String)>,
    ) -> Result<(), String> {
        if ty.path.segments.len() > 1 {
            let parts = &ty.path.segments;
            let prefix = parts[..parts.len() - 1].join(".");
            let module = imported
                .get(&prefix)
                .cloned()
                .or_else(|| imported.values().find(|value| **value == prefix).cloned())
                .ok_or_else(|| {
                    format!(
                        "CGNK012: type {ty} requires preceding import at {}..{}",
                        ty.span.start, ty.span.end
                    )
                })?;
            let identity = format!("{module}.{}", parts.last().expect("qualified type"));
            if !ty.arguments.is_empty() || !self.records.contains_key(&identity) {
                return Err(format!(
                    "CGNK012: type {identity} is not a public scalar record at {}..{}",
                    ty.span.start, ty.span.end
                ));
            }
            edits.push((ty.span.start, ty.span.end, lowered_identity(&identity)));
        }
        Ok(())
    }

    pub fn lower_cell(
        &self,
        source: &str,
        module: &Module,
        imported: &BTreeMap<String, String>,
    ) -> Result<LoweredCell, String> {
        let mut edits = Vec::new();
        for path in &module.imports {
            // Keep newlines and byte offsets stable for diagnostics in the source cell.
            edits.push((
                path.span.start.saturating_sub("import ".len()),
                path.span.end,
                " ".repeat(path.span.end - path.span.start + "import ".len()),
            ));
        }
        for function in &module.functions {
            for parameter in &function.parameters {
                check_local_root(&parameter.name, imported, parameter.ty.span)?;
            }
            for ty in function
                .parameters
                .iter()
                .map(|p| &p.ty)
                .chain(function.return_type.iter())
            {
                self.cell_type(ty, imported, &mut edits)?;
            }
            for statement in &function.body {
                lower_statement(self, statement, imported, &mut edits)?;
            }
        }
        for statement in &module.statements {
            lower_statement(self, statement, imported, &mut edits)?;
        }
        let lowered = apply_edits(
            source,
            Span {
                start: 0,
                end: source.len(),
            },
            edits.clone(),
        )?;
        edits.sort_by_key(|edit| edit.0);
        Ok(LoweredCell {
            source: lowered,
            edits,
        })
    }
}

fn check_local_root(
    name: &str,
    imported: &BTreeMap<String, String>,
    span: Span,
) -> Result<(), String> {
    if imported.contains_key(name)
        || imported
            .values()
            .any(|module| module.split('.').next() == Some(name))
    {
        return Err(format!(
            "CGNK012: ambiguous local/module root {name} at {}..{}",
            span.start, span.end
        ));
    }
    Ok(())
}

fn lower_statement(
    project: &Project,
    statement: &cgx2_syntax::Stmt,
    imported: &BTreeMap<String, String>,
    edits: &mut Vec<(usize, usize, String)>,
) -> Result<(), String> {
    match &statement.kind {
        StmtKind::If {
            condition,
            then_body,
            else_body,
        } => {
            lower_expression(project, condition, imported, edits)?;
            for stmt in then_body.iter().chain(else_body) {
                lower_statement(project, stmt, imported, edits)?;
            }
            Ok(())
        }
        StmtKind::Let {
            name,
            annotation,
            value,
            ..
        } => {
            check_local_root(name, imported, statement.span)?;
            if let Some(ty) = annotation {
                project.cell_type(ty, imported, edits)?;
            }
            lower_expression(project, value, imported, edits)
        }
        StmtKind::Expr(value) | StmtKind::Return(Some(value)) => {
            lower_expression(project, value, imported, edits)
        }
        _ => Ok(()),
    }
}

fn lower_expression(
    project: &Project,
    expression: &Expr,
    imported: &BTreeMap<String, String>,
    edits: &mut Vec<(usize, usize, String)>,
) -> Result<(), String> {
    match &expression.kind {
        ExprKind::Binary { left, right, .. } => {
            lower_expression(project, left, imported, edits)?;
            lower_expression(project, right, imported, edits)
        }
        ExprKind::Call { callee, arguments } => {
            if let ExprKind::Name(path) = &callee.kind {
                if let [short, function] = path.segments.as_slice() {
                    let full = imported.get(short).ok_or_else(|| {
                        format!(
                            "CGNK012: call {short}.{function} at {}..{} requires preceding import",
                            path.span.start, path.span.end
                        )
                    })?;
                    let replacement = project.public_call(full, function, path.span)?;
                    edits.push((path.span.start, path.span.end, replacement));
                } else if path.segments.len() > 2
                    && project.records.contains_key(&path.segments.join("."))
                {
                    let full = path.segments[..path.segments.len() - 1].join(".");
                    if !imported.values().any(|module| module == &full) {
                        return Err(format!(
                            "CGNK012: constructor requires preceding import at {}..{}",
                            path.span.start, path.span.end
                        ));
                    }
                    edits.push((
                        path.span.start,
                        path.span.end,
                        lowered_identity(&path.segments.join(".")),
                    ));
                } else if path.segments.len() > 2 {
                    return Err(format!(
                        "CGNK012: call {} at {}..{} must use module.function spelling",
                        path.segments.join("."),
                        path.span.start,
                        path.span.end
                    ));
                }
            }
            for argument in arguments {
                lower_expression(project, &argument.value, imported, edits)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn lowered_identity(identity: &str) -> String {
    let (module, name) = identity
        .rsplit_once('.')
        .expect("qualified record identity");
    mangle(module, name)
}

pub(crate) fn canonical_worker_identity(name: &str) -> Option<String> {
    let mut remaining = name.strip_prefix("__cgx2_pkg")?;
    let mut parts = Vec::new();
    while !remaining.is_empty() {
        remaining = remaining.strip_prefix('_')?;
        let digits = remaining.bytes().take_while(u8::is_ascii_digit).count();
        let size: usize = remaining.get(..digits)?.parse().ok()?;
        remaining = remaining.get(digits..)?;
        let part = remaining.get(..size)?;
        if !super::valid_record_identifier(part) {
            return None;
        }
        parts.push(part);
        remaining = remaining.get(size..)?;
    }
    (parts.len() >= 2).then(|| parts.join("."))
}

fn mangle(module: &str, function: &str) -> String {
    let mut name = String::from("__cgx2_pkg");
    for segment in module.split('.') {
        name.push_str(&format!("_{}{}", segment.len(), segment));
    }
    name.push_str(&format!("_{}{}", function.len(), function));
    name
}

fn lower_module(
    name: &str,
    module: &Module,
    text: &str,
    hash: &str,
    public: BTreeSet<String>,
    records: &BTreeMap<String, Record>,
) -> Result<PureModule, String> {
    if text.contains("__cgx2_pkg") {
        return Err(format!(
            "CGNK012: module {name} uses reserved package symbol prefix"
        ));
    }
    let local: BTreeSet<_> = module
        .functions
        .iter()
        .map(|f| f.name.clone())
        .chain(
            module
                .records
                .iter()
                .filter(|r| r.public)
                .map(|r| r.name.clone()),
        )
        .collect();
    let imports: BTreeSet<_> = module
        .imports
        .iter()
        .map(|p| p.segments.join("."))
        .collect();
    let mut dependencies = BTreeSet::new();
    let mut source = String::new();
    for record in module.records.iter().filter(|r| r.public) {
        if !(1..=8).contains(&record.fields.len())
            || record
                .fields
                .iter()
                .any(|f| !matches!(f.ty.to_string().as_str(), "i64" | "Bool"))
        {
            return Err(format!(
                "CGNK012: record {name}.{} requires one to eight i64/Bool fields at {}..{}",
                record.name, record.span.start, record.span.end
            ));
        }
        let snippet = &text[record.span.start..record.span.end];
        let start =
            record.span.start + snippet.find("record ").ok_or("CGNK012: record header")? + 7;
        source.push_str(&apply_edits(
            text,
            record.span,
            vec![(start, start + record.name.len(), mangle(name, &record.name))],
        )?);
        source.push('\n');
    }
    for function in &module.functions {
        validate_signature(name, function)?;
        let mut edits = Vec::new();
        for ty in function
            .parameters
            .iter()
            .map(|p| &p.ty)
            .chain(function.return_type.iter())
        {
            module_type(ty, name, &imports, records, &mut dependencies, &mut edits)?;
        }
        let snippet = &text[function.span.start..function.span.end];
        let header = snippet
            .find(&format!("fn {}", function.name))
            .ok_or_else(|| format!("CGNK012: cannot lower {name}.{} header", function.name))?;
        let start = function.span.start + header + 3;
        edits.push((
            start,
            start + function.name.len(),
            mangle(name, &function.name),
        ));
        let mut locals: BTreeSet<_> = function.parameters.iter().map(|p| p.name.clone()).collect();
        validate_block(
            name,
            &function.body,
            &mut locals,
            &local,
            records,
            &imports,
            &mut dependencies,
            &mut edits,
        )?;
        source.push_str(&apply_edits(text, function.span, edits)?);
        source.push('\n');
    }
    Ok(PureModule {
        source,
        dependencies,
        hash: hash.to_owned(),
        public,
    })
}

fn module_type(
    ty: &TypeSyntax,
    name: &str,
    imports: &BTreeSet<String>,
    records: &BTreeMap<String, Record>,
    dependencies: &mut BTreeSet<String>,
    edits: &mut Vec<(usize, usize, String)>,
) -> Result<(), String> {
    let type_name = ty.to_string();
    if matches!(type_name.as_str(), "i64" | "Bool") {
        return Ok(());
    }
    let identity = if ty.path.segments.len() == 1 {
        format!("{name}.{type_name}")
    } else {
        type_name
    };
    let owner = identity
        .rsplit_once('.')
        .map(|(module, _)| module)
        .unwrap_or("");
    if !ty.arguments.is_empty()
        || !records.contains_key(&identity)
        || (owner != name && !imports.contains(owner))
    {
        return Err(format!(
            "CGNK012: type {ty} is not a public scalar record at {}..{}",
            ty.span.start, ty.span.end
        ));
    }
    if owner != name {
        dependencies.insert(owner.to_owned());
    }
    edits.push((ty.span.start, ty.span.end, lowered_identity(&identity)));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_block(
    module: &str,
    body: &[cgx2_syntax::Stmt],
    locals: &mut BTreeSet<String>,
    local: &BTreeSet<String>,
    records: &BTreeMap<String, Record>,
    imports: &BTreeSet<String>,
    dependencies: &mut BTreeSet<String>,
    edits: &mut Vec<(usize, usize, String)>,
) -> Result<(), String> {
    for statement in body {
        match &statement.kind {
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                validate_expression(
                    module,
                    condition,
                    locals,
                    local,
                    records,
                    imports,
                    dependencies,
                    edits,
                )?;
                validate_block(
                    module,
                    then_body,
                    &mut locals.clone(),
                    local,
                    records,
                    imports,
                    dependencies,
                    edits,
                )?;
                validate_block(
                    module,
                    else_body,
                    &mut locals.clone(),
                    local,
                    records,
                    imports,
                    dependencies,
                    edits,
                )?;
            }
            StmtKind::Let {
                mutable: false,
                name,
                annotation,
                value,
            } => {
                if let Some(ty) = annotation {
                    module_type(ty, module, imports, records, dependencies, edits)?;
                }
                validate_expression(
                    module,
                    value,
                    locals,
                    local,
                    records,
                    imports,
                    dependencies,
                    edits,
                )?;
                locals.insert(name.clone());
            }
            StmtKind::Expr(value) | StmtKind::Return(Some(value)) => validate_expression(
                module,
                value,
                locals,
                local,
                records,
                imports,
                dependencies,
                edits,
            )?,
            StmtKind::Return(None) => {}
            _ => {
                return Err(format!(
                    "CGNK012: {module} {}..{} uses unsupported statement",
                    statement.span.start, statement.span.end
                ))
            }
        }
    }
    Ok(())
}

fn validate_signature(module: &str, function: &Function) -> Result<(), String> {
    let label = format!(
        "{module}.{} at {}..{}",
        function.name, function.span.start, function.span.end
    );
    if function.foreign.is_some()
        || function
            .effects
            .as_ref()
            .is_none_or(|d| !d.names.is_empty())
        || function
            .capabilities
            .as_ref()
            .is_some_and(|d| !d.names.is_empty())
        || !function.requires.is_empty()
        || !function.ensures.is_empty()
    {
        return Err(format!("CGNK012: {label} must declare effects() and have no capabilities, contracts, or foreign binding"));
    }
    if function.parameters.len() > super::MAX_FUNCTION_PARAMETERS {
        return Err(format!("CGNK012: {label} exceeds scalar parameter limit"));
    }
    if function.return_type.is_none() {
        return Err(format!(
            "CGNK012: {label} requires an explicit scalar/record return type"
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_expression(
    module: &str,
    expression: &Expr,
    locals: &BTreeSet<String>,
    functions: &BTreeSet<String>,
    records: &BTreeMap<String, Record>,
    imports: &BTreeSet<String>,
    dependencies: &mut BTreeSet<String>,
    edits: &mut Vec<(usize, usize, String)>,
) -> Result<(), String> {
    match &expression.kind {
        ExprKind::Integer(_) | ExprKind::Boolean(_) => Ok(()),
        ExprKind::Name(path) if path.segments.len() <= 2 && locals.contains(&path.segments[0]) => {
            Ok(())
        }
        ExprKind::Binary { left, right, .. } => {
            validate_expression(
                module,
                left,
                locals,
                functions,
                records,
                imports,
                dependencies,
                edits,
            )?;
            validate_expression(
                module,
                right,
                locals,
                functions,
                records,
                imports,
                dependencies,
                edits,
            )
        }
        ExprKind::Call { callee, arguments } => {
            let ExprKind::Name(path) = &callee.kind else {
                return Err(format!(
                    "CGNK012: computed call at {}..{} is unsupported",
                    callee.span.start, callee.span.end
                ));
            };
            if path.segments.len() == 1 && path.segments[0].starts_with("$cgx2_cmp_") {
                for argument in arguments {
                    validate_expression(
                        module,
                        &argument.value,
                        locals,
                        functions,
                        records,
                        imports,
                        dependencies,
                        edits,
                    )?;
                }
                return Ok(());
            }
            let (target, function) = match path.segments.as_slice() {
                [function] if functions.contains(function) => {
                    (module.to_owned(), function.as_str())
                }
                [parts @ .., function] if !parts.is_empty() => {
                    let target = parts.join(".");
                    if !imports.contains(&target) {
                        return Err(format!(
                            "CGNK012: call {} at {}..{} is outside declared imports",
                            path.segments.join("."),
                            path.span.start,
                            path.span.end
                        ));
                    }
                    dependencies.insert(target.clone());
                    (target, function.as_str())
                }
                _ => {
                    return Err(format!(
                        "CGNK012: call {} at {}..{} is outside pure local module",
                        path.segments.join("."),
                        path.span.start,
                        path.span.end
                    ))
                }
            };
            edits.push((path.span.start, path.span.end, mangle(&target, function)));
            for argument in arguments {
                if argument.name.is_some() && !records.contains_key(&format!("{target}.{function}"))
                {
                    return Err(format!(
                        "CGNK012: named call at {}..{} is unsupported",
                        argument.span.start, argument.span.end
                    ));
                }
                validate_expression(
                    module,
                    &argument.value,
                    locals,
                    functions,
                    records,
                    imports,
                    dependencies,
                    edits,
                )?;
            }
            Ok(())
        }
        _ => Err(format!(
            "CGNK012: non-scalar expression at {}..{} is unsupported",
            expression.span.start, expression.span.end
        )),
    }
}

pub(crate) fn apply_edits(
    text: &str,
    span: Span,
    mut edits: Vec<(usize, usize, String)>,
) -> Result<String, String> {
    let mut output = text[span.start..span.end].to_owned();
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.0));
    let mut previous = span.end;
    for (start, end, replacement) in edits {
        if start < span.start || end > previous || start > end {
            return Err("CGNK012: overlapping lowered source spans".into());
        }
        output.replace_range(start - span.start..end - span.start, &replacement);
        previous = start;
    }
    Ok(output)
}
