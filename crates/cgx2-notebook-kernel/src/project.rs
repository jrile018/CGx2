//! Narrow, source-preserving lowering of checked local package scalar functions.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use cgx2_notebook_format::PackageProvenance;
use cgx2_package::{load_checked_package, CheckedPackage};
use cgx2_syntax::{Expr, ExprKind, Function, Module, Path as SyntaxPath, Span, StmtKind};

#[derive(Clone, Debug)]
pub(crate) struct Project {
    checked: CheckedPackage,
    modules: BTreeMap<String, Result<PureModule, String>>,
}

#[derive(Clone, Debug)]
pub(crate) struct PureModule {
    pub source: String,
    pub dependencies: BTreeSet<String>,
    pub hash: String,
    pub public: BTreeSet<String>,
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
                ),
            );
        }
        Ok(Self { checked, modules })
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
            modules,
        }))
    }

    pub fn public_call(&self, module: &str, function: &str, span: Span) -> Result<String, String> {
        let selected = self.module(module)?;
        if !selected.public.contains(function) {
            return Err(format!(
                "CGNK012: call {module}.{function} at {}..{} is not a public pure scalar function",
                span.start, span.end
            ));
        }
        Ok(mangle(module, function))
    }

    pub fn lower_cell(
        &self,
        source: &str,
        module: &Module,
        imported: &BTreeMap<String, String>,
    ) -> Result<String, String> {
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
            for statement in &function.body {
                lower_statement(self, statement, imported, &mut edits)?;
            }
        }
        for statement in &module.statements {
            lower_statement(self, statement, imported, &mut edits)?;
        }
        apply_edits(
            source,
            Span {
                start: 0,
                end: source.len(),
            },
            edits,
        )
    }
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
        StmtKind::Let { value, .. } | StmtKind::Expr(value) | StmtKind::Return(Some(value)) => {
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
) -> Result<PureModule, String> {
    if let Some(record) = module.records.iter().find(|record| record.public) {
        return Err(format!(
            "CGNK012: imported records are unsupported in notebooks: {name}.{} at {}..{}",
            record.name, record.span.start, record.span.end
        ));
    }
    let local: BTreeSet<_> = module.functions.iter().map(|f| f.name.clone()).collect();
    let imports: BTreeSet<_> = module
        .imports
        .iter()
        .map(|p| p.segments.join("."))
        .collect();
    let mut dependencies = BTreeSet::new();
    let mut source = String::new();
    for function in &module.functions {
        validate_signature(name, function)?;
        let mut edits = Vec::new();
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

#[allow(clippy::too_many_arguments)]
fn validate_block(
    module: &str,
    body: &[cgx2_syntax::Stmt],
    locals: &mut BTreeSet<String>,
    local: &BTreeSet<String>,
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
                    imports,
                    dependencies,
                    edits,
                )?;
                validate_block(
                    module,
                    then_body,
                    &mut locals.clone(),
                    local,
                    imports,
                    dependencies,
                    edits,
                )?;
                validate_block(
                    module,
                    else_body,
                    &mut locals.clone(),
                    local,
                    imports,
                    dependencies,
                    edits,
                )?;
            }
            StmtKind::Let {
                mutable: false,
                name,
                value,
                ..
            } => {
                validate_expression(module, value, locals, local, imports, dependencies, edits)?;
                locals.insert(name.clone());
            }
            StmtKind::Expr(value) | StmtKind::Return(Some(value)) => {
                validate_expression(module, value, locals, local, imports, dependencies, edits)?
            }
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
    if function
        .return_type
        .as_ref()
        .is_none_or(|t| !matches!(t.to_string().as_str(), "i64" | "Bool"))
        || function
            .parameters
            .iter()
            .any(|p| !matches!(p.ty.to_string().as_str(), "i64" | "Bool"))
    {
        return Err(format!(
            "CGNK012: {label} requires i64/Bool parameter and return types"
        ));
    }
    Ok(())
}

fn validate_expression(
    module: &str,
    expression: &Expr,
    locals: &BTreeSet<String>,
    functions: &BTreeSet<String>,
    imports: &BTreeSet<String>,
    dependencies: &mut BTreeSet<String>,
    edits: &mut Vec<(usize, usize, String)>,
) -> Result<(), String> {
    match &expression.kind {
        ExprKind::Integer(_) | ExprKind::Boolean(_) => Ok(()),
        ExprKind::Name(path) if path.segments.len() == 1 && locals.contains(&path.segments[0]) => {
            Ok(())
        }
        ExprKind::Binary { left, right, .. } => {
            validate_expression(
                module,
                left,
                locals,
                functions,
                imports,
                dependencies,
                edits,
            )?;
            validate_expression(
                module,
                right,
                locals,
                functions,
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
                if argument.name.is_some() {
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
