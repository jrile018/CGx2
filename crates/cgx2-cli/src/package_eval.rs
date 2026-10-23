//! Interpretation of a checked local package. Qualification is internal to the
//! existing AST; byte spans stay in their defining files.
use crate::{eval, NativeDiagnostic, RunResult};
use cgx2_package::CheckedPackage;
use cgx2_syntax::{Expr, ExprKind, Function, Module, Stmt, StmtKind, TypeSyntax};
use std::collections::BTreeMap;

fn identity(module: &str, name: &str) -> String {
    format!("{module}.{name}")
}

fn ty(ty: &mut TypeSyntax, module: &str, records: &BTreeMap<String, String>) {
    for arg in &mut ty.arguments {
        self::ty(arg, module, records);
    }
    let name = ty.path.segments.join(".");
    let qualified = if ty.path.segments.len() == 1 {
        identity(module, &name)
    } else {
        name
    };
    if let Some(name) = records.get(&qualified) {
        ty.path.segments = vec![name.clone()];
    }
}
fn expr(expr: &mut Expr, module: &str, calls: &BTreeMap<String, String>) {
    match &mut expr.kind {
        ExprKind::Constructor { arguments, .. } => {
            for arg in arguments {
                self::expr(&mut arg.value, module, calls);
            }
        }
        ExprKind::Collection(values) => {
            for value in values {
                self::expr(value, module, calls);
            }
        }
        ExprKind::Index { collection, index } => {
            self::expr(collection, module, calls);
            self::expr(index, module, calls);
        }
        ExprKind::Binary { left, right, .. } => {
            self::expr(left, module, calls);
            self::expr(right, module, calls);
        }
        ExprKind::Call { callee, arguments } => {
            if let ExprKind::Name(path) = &mut callee.kind {
                let name = path.segments.join(".");
                let full = if path.segments.len() == 1 {
                    identity(module, &name)
                } else {
                    name
                };
                if let Some(name) = calls.get(&full) {
                    path.segments = vec![name.clone()];
                }
            }
            for arg in arguments {
                self::expr(&mut arg.value, module, calls);
            }
        }
        _ => {}
    }
}
fn block(
    body: &mut [Stmt],
    module: &str,
    calls: &BTreeMap<String, String>,
    records: &BTreeMap<String, String>,
) {
    for stmt in body {
        match &mut stmt.kind {
            StmtKind::Match { scrutinee, arms } => {
                expr(scrutinee, module, calls);
                for arm in arms {
                    if let cgx2_syntax::Pattern::Case { ty, .. } = &mut arm.pattern {
                        self::ty(ty, module, records);
                    }
                    block(&mut arm.body, module, calls, records);
                }
            }
            StmtKind::LoopContract { expression, .. } => expr(expression, module, calls),
            StmtKind::Let {
                annotation, value, ..
            } => {
                if let Some(ty) = annotation {
                    self::ty(ty, module, records);
                }
                expr(value, module, calls);
            }
            StmtKind::Assign { value, .. }
            | StmtKind::Expr(value)
            | StmtKind::Return(Some(value)) => expr(value, module, calls),
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                expr(condition, module, calls);
                block(then_body, module, calls, records);
                block(else_body, module, calls, records);
            }
            StmtKind::For { iter, body, .. } => {
                expr(iter, module, calls);
                block(body, module, calls, records);
            }
            _ => {}
        }
    }
}

pub(crate) fn run(
    package: &CheckedPackage,
    args: &[String],
) -> Result<RunResult, Vec<NativeDiagnostic>> {
    for source in package.modules() {
        if source.syntax().name.is_none() {
            return Err(vec![NativeDiagnostic {
                code: "CGR009".into(),
                message: "pure package execution requires module declarations".into(),
                file: source.source().to_path_buf(),
                span: Some(source.syntax().span),
            }]);
        }
        if let Some(stmt) = source.syntax().statements.first() {
            return Err(vec![NativeDiagnostic {
                code: "CGR009".into(),
                message: "pure package execution requires function modules".into(),
                file: source.source().to_path_buf(),
                span: Some(stmt.span),
            }]);
        }
    }
    let entry = package
        .modules()
        .iter()
        .find(|m| m.source() == package.index().entry)
        .expect("checked entry");
    let entry_name = entry
        .syntax()
        .name
        .as_ref()
        .map(|p| p.segments.join("."))
        .unwrap_or_default();
    let mut module: Module = entry.syntax().clone();
    module.name = None;
    module.imports.clear();
    module.records.clear();
    module.variants.clear();
    module.functions.clear();
    let mut calls = BTreeMap::new();
    let mut records = BTreeMap::new();
    let mut origins = BTreeMap::new();
    let known_variants = package
        .modules()
        .iter()
        .flat_map(|source| {
            source.syntax().variants.iter().map(move |v| {
                format!(
                    "{}.{}",
                    source
                        .syntax()
                        .name
                        .as_ref()
                        .expect("named module")
                        .segments
                        .join("."),
                    v.name
                )
            })
        })
        .collect();
    for source in package.modules() {
        let canonical = cgx2_syntax::canonical_module(source.syntax(), &known_variants);
        let name = source
            .syntax()
            .name
            .as_ref()
            .map(|p| p.segments.join("."))
            .unwrap_or_default();
        for variant in &canonical.variants {
            let full = identity(&name, &variant.name);
            records.insert(full.clone(), full.clone());
            let mut variant = variant.clone();
            variant.name = full;
            module.variants.push(variant);
        }
        for record in &canonical.records {
            let full = identity(&name, &record.name);
            records.insert(full.clone(), full.clone());
            calls.insert(full.clone(), full.clone());
            let mut record = record.clone();
            record.name = full;
            module.records.push(record);
        }
        for function in &source.syntax().functions {
            let full = identity(&name, &function.name);
            let lowered = if name == entry_name && function.name == "main" {
                "main".to_owned()
            } else {
                full.clone()
            };
            calls.insert(full, lowered.clone());
            origins.insert(lowered, source.source().to_path_buf());
        }
    }
    for source in package.modules() {
        let canonical = cgx2_syntax::canonical_module(source.syntax(), &known_variants);
        let name = source
            .syntax()
            .name
            .as_ref()
            .map(|p| p.segments.join("."))
            .unwrap_or_default();
        for function in &canonical.functions {
            let mut function: Function = function.clone();
            function.name = calls[&identity(&name, &function.name)].clone();
            for parameter in &mut function.parameters {
                ty(&mut parameter.ty, &name, &records);
            }
            if let Some(result) = &mut function.return_type {
                ty(result, &name, &records);
            }
            for condition in function.requires.iter_mut().chain(&mut function.ensures) {
                expr(condition, &name, &calls);
            }
            block(&mut function.body, &name, &calls, &records);
            module.functions.push(function);
        }
    }
    block(&mut module.statements, &entry_name, &calls, &records);
    package.verify_unchanged().map_err(|errors| {
        errors
            .into_iter()
            .map(|d| NativeDiagnostic {
                code: d.code.into(),
                message: d.message,
                file: d.file,
                span: d.span,
            })
            .collect::<Vec<_>>()
    })?;
    let (result, file) = eval::run_package_module(&module, args, &origins);
    result.map_err(|error| {
        vec![NativeDiagnostic {
            code: error.code.into(),
            message: error.message,
            file: file.unwrap_or_else(|| entry.source().to_path_buf()),
            span: error.span,
        }]
    })
}
