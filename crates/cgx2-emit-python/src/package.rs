//! Source-only lowering of a checked, immutable local scalar package.
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use cgx2_package::{CheckedModule, CheckedPackage, PackageDiagnostic};
use cgx2_sema::Type;
use cgx2_syntax::{Expr, ExprKind, Module, Span, Stmt, StmtKind};

use crate::emit::{emit_package_module, package_support};
use crate::names::python_name;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageEmitDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub file: PathBuf,
    pub span: Option<Span>,
}

impl PackageEmitDiagnostic {
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

fn package_errors(errors: Vec<PackageDiagnostic>) -> Vec<PackageEmitDiagnostic> {
    errors
        .into_iter()
        .map(|d| PackageEmitDiagnostic::new(d.code, d.message, &d.file, d.span))
        .collect()
}

fn python_module_name(name: &str) -> String {
    let mut result = String::from("cgx2_mod_");
    for byte in name.bytes() {
        result.push_str(&format!("{byte:02x}"));
    }
    result
}

fn imported_alias(name: &str) -> String {
    let mut result = String::from("cgx2import_");
    for byte in name.bytes() {
        result.push_str(&format!("{byte:02x}"));
    }
    result
}

fn private_name(name: &str) -> String {
    let mut result = String::from("__cgx2_private_");
    for byte in name.bytes() {
        result.push_str(&format!("{byte:02x}"));
    }
    result
}

fn scalar(ty: &Type) -> bool {
    matches!(ty, Type::I64 | Type::Bool | Type::String | Type::Unit)
}

fn declared_names(block: &[Stmt], names: &mut BTreeSet<String>) {
    for stmt in block {
        match &stmt.kind {
            StmtKind::Let { name, .. } => {
                names.insert(name.clone());
            }
            StmtKind::For { binding, body, .. } => {
                names.insert(binding.clone());
                declared_names(body, names);
            }
            StmtKind::NoGc { body, .. } => declared_names(body, names),
            _ => {}
        }
    }
}

fn lower_expr(
    expr: &mut Expr,
    imports: &BTreeSet<String>,
    private: &BTreeMap<String, String>,
    aliases: &mut BTreeMap<String, String>,
    file: &Path,
    errors: &mut Vec<PackageEmitDiagnostic>,
) {
    match &mut expr.kind {
        ExprKind::Name(path) if path.segments.len() > 1 => {
            let Some((function, module)) = path.segments.split_last() else {
                return;
            };
            let module = module.join(".");
            if !imports.contains(&module) {
                errors.push(PackageEmitDiagnostic::new(
                    "CGE005",
                    format!("qualified name `{module}.{function}` is not explicitly imported"),
                    file,
                    Some(path.span),
                ));
                return;
            }
            let full_name = format!("{module}.{function}");
            let alias = imported_alias(&full_name);
            aliases.insert(full_name, alias.clone());
            path.segments = vec![alias];
        }
        ExprKind::Binary { left, right, .. } => {
            lower_expr(left, imports, private, aliases, file, errors);
            lower_expr(right, imports, private, aliases, file, errors);
        }
        ExprKind::Call { callee, arguments } => {
            if let ExprKind::Name(path) = &mut callee.kind {
                if path.segments.len() == 1 {
                    if let Some(replacement) = private.get(&path.segments[0]) {
                        path.segments[0] = replacement.clone();
                    }
                }
            }
            lower_expr(callee, imports, private, aliases, file, errors);
            for arg in arguments {
                lower_expr(&mut arg.value, imports, private, aliases, file, errors);
            }
        }
        _ => {}
    }
}

fn lower_block(
    block: &mut [Stmt],
    imports: &BTreeSet<String>,
    private: &BTreeMap<String, String>,
    aliases: &mut BTreeMap<String, String>,
    file: &Path,
    errors: &mut Vec<PackageEmitDiagnostic>,
) {
    for stmt in block {
        match &mut stmt.kind {
            StmtKind::Let { value, .. }
            | StmtKind::Assign { value, .. }
            | StmtKind::Expr(value) => lower_expr(value, imports, private, aliases, file, errors),
            StmtKind::Return(Some(value)) => {
                lower_expr(value, imports, private, aliases, file, errors)
            }
            StmtKind::For { iter, body, .. } => {
                lower_expr(iter, imports, private, aliases, file, errors);
                lower_block(body, imports, private, aliases, file, errors);
            }
            StmtKind::NoGc { budget, body } => {
                lower_expr(budget, imports, private, aliases, file, errors);
                lower_block(body, imports, private, aliases, file, errors);
            }
            StmtKind::Return(None) => {}
        }
    }
}

fn lower_module(
    source: &CheckedModule,
    names: &BTreeMap<String, &CheckedModule>,
) -> Result<(Module, String), Vec<PackageEmitDiagnostic>> {
    let file = source.source();
    let mut module = source.syntax().clone();
    let imports: BTreeSet<_> = module
        .imports
        .iter()
        .map(|p| p.segments.join("."))
        .collect();
    let private: BTreeMap<_, _> = module
        .functions
        .iter()
        .filter(|f| !f.public)
        .map(|f| (f.name.clone(), private_name(&f.name)))
        .collect();
    let mut aliases = BTreeMap::new();
    let mut errors = Vec::new();
    let mut names_in_source = BTreeSet::new();
    for function in &module.functions {
        names_in_source.insert(function.name.clone());
        names_in_source.extend(function.parameters.iter().map(|p| p.name.clone()));
        declared_names(&function.body, &mut names_in_source);
    }
    declared_names(&module.statements, &mut names_in_source);
    for function in &mut module.functions {
        for condition in function.requires.iter_mut().chain(&mut function.ensures) {
            lower_expr(
                condition,
                &imports,
                &private,
                &mut aliases,
                file,
                &mut errors,
            );
        }
        lower_block(
            &mut function.body,
            &imports,
            &private,
            &mut aliases,
            file,
            &mut errors,
        );
        if let Some(replacement) = private.get(&function.name) {
            function.name = replacement.clone();
        }
    }
    lower_block(
        &mut module.statements,
        &imports,
        &private,
        &mut aliases,
        file,
        &mut errors,
    );
    for generated in private.values().chain(aliases.values()) {
        if names_in_source.contains(generated) {
            errors.push(PackageEmitDiagnostic::new(
                "CGE005",
                format!("generated Python name `{generated}` collides with a source identifier"),
                file,
                Some(source.syntax().span),
            ));
        }
    }
    let mut header = String::new();
    for import in &module.imports {
        let name = import.segments.join(".");
        if let Some(target) = names.get(&name) {
            header.push_str(&format!("import {}\n", python_module_name(&name)));
            for (full_name, alias) in &aliases {
                if let Some(function) = full_name
                    .strip_prefix(&format!("{name}."))
                    .filter(|f| !f.contains('.'))
                {
                    if target
                        .syntax()
                        .functions
                        .iter()
                        .any(|f| f.public && f.name == function)
                    {
                        header.push_str(&format!(
                            "from {} import {} as {}\n",
                            python_module_name(&name),
                            python_name(function),
                            alias
                        ));
                    } else {
                        errors.push(PackageEmitDiagnostic::new(
                            "CGE005",
                            format!("`{full_name}` is not a public function"),
                            file,
                            Some(import.span),
                        ));
                    }
                }
            }
        }
    }
    module.imports.clear();
    if errors.is_empty() {
        Ok((module, header))
    } else {
        Err(errors)
    }
}

/// Build deterministic, package-relative Python files without executing them.
/// The returned paths are flat file names, safe to place in a new directory.
pub fn emit_package_python(
    package: &CheckedPackage,
) -> Result<BTreeMap<PathBuf, String>, Vec<PackageEmitDiagnostic>> {
    package.verify_unchanged().map_err(package_errors)?;
    let names: BTreeMap<_, _> = package
        .modules()
        .iter()
        .filter_map(|m| m.syntax().name.as_ref().map(|p| (p.segments.join("."), m)))
        .collect();
    let mut files = BTreeMap::new();
    files.insert(
        PathBuf::from("_cgx2_support.py"),
        package_support().to_owned(),
    );
    let mut errors = Vec::new();
    for name in &package.index().topological_order {
        let Some(source) = names.get(name) else {
            continue;
        };
        let filename = format!("{}.py", python_module_name(name));
        if filename.len() > 240 {
            errors.push(PackageEmitDiagnostic::new(
                "CGE005",
                "module name cannot fit a portable Python file name",
                source.source(),
                source.syntax().name.as_ref().map(|p| p.span),
            ));
            continue;
        }
        if let Some(stmt) = source.syntax().statements.first() {
            errors.push(PackageEmitDiagnostic::new(
                "CGE005",
                "package modules cannot contain script statements",
                source.source(),
                Some(stmt.span),
            ));
        }
        for (function, signature) in &source.typed().functions {
            if !signature.parameters.iter().all(scalar) || !scalar(&signature.return_type) {
                errors.push(PackageEmitDiagnostic::new(
                    "CGE005",
                    format!("function `{function}` has a nonscalar signature"),
                    source.source(),
                    Some(signature.span),
                ));
            }
        }
        for (span, ty) in source.typed().expressions.iter() {
            if !scalar(ty) {
                errors.push(PackageEmitDiagnostic::new(
                    "CGE005",
                    "nonscalar expression is unsupported by Python package emission",
                    source.source(),
                    Some(span),
                ));
            }
        }
        for (function, summary) in &source.typed().effects {
            let span = source.typed().functions.get(function).map(|s| s.span);
            for effect in &summary.effects {
                if !matches!(effect.as_str(), "Mutate" | "ReadArgs" | "Unknown") {
                    errors.push(PackageEmitDiagnostic::new(
                        "CGE004",
                        format!("effect `{effect}` is unsupported by Python package emission"),
                        source.source(),
                        span,
                    ));
                }
            }
            for capability in &summary.capabilities {
                if capability != "ArgsRead" {
                    errors.push(PackageEmitDiagnostic::new(
                        "CGE004",
                        format!(
                            "capability `{capability}` is unsupported by Python package emission"
                        ),
                        source.source(),
                        span,
                    ));
                }
            }
        }
        match lower_module(source, &names) {
            Ok((lowered, imports)) => match emit_package_module(&lowered, source.typed(), &imports)
            {
                Ok(mut python) => {
                    let exports: Vec<_> = source
                        .syntax()
                        .functions
                        .iter()
                        .filter(|f| f.public)
                        .map(|f| format!("{:?}", python_name(&f.name)))
                        .collect();
                    python.push_str(&format!("__all__ = [{}]\n", exports.join(", ")));
                    if files.insert(PathBuf::from(filename), python).is_some() {
                        errors.push(PackageEmitDiagnostic::new(
                            "CGE005",
                            "generated Python output collision",
                            source.source(),
                            source.syntax().name.as_ref().map(|p| p.span),
                        ));
                    }
                }
                Err(diagnostics) => errors.extend(diagnostics.into_iter().map(|d| {
                    PackageEmitDiagnostic::new(d.code, d.message, source.source(), Some(d.span))
                })),
            },
            Err(mut diagnostics) => errors.append(&mut diagnostics),
        }
    }
    let entry = package
        .modules()
        .iter()
        .find(|m| m.source() == package.index().entry);
    match entry {
        Some(entry) => {
            let main = entry.typed().functions.get("main");
            match main {
                Some(signature)
                    if signature.parameters.is_empty() && scalar(&signature.return_type) =>
                {
                    if let Some(name) = entry.syntax().name.as_ref().map(|p| p.segments.join(".")) {
                        files.insert(PathBuf::from("__main__.py"), format!("# Generated CGx2 package entry.\nimport sys\nimport {} as _cgx2_entry\n\n_result = _cgx2_entry.{}()\nif _result is not None:\n    sys.stdout.write(_cgx2_entry._cgx2_show(_result) + \"\\n\")\n", python_module_name(&name), python_name("main")));
                    } else {
                        errors.push(PackageEmitDiagnostic::new(
                            "CGE005",
                            "entry requires a module declaration",
                            entry.source(),
                            Some(entry.syntax().span),
                        ));
                    }
                }
                _ => errors.push(PackageEmitDiagnostic::new(
                    "CGE005",
                    "package entry requires zero-argument scalar main",
                    entry.source(),
                    Some(entry.syntax().span),
                )),
            }
        }
        None => errors.push(PackageEmitDiagnostic::new(
            "CGE005",
            "checked entry source is missing",
            &package.index().entry,
            None,
        )),
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    package.verify_unchanged().map_err(package_errors)?;
    Ok(files)
}

fn safe_output_parent(output: &Path) -> Result<PathBuf, PackageEmitDiagnostic> {
    if output
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        || output.file_name().is_none()
    {
        return Err(PackageEmitDiagnostic::new(
            "CGE006",
            "output path must not contain traversal or dot components",
            output,
            None,
        ));
    }
    let absolute = if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| {
                PackageEmitDiagnostic::new(
                    "CGE006",
                    format!("cannot read current directory: {e}"),
                    output,
                    None,
                )
            })?
            .join(output)
    };
    let parent = absolute.parent().expect("output has file name");
    let mut prefix = PathBuf::new();
    for component in parent.components() {
        prefix.push(component);
        if fs::symlink_metadata(&prefix)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(PackageEmitDiagnostic::new(
                "CGE006",
                "output parent contains a symlink",
                &prefix,
                None,
            ));
        }
    }
    fs::canonicalize(parent).map_err(|e| {
        PackageEmitDiagnostic::new(
            "CGE006",
            format!("cannot access output parent: {e}"),
            parent,
            None,
        )
    })
}

/// Write into a new directory only. Existing files and directories are never replaced.
pub fn write_package_python(
    package: &CheckedPackage,
    output: &Path,
) -> Result<Vec<PathBuf>, Vec<PackageEmitDiagnostic>> {
    let files = emit_package_python(package)?;
    let parent = safe_output_parent(output).map_err(|e| vec![e])?;
    let destination = parent.join(output.file_name().expect("validated file name"));
    fs::create_dir(&destination).map_err(|e| {
        vec![PackageEmitDiagnostic::new(
            "CGE006",
            format!("cannot create new output directory: {e}"),
            &destination,
            None,
        )]
    })?;
    let result = (|| {
        for (name, contents) in &files {
            fs::write(destination.join(name), contents).map_err(|e| {
                PackageEmitDiagnostic::new(
                    "CGE006",
                    format!("cannot write generated file: {e}"),
                    name,
                    None,
                )
            })?;
        }
        package
            .verify_unchanged()
            .map_err(|e| package_errors(e).remove(0))?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&destination);
        return Err(vec![error]);
    }
    Ok(files.keys().map(|p| destination.join(p)).collect())
}
