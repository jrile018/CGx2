//! Mechanical qualification after semantic checking. Never used for admission.
use crate::*;
use std::collections::BTreeSet;

pub fn qualify_type(ty: &mut TypeSyntax, module: &Module) {
    if ty.path.segments.len() == 1 && module.records.iter().any(|r| r.name == ty.path.segments[0])
        || ty.path.segments.len() == 1
            && module
                .variants
                .iter()
                .any(|v| v.name == ty.path.segments[0])
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
                        if known.contains(&ty.path.segments.join("."))
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
        for field in &mut record.fields {
            qualify_type(&mut field.ty, module);
        }
    }
    for variant in &mut result.variants {
        for case in &mut variant.cases {
            for payload in &mut case.payloads {
                qualify_type(payload, module);
            }
        }
    }
    for function in &mut result.functions {
        for parameter in &mut function.parameters {
            qualify_type(&mut parameter.ty, module);
        }
        if let Some(ty) = &mut function.return_type {
            qualify_type(ty, module);
        }
        for condition in function.requires.iter_mut().chain(&mut function.ensures) {
            rewrite_expr(condition, module, known_variants);
        }
        block(&mut function.body, module, known_variants);
    }
    block(&mut result.statements, module, known_variants);
    result
}
