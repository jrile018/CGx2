//! Exhaustive shared traversal for source admission and conservative analysis.
use crate::{Expr, ExprKind, Module, Span, Stmt, StmtKind, TypeSyntax};

pub fn visit_expr(expr: &Expr, visitor: &mut impl FnMut(&Expr)) {
    visitor(expr);
    match &expr.kind {
        ExprKind::Call { callee, arguments } => {
            visit_expr(callee, visitor);
            for arg in arguments {
                visit_expr(&arg.value, visitor);
            }
        }
        ExprKind::Constructor { arguments, .. } => {
            for arg in arguments {
                visit_expr(&arg.value, visitor);
            }
        }
        ExprKind::Collection(values) => {
            for value in values {
                visit_expr(value, visitor);
            }
        }
        ExprKind::Index { collection, index }
        | ExprKind::Binary {
            left: collection,
            right: index,
            ..
        } => {
            visit_expr(collection, visitor);
            visit_expr(index, visitor);
        }
        ExprKind::Name(_)
        | ExprKind::Integer(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::Boolean(_) => {}
    }
}

pub fn visit_stmts(
    statements: &[Stmt],
    stmt_visitor: &mut impl FnMut(&Stmt),
    expr_visitor: &mut impl FnMut(&Expr),
) {
    for stmt in statements {
        stmt_visitor(stmt);
        match &stmt.kind {
            StmtKind::Match { scrutinee, arms } => {
                visit_expr(scrutinee, expr_visitor);
                for arm in arms {
                    visit_stmts(&arm.body, stmt_visitor, expr_visitor);
                }
            }
            StmtKind::LoopContract { expression, .. } => visit_expr(expression, expr_visitor),
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                visit_expr(condition, expr_visitor);
                visit_stmts(then_body, stmt_visitor, expr_visitor);
                visit_stmts(else_body, stmt_visitor, expr_visitor);
            }
            StmtKind::For { iter, body, .. } | StmtKind::NoGc { budget: iter, body } => {
                visit_expr(iter, expr_visitor);
                visit_stmts(body, stmt_visitor, expr_visitor);
            }
            StmtKind::Let { value, .. }
            | StmtKind::Assign { value, .. }
            | StmtKind::Expr(value)
            | StmtKind::Return(Some(value)) => visit_expr(value, expr_visitor),
            StmtKind::Return(None) => {}
        }
    }
}

/// First S2a interface feature in source order. Frozen consumers use this to
/// reject the complete profile before their older traversals execute.
pub fn s2a_feature_span(module: &Module) -> Option<Span> {
    use std::cell::RefCell;
    let spans = RefCell::new(Vec::new());
    let ty_span = |ty: &TypeSyntax| {
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            if (!ty.arguments.is_empty() && ty.path.segments.as_slice() != ["List"])
                || matches!(
                    ty.path.segments.last().map(String::as_str),
                    Some("Option" | "Result" | "Array")
                )
            {
                spans.borrow_mut().push(ty.span);
            }
            pending.extend(&ty.arguments);
        }
    };
    for variant in &module.variants {
        spans.borrow_mut().push(variant.span);
    }
    for record in &module.records {
        for p in &record.type_parameters {
            spans.borrow_mut().push(p.span);
        }
        for field in &record.fields {
            if !matches!(field.ty.to_string().as_str(), "i64" | "Bool") {
                spans.borrow_mut().push(field.ty.span);
            }
        }
    }
    let mut stmt_visitor = |stmt: &Stmt| match &stmt.kind {
        StmtKind::Match { .. } | StmtKind::LoopContract { .. } => {
            spans.borrow_mut().push(stmt.span)
        }
        StmtKind::Let {
            annotation: Some(ty),
            ..
        } => ty_span(ty),
        _ => {}
    };
    let mut expr_visitor = |expr: &Expr| {
        if matches!(
            expr.kind,
            ExprKind::Constructor { .. } | ExprKind::Collection(_) | ExprKind::Index { .. }
        ) {
            spans.borrow_mut().push(expr.span);
        }
    };
    visit_stmts(&module.statements, &mut stmt_visitor, &mut expr_visitor);
    for function in &module.functions {
        for p in &function.type_parameters {
            spans.borrow_mut().push(p.span);
        }
        for p in &function.parameters {
            ty_span(&p.ty);
        }
        if let Some(ty) = &function.return_type {
            ty_span(ty);
        }
        visit_stmts(&function.body, &mut stmt_visitor, &mut expr_visitor);
        for expr in function.requires.iter().chain(&function.ensures) {
            visit_expr(expr, &mut expr_visitor);
        }
    }
    spans
        .into_inner()
        .into_iter()
        .min_by_key(|span| (span.start, span.end))
}
