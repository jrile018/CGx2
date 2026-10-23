use crate::{AssignOp, BinaryOp, Expr, ExprKind, Function, Module, Stmt, StmtKind};

/// Print a parsed module in the candidate canonical four-space style.
pub fn format_module(module: &Module) -> String {
    let mut out = String::new();
    if let Some(name) = &module.name {
        out.push_str("module ");
        out.push_str(&name.segments.join("."));
        out.push('\n');
    }
    for import in &module.imports {
        out.push_str("import ");
        out.push_str(&import.segments.join("."));
        out.push('\n');
    }
    if !out.is_empty()
        && ((!module.records.is_empty() || !module.variants.is_empty())
            || !module.variants.is_empty()
            || !module.functions.is_empty()
            || !module.statements.is_empty())
    {
        out.push('\n');
    }
    for (index, record) in module.records.iter().enumerate() {
        if record.public {
            out.push_str("pub ");
        }
        out.push_str("record ");
        out.push_str(&record.name);
        write_type_parameters(&mut out, &record.type_parameters);
        out.push_str(":\n");
        for field in &record.fields {
            out.push_str("    ");
            out.push_str(&field.name);
            out.push_str(": ");
            out.push_str(&field.ty.to_string());
            out.push('\n');
        }
        if index + 1 < module.records.len()
            || !module.variants.is_empty()
            || !module.functions.is_empty()
            || !module.statements.is_empty()
        {
            out.push('\n');
        }
    }
    for variant in &module.variants {
        if variant.public {
            out.push_str("pub ");
        }
        out.push_str("variant ");
        out.push_str(&variant.name);
        write_type_parameters(&mut out, &variant.type_parameters);
        out.push_str(":\n");
        for case in &variant.cases {
            out.push_str("    ");
            out.push_str(&case.name);
            if !case.payloads.is_empty() {
                out.push('(');
                out.push_str(
                    &case
                        .payloads
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                out.push(')');
            }
            out.push('\n');
        }
        out.push('\n');
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
    if let Some(binding) = &function.foreign {
        if binding.unsafe_call {
            out.push_str("unsafe ");
        }
        out.push_str("foreign ");
        out.push_str(&binding.route);
        out.push(' ');
    }
    if function.public {
        out.push_str("pub ");
    }
    out.push_str("fn ");
    out.push_str(&function.name);
    write_type_parameters(out, &function.type_parameters);
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
    for (label, declaration) in [
        ("effects", &function.effects),
        ("caps", &function.capabilities),
    ] {
        if let Some(declaration) = declaration {
            out.push(' ');
            out.push_str(label);
            out.push('(');
            out.push_str(
                &declaration
                    .names
                    .iter()
                    .map(|name| name.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            out.push(')');
        }
    }
    if let Some(binding) = &function.foreign {
        out.push_str(" = ");
        out.push_str(&binding.target.segments.join("."));
        out.push_str(" runtime \"");
        out.push_str(&binding.runtime);
        out.push_str("\" ownership ");
        out.push_str(&binding.ownership);
        out.push('\n');
        return;
    }
    out.push_str(":\n");
    for (label, contracts) in [
        ("requires", &function.requires),
        ("ensures", &function.ensures),
    ] {
        for contract in contracts {
            out.push_str("    ");
            out.push_str(label);
            out.push(' ');
            write_expr(out, contract, 0);
            out.push('\n');
        }
    }
    for stmt in &function.body {
        write_stmt(out, stmt, 1);
    }
}

fn write_stmt(out: &mut String, stmt: &Stmt, indent: usize) {
    for _ in 0..indent {
        out.push_str("    ");
    }
    match &stmt.kind {
        StmtKind::Match { scrutinee, arms } => {
            out.push_str("match ");
            write_expr(out, scrutinee, 0);
            out.push_str(":\n");
            for arm in arms {
                out.push_str(&"    ".repeat(indent + 1));
                match &arm.pattern {
                    crate::Pattern::Wildcard(_) => out.push('_'),
                    crate::Pattern::Case {
                        ty, case, bindings, ..
                    } => {
                        out.push_str(&ty.to_string());
                        out.push('.');
                        out.push_str(case);
                        if !bindings.is_empty() {
                            out.push('(');
                            out.push_str(
                                &bindings
                                    .iter()
                                    .map(|b| b.name.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            );
                            out.push(')');
                        }
                    }
                }
                out.push_str(":\n");
                for nested in &arm.body {
                    write_stmt(out, nested, indent + 2);
                }
            }
        }
        StmtKind::LoopContract {
            decreases,
            expression,
        } => {
            out.push_str(if *decreases {
                "decreases "
            } else {
                "invariant "
            });
            write_expr(out, expression, 0);
            out.push('\n');
        }

        StmtKind::If {
            condition,
            then_body,
            else_body,
        } => {
            out.push_str("if ");
            write_expr(out, condition, 0);
            out.push_str(":\n");
            for nested in then_body {
                write_stmt(out, nested, indent + 1);
            }
            out.push_str(&"    ".repeat(indent));
            out.push_str("else:\n");
            for nested in else_body {
                write_stmt(out, nested, indent + 1);
            }
        }
        StmtKind::NoGc { budget, body } => {
            out.push_str("no_gc(budget: ");
            write_expr(out, budget, 0);
            out.push_str("):\n");
            for nested in body {
                write_stmt(out, nested, indent + 1);
            }
        }
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
        ExprKind::Binary {
            op: BinaryOp::Multiply | BinaryOp::Divide,
            ..
        } => 3,
        ExprKind::Call { callee, .. } if comparison_symbol(callee).is_some() => 0,
        ExprKind::Call { .. } => 7,
        _ => 9,
    }
}

fn comparison_symbol(callee: &Expr) -> Option<&'static str> {
    let ExprKind::Name(path) = &callee.kind else {
        return None;
    };
    match path.segments.as_slice() {
        [name] => match name.as_str() {
            "$cgx2_cmp_eq" => Some(" == "),
            "$cgx2_cmp_ne" => Some(" != "),
            "$cgx2_cmp_lt" => Some(" < "),
            "$cgx2_cmp_le" => Some(" <= "),
            "$cgx2_cmp_gt" => Some(" > "),
            "$cgx2_cmp_ge" => Some(" >= "),
            _ => None,
        },
        _ => None,
    }
}

fn write_expr(out: &mut String, expr: &Expr, required: u8) {
    let bp = precedence(expr);
    let parenthesize = bp < required;
    if parenthesize {
        out.push('(');
    }
    match &expr.kind {
        ExprKind::Constructor {
            ty,
            case,
            arguments,
        } => {
            out.push_str(&ty.to_string());
            out.push('.');
            out.push_str(case);
            out.push('(');
            for (i, arg) in arguments.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                if let Some(name) = &arg.name {
                    out.push_str(name);
                    out.push_str(": ");
                }
                write_expr(out, &arg.value, 0);
            }
            out.push(')');
        }
        ExprKind::Collection(values) => {
            out.push('[');
            for (i, value) in values.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_expr(out, value, 0);
            }
            out.push(']');
        }
        ExprKind::Index { collection, index } => {
            write_expr(out, collection, 7);
            out.push('[');
            write_expr(out, index, 0);
            out.push(']');
        }

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
            if let (Some(symbol), [left, right]) = (comparison_symbol(callee), arguments.as_slice())
            {
                write_expr(out, &left.value, 0);
                out.push_str(symbol);
                write_expr(out, &right.value, 1);
                if parenthesize {
                    out.push(')');
                }
                return;
            }
            write_expr(out, callee, 7);
            out.push('(');
            for (index, arg) in arguments.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                if let Some(name) = &arg.name {
                    out.push_str(name);
                    out.push_str(": ");
                }
                write_expr(out, &arg.value, 0);
            }
            out.push(')');
        }
    }
    if parenthesize {
        out.push(')');
    }
}

/// Canonical expression text for source-visible contract summaries.
pub fn format_expression(expr: &Expr) -> String {
    let mut out = String::new();
    write_expr(&mut out, expr, 0);
    out
}

fn write_type_parameters(out: &mut String, parameters: &[crate::TypeParameter]) {
    if parameters.is_empty() {
        return;
    }
    out.push('<');
    for (index, parameter) in parameters.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(&parameter.name);
        if !parameter.constraints.is_empty() {
            out.push_str(": ");
            out.push_str(
                &parameter
                    .constraints
                    .iter()
                    .map(|p| p.segments.join("."))
                    .collect::<Vec<_>>()
                    .join(" + "),
            );
        }
    }
    out.push('>');
}
