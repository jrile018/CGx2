use crate::{AssignOp, BinaryOp, Expr, ExprKind, Function, Module, Stmt, StmtKind};

/// Print a parsed module in the candidate canonical four-space style.
pub fn format_module(module: &Module) -> String {
    let mut out = String::new();
    if let Some(name) = &module.name {
        out.push_str("module ");
        out.push_str(&name.segments.join("."));
        out.push('\n');
        if !module.functions.is_empty() || !module.statements.is_empty() {
            out.push('\n');
        }
    }
    for (index, function) in module.functions.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        write_function(&mut out, function);
    }
    for stmt in &module.statements {
        write_stmt(&mut out, stmt, 0);
    }
    if out.is_empty() {
        out.push('\n');
    }
    out
}

fn write_function(out: &mut String, function: &Function) {
    if function.public {
        out.push_str("pub ");
    }
    out.push_str("fn ");
    out.push_str(&function.name);
    out.push('(');
    for (index, parameter) in function.parameters.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(&parameter.name);
        out.push_str(": ");
        out.push_str(&parameter.ty.to_string());
    }
    out.push(')');
    if let Some(ty) = &function.return_type {
        out.push_str(" -> ");
        out.push_str(&ty.to_string());
    }
    out.push_str(":\n");
    for stmt in &function.body {
        write_stmt(out, stmt, 1);
    }
}

fn write_stmt(out: &mut String, stmt: &Stmt, indent: usize) {
    for _ in 0..indent {
        out.push_str("    ");
    }
    match &stmt.kind {
        StmtKind::Let {
            mutable,
            name,
            annotation,
            value,
        } => {
            out.push_str("let ");
            if *mutable {
                out.push_str("mut ");
            }
            out.push_str(name);
            if let Some(ty) = annotation {
                out.push_str(": ");
                out.push_str(&ty.to_string());
            }
            out.push_str(" = ");
            write_expr(out, value, 0);
            out.push('\n');
        }
        StmtKind::Assign { target, op, value } => {
            out.push_str(&target.segments.join("."));
            out.push_str(match op {
                AssignOp::Set => " = ",
                AssignOp::Add => " += ",
            });
            write_expr(out, value, 0);
            out.push('\n');
        }
        StmtKind::Return(value) => {
            out.push_str("return");
            if let Some(value) = value {
                out.push(' ');
                write_expr(out, value, 0);
            }
            out.push('\n');
        }
        StmtKind::Expr(expr) => {
            write_expr(out, expr, 0);
            out.push('\n');
        }
        StmtKind::For {
            binding,
            iter,
            body,
        } => {
            out.push_str("for ");
            out.push_str(binding);
            out.push_str(" in ");
            write_expr(out, iter, 0);
            out.push_str(":\n");
            for nested in body {
                write_stmt(out, nested, indent + 1);
            }
        }
    }
}

fn precedence(expr: &Expr) -> u8 {
    match &expr.kind {
        ExprKind::Binary {
            op: BinaryOp::Add | BinaryOp::Subtract,
            ..
        } => 1,
        ExprKind::Binary { .. } => 3,
        ExprKind::Call { .. } => 7,
        _ => 9,
    }
}

fn write_expr(out: &mut String, expr: &Expr, required: u8) {
    let bp = precedence(expr);
    let parenthesize = bp < required;
    if parenthesize {
        out.push('(');
    }
    match &expr.kind {
        ExprKind::Name(path) => out.push_str(&path.segments.join(".")),
        ExprKind::Integer(value) | ExprKind::StringLiteral(value) => out.push_str(value),
        ExprKind::Boolean(value) => out.push_str(if *value { "true" } else { "false" }),
        ExprKind::Binary { left, op, right } => {
            write_expr(out, left, bp);
            out.push_str(match op {
                BinaryOp::Add => " + ",
                BinaryOp::Subtract => " - ",
                BinaryOp::Multiply => " * ",
                BinaryOp::Divide => " / ",
            });
            write_expr(out, right, bp + 1);
        }
        ExprKind::Call { callee, arguments } => {
            write_expr(out, callee, 7);
            out.push('(');
            for (index, arg) in arguments.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write_expr(out, arg, 0);
            }
            out.push(')');
        }
    }
    if parenthesize {
        out.push(')');
    }
}
