//! Mechanical qualification after semantic checking. Never used for admission.
use crate::*;
use std::collections::BTreeSet;

pub fn qualify_type(ty: &mut TypeSyntax, module: &Module) {
    // Lowering can store a complete nominal identity in one path segment.
    // Such a segment is already qualified, just like a split dotted path.
    if ty.path.segments.len() == 1
        && !ty.path.segments[0].contains('.')
        && (module.records.iter().any(|r| r.name == ty.path.segments[0])
            || module
                .variants
                .iter()
                .any(|v| v.name == ty.path.segments[0]))
    {
        if let Some(name) = &module.name {
            let mut parts = name.segments.clone();
            parts.extend(ty.path.segments.clone());
            ty.path.segments = parts;
        }
    }
    for arg in &mut ty.arguments {
        qualify_type(arg, module);
    }
}

/// Preserve source spans, qualify local nominal references, and make checked
/// constructor calls explicit. `known_variants` contains canonical identities.
pub fn canonical_module(module: &Module, known_variants: &BTreeSet<String>) -> Module {
    let mut result = module.clone();
    fn rewrite_expr(expr: &mut Expr, module: &Module, known: &BTreeSet<String>) {
        match &mut expr.kind {
            ExprKind::Call { callee, arguments } => {
                rewrite_expr(callee, module, known);
                for arg in &mut *arguments {
                    rewrite_expr(&mut arg.value, module, known);
                }
                if let ExprKind::Name(path) = &mut callee.kind {
                    if path.segments.len() == 1
                        && !path.segments[0].contains('.')
                        && module.records.iter().any(|r| r.name == path.segments[0])
                    {
                        if let Some(name) = &module.name {
                            path.segments =
                                vec![format!("{}.{}", name.segments.join("."), path.segments[0])];
                        }
                    }
                }
                if let ExprKind::Name(path) = &callee.kind {
                    if path.segments.len() >= 2 {
                        let prefix = path.segments[..path.segments.len() - 1].to_vec();
                        let mut ty = TypeSyntax {
                            path: Path {
                                segments: prefix,
                                span: path.span,
                            },
                            arguments: vec![],
                            span: path.span,
                        };
                        qualify_type(&mut ty, module);
                        if matches!(ty.path.segments.join(".").as_str(), "Option" | "Result")
                            || known.contains(&ty.path.segments.join("."))
                            || module
                                .variants
                                .iter()
                                .any(|v| v.name == ty.path.segments.join("."))
                        {
                            expr.kind = ExprKind::Constructor {
                                ty,
                                case: path.segments.last().expect("case").clone(),
                                arguments: arguments.clone(),
                            };
                        }
                    }
                }
            }
            ExprKind::Constructor { ty, arguments, .. } => {
                qualify_type(ty, module);
                for arg in arguments {
                    rewrite_expr(&mut arg.value, module, known);
                }
            }
            ExprKind::Binary { left, right, .. }
            | ExprKind::Index {
                collection: left,
                index: right,
            } => {
                rewrite_expr(left, module, known);
                rewrite_expr(right, module, known);
            }
            ExprKind::Collection(values) => {
                for value in values {
                    rewrite_expr(value, module, known);
                }
            }
            ExprKind::Name(_)
            | ExprKind::Integer(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::Boolean(_) => {}
        }
    }
    fn block(body: &mut [Stmt], module: &Module, known: &BTreeSet<String>) {
        for stmt in body {
            match &mut stmt.kind {
                StmtKind::Match { scrutinee, arms } => {
                    rewrite_expr(scrutinee, module, known);
                    for arm in arms {
                        if let Pattern::Case { ty, .. } = &mut arm.pattern {
                            qualify_type(ty, module);
                        }
                        block(&mut arm.body, module, known);
                    }
                }
                StmtKind::Let {
                    annotation, value, ..
                } => {
                    if let Some(ty) = annotation {
                        qualify_type(ty, module);
                    }
                    rewrite_expr(value, module, known);
                }
                StmtKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    rewrite_expr(condition, module, known);
                    block(then_body, module, known);
                    block(else_body, module, known);
                }
                StmtKind::For { iter, body, .. } | StmtKind::NoGc { budget: iter, body } => {
                    rewrite_expr(iter, module, known);
                    block(body, module, known);
                }
                StmtKind::LoopContract { expression, .. }
                | StmtKind::Expr(expression)
                | StmtKind::Assign {
                    value: expression, ..
                }
                | StmtKind::Return(Some(expression)) => rewrite_expr(expression, module, known),
                StmtKind::Return(None) => {}
            }
        }
    }
    for record in &mut result.records {
        let mut scope = module.clone();
        scope
            .records
            .retain(|r| !record.type_parameters.iter().any(|p| p.name == r.name));
        scope
            .variants
            .retain(|v| !record.type_parameters.iter().any(|p| p.name == v.name));
        for field in &mut record.fields {
            qualify_type(&mut field.ty, &scope);
        }
    }
    for variant in &mut result.variants {
        let mut scope = module.clone();
        scope
            .records
            .retain(|r| !variant.type_parameters.iter().any(|p| p.name == r.name));
        scope
            .variants
            .retain(|v| !variant.type_parameters.iter().any(|p| p.name == v.name));
        for case in &mut variant.cases {
            for payload in &mut case.payloads {
                qualify_type(payload, &scope);
            }
        }
    }
    for function in &mut result.functions {
        let mut scope = module.clone();
        scope
            .records
            .retain(|r| !function.type_parameters.iter().any(|p| p.name == r.name));
        scope
            .variants
            .retain(|v| !function.type_parameters.iter().any(|p| p.name == v.name));
        for parameter in &mut function.parameters {
            qualify_type(&mut parameter.ty, &scope);
        }
        if let Some(ty) = &mut function.return_type {
            qualify_type(ty, &scope);
        }
        for condition in function.requires.iter_mut().chain(&mut function.ensures) {
            rewrite_expr(condition, &scope, known_variants);
        }
        block(&mut function.body, &scope, known_variants);
    }
    block(&mut result.statements, module, known_variants);
    result
}
