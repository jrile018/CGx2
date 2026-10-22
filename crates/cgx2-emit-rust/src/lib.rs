//! Readable, standalone Rust output for checked CGx2 scalar programs.

use cgx2_sema::{Type, TypedModule, check_module};
use cgx2_syntax::{AssignOp, BinaryOp, Expr, ExprKind, Module, Span, Stmt, StmtKind, TypeSyntax};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmitDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

impl EmitDiagnostic {
    fn new(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self {
            code,
            message: message.into(),
            span,
        }
    }
}

/// Emit a standalone Rust 2024 program from a checked scalar module.
pub fn emit_rust(module: &Module, checked: &TypedModule) -> Result<String, Vec<EmitDiagnostic>> {
    let mut errors = Vec::new();
    let functions = module
        .functions
        .iter()
        .map(|f| f.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if !module.functions.is_empty() && !module.statements.is_empty() {
        errors.push(EmitDiagnostic::new(
            "CGRS001",
            "function items and script statements cannot be mixed",
            module.statements[0].span,
        ));
    }
    for function in &module.functions {
        for parameter in &function.parameters {
            validate_type(&parameter.ty, &mut errors);
        }
        if let Some(ty) = &function.return_type {
            validate_type(ty, &mut errors);
        }
        validate_block(&function.body, false, &functions, &mut errors);
    }
    validate_block(&module.statements, true, &functions, &mut errors);
    if !module.functions.is_empty() {
        match module.functions.iter().find(|f| f.name == "main") {
            Some(main) if !main.parameters.is_empty() => errors.push(EmitDiagnostic::new(
                "CGRS001",
                "main must have zero parameters",
                main.span,
            )),
            None => errors.push(EmitDiagnostic::new(
                "CGRS001",
                "function module requires a main function",
                module.span,
            )),
            _ => {}
        }
        // The preview evaluator bounds recursion and counts every expression. Reject cycles
        // until generated code has an equivalent recursion and step contract.
        let mut stack = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for function in &module.functions {
            find_cycle(&function.name, module, &mut stack, &mut seen, &mut errors);
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    match check_module(module) {
        Ok(fresh) if fresh == *checked => {}
        _ => {
            return Err(vec![EmitDiagnostic::new(
                "CGRS002",
                "typed summary does not match a fresh check of the syntax tree",
                module.span,
            )]);
        }
    }

    let mut out = String::from(PRELUDE);
    for function in &module.functions {
        let signature = &checked.functions[&function.name];
        let params = function
            .parameters
            .iter()
            .zip(&signature.parameters)
            .map(|(p, ty)| format!(", {}: {}", rust_name(&p.name), rust_type(ty)))
            .collect::<String>();
        out.push_str(&format!(
            "fn {}(ctx: &mut Cgx2Context, args: &[String]{params}) -> Result<{}, String> {{\n",
            rust_name(&function.name),
            rust_type(&signature.return_type)
        ));
        out.push_str("    ctx.enter()?;\n");
        out.push_str("    let result = (|| -> Result<_, String> {\n");
        emit_block(&function.body, 2, &mut out);
        out.push_str("    })();\n    ctx.leave();\n    result\n}\n\n");
    }
    if module.functions.is_empty() {
        let ty = tail_type(&module.statements, checked);
        out.push_str(&format!(
            "fn cgx2_script(ctx: &mut Cgx2Context, args: &[String]) -> Result<{}, String> {{\n",
            rust_type(&ty)
        ));
        emit_block(&module.statements, 1, &mut out);
        out.push_str("}\n\n");
    }
    let entry = if module.functions.is_empty() {
        "cgx2_script(&mut ctx, &args)".to_owned()
    } else {
        format!("{}(&mut ctx, &args)", rust_name("main"))
    };
    let result_type = if module.functions.is_empty() {
        tail_type(&module.statements, checked)
    } else {
        checked.functions["main"].return_type.clone()
    };
    out.push_str("fn main() {\n    let args: Vec<String> = std::env::args().skip(1).collect();\n    let mut ctx = Cgx2Context::default();\n");
    out.push_str(&format!("    match {entry} {{\n"));
    if result_type == Type::Unit {
        out.push_str("        Ok(()) => {},\n");
    } else {
        out.push_str("        Ok(value) => println!(\"{value}\"),\n");
    }
    out.push_str(
        "        Err(message) => { eprintln!(\"{message}\"); std::process::exit(1); }\n    }\n}\n",
    );
    Ok(out)
}

const PRELUDE: &str = r#"// Generated from CGx2's checked scalar profile.
// This file compiles with rustc --edition=2024 and has no external dependencies.
#[derive(Default)]
struct Cgx2Context { steps: usize, depth: usize }

impl Cgx2Context {
    fn step(&mut self) -> Result<(), String> {
        if self.steps >= 1_000_000 { return Err("execution step limit of 1000000 exceeded".into()); }
        self.steps += 1;
        Ok(())
    }
    fn enter(&mut self) -> Result<(), String> {
        if self.depth >= 64 { return Err("function call depth limit of 64 exceeded".into()); }
        self.depth += 1;
        Ok(())
    }
    fn leave(&mut self) { self.depth -= 1; }
}

fn cgx2_add(left: i64, right: i64) -> Result<i64, String> {
    left.checked_add(right).ok_or_else(|| "i64 arithmetic overflow".into())
}
fn cgx2_sub(left: i64, right: i64) -> Result<i64, String> {
    left.checked_sub(right).ok_or_else(|| "i64 arithmetic overflow".into())
}
fn cgx2_mul(left: i64, right: i64) -> Result<i64, String> {
    left.checked_mul(right).ok_or_else(|| "i64 arithmetic overflow".into())
}
fn cgx2_div(left: i64, right: i64) -> Result<i64, String> {
    if right == 0 { return Err("division by zero".into()); }
    left.checked_div(right).ok_or_else(|| "i64 arithmetic overflow".into())
}
fn cgx2_arg_count(args: &[String]) -> Result<i64, String> {
    i64::try_from(args.len()).map_err(|_| "argument count exceeds i64 range".into())
}
fn cgx2_arg(args: &[String], index: i64) -> Result<String, String> {
    let index = usize::try_from(index).map_err(|_| "arg index must be nonnegative".to_owned())?;
    args.get(index).cloned().ok_or_else(|| format!("arg index {index} is out of range"))
}

"#;

fn validate_type(ty: &TypeSyntax, errors: &mut Vec<EmitDiagnostic>) {
    if !ty.arguments.is_empty()
        || ty.path.segments.len() != 1
        || !matches!(
            ty.path.segments[0].as_str(),
            "i64" | "Bool" | "String" | "Unit"
        )
    {
        errors.push(EmitDiagnostic::new(
            "CGRS001",
            format!("type {ty} is unsupported by Rust scalar emission"),
            ty.span,
        ));
    }
}

fn validate_block(
    stmts: &[Stmt],
    script: bool,
    functions: &std::collections::BTreeSet<&str>,
    errors: &mut Vec<EmitDiagnostic>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Let {
                annotation, value, ..
            } => {
                if let Some(ty) = annotation {
                    validate_type(ty, errors);
                }
                validate_expr(value, functions, errors);
            }
            StmtKind::Assign { target, value, .. } => {
                if target.segments.len() != 1 {
                    errors.push(EmitDiagnostic::new(
                        "CGRS001",
                        "qualified assignment is unsupported by Rust emission",
                        target.span,
                    ));
                }
                validate_expr(value, functions, errors);
            }
            StmtKind::Return(value) => {
                if script {
                    errors.push(EmitDiagnostic::new(
                        "CGRS001",
                        "return outside a function is unsupported",
                        stmt.span,
                    ));
                }
                if let Some(expr) = value {
                    validate_expr(expr, functions, errors);
                }
            }
            StmtKind::Expr(expr) => validate_expr(expr, functions, errors),
            StmtKind::For { .. } => errors.push(EmitDiagnostic::new(
                "CGRS001",
                "for loops are unsupported by Rust scalar emission",
                stmt.span,
            )),
            StmtKind::NoGc { .. } => errors.push(EmitDiagnostic::new(
                "CGRS001",
                "no_gc and memory are unsupported by Rust scalar emission",
                stmt.span,
            )),
        }
    }
}

fn validate_expr(
    expr: &Expr,
    functions: &std::collections::BTreeSet<&str>,
    errors: &mut Vec<EmitDiagnostic>,
) {
    match &expr.kind {
        ExprKind::Name(path) if path.segments.len() != 1 => errors.push(EmitDiagnostic::new(
            "CGRS001",
            "qualified names are unsupported by Rust scalar emission",
            path.span,
        )),
        ExprKind::Integer(raw) if raw.replace('_', "").parse::<i64>().is_err() => {
            errors.push(EmitDiagnostic::new(
                "CGRS003",
                "integer literal cannot be represented as i64",
                expr.span,
            ))
        }
        ExprKind::StringLiteral(raw) if decode_string(raw).is_none() => {
            errors.push(EmitDiagnostic::new(
                "CGRS003",
                "string literal contains an unsupported escape",
                expr.span,
            ))
        }
        ExprKind::Binary { left, right, .. } => {
            validate_expr(left, functions, errors);
            validate_expr(right, functions, errors);
        }
        ExprKind::Call { callee, arguments } => {
            if matches!(&callee.kind, ExprKind::Name(path) if path.segments.first().is_some_and(|s| s == "memory"))
            {
                errors.push(EmitDiagnostic::new(
                    "CGRS001",
                    "memory operations are unsupported by Rust scalar emission",
                    expr.span,
                ));
                return;
            }
            if !matches!(&callee.kind, ExprKind::Name(path) if path.segments.len() == 1) {
                errors.push(EmitDiagnostic::new(
                    "CGRS001",
                    "computed or qualified calls are unsupported by Rust scalar emission",
                    callee.span,
                ));
            } else if let ExprKind::Name(path) = &callee.kind {
                let name = path.segments[0].as_str();
                if !matches!(name, "arg" | "arg_count") && !functions.contains(name) {
                    errors.push(EmitDiagnostic::new(
                        "CGRS001",
                        format!("unknown or unsupported call `{name}` for Rust scalar emission"),
                        callee.span,
                    ));
                }
            }
            for arg in arguments {
                if arg.name.is_some() {
                    errors.push(EmitDiagnostic::new(
                        "CGRS001",
                        "named calls are unsupported by Rust scalar emission",
                        arg.span,
                    ));
                }
                validate_expr(&arg.value, functions, errors);
            }
        }
        _ => {}
    }
}

fn find_cycle<'a>(
    name: &'a str,
    module: &'a Module,
    stack: &mut Vec<&'a str>,
    seen: &mut std::collections::BTreeSet<&'a str>,
    errors: &mut Vec<EmitDiagnostic>,
) {
    if seen.contains(name) {
        return;
    }
    let Some(function) = module.functions.iter().find(|f| f.name == name) else {
        return;
    };
    stack.push(name);
    let mut calls = Vec::new();
    for stmt in &function.body {
        calls_in_stmt(stmt, &mut calls);
    }
    for (callee, span) in calls {
        if stack.contains(&callee) {
            errors.push(EmitDiagnostic::new(
                "CGRS001",
                "recursive calls are unsupported by Rust scalar emission",
                span,
            ));
        } else if module.functions.iter().any(|f| f.name == callee) {
            find_cycle(callee, module, stack, seen, errors);
        }
    }
    stack.pop();
    seen.insert(name);
}

fn calls_in_stmt<'a>(stmt: &'a Stmt, calls: &mut Vec<(&'a str, Span)>) {
    match &stmt.kind {
        StmtKind::Let { value, .. } | StmtKind::Assign { value, .. } | StmtKind::Expr(value) => {
            calls_in_expr(value, calls)
        }
        StmtKind::Return(Some(value)) => calls_in_expr(value, calls),
        _ => {}
    }
}

fn calls_in_expr<'a>(expr: &'a Expr, calls: &mut Vec<(&'a str, Span)>) {
    match &expr.kind {
        ExprKind::Call { callee, arguments } => {
            if let ExprKind::Name(path) = &callee.kind
                && let [name] = path.segments.as_slice()
            {
                calls.push((name, callee.span));
            }
            for arg in arguments {
                calls_in_expr(&arg.value, calls);
            }
        }
        ExprKind::Binary { left, right, .. } => {
            calls_in_expr(left, calls);
            calls_in_expr(right, calls);
        }
        _ => {}
    }
}

fn rust_name(name: &str) -> String {
    if name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        format!("cgx2_user_{}_{}", name.len(), name)
    } else {
        let encoded = name.bytes().map(|b| format!("{b:02x}")).collect::<String>();
        format!("cgx2_user_hex_{encoded}")
    }
}

fn rust_type(ty: &Type) -> &'static str {
    match ty {
        Type::I64 => "i64",
        Type::Bool => "bool",
        Type::String => "String",
        Type::Unit => "()",
        Type::List(_) => unreachable!("validated scalar type"),
    }
}

fn tail_type(stmts: &[Stmt], typed: &TypedModule) -> Type {
    match stmts.last().map(|s| &s.kind) {
        Some(StmtKind::Expr(expr)) | Some(StmtKind::Return(Some(expr))) => typed
            .expressions
            .get(expr.span)
            .cloned()
            .unwrap_or(Type::Unit),
        _ => Type::Unit,
    }
}

fn emit_block(stmts: &[Stmt], indent: usize, out: &mut String) {
    let pad = "    ".repeat(indent);
    for (index, stmt) in stmts.iter().enumerate() {
        out.push_str(&format!("{pad}ctx.step()?;\n"));
        let last = index + 1 == stmts.len();
        match &stmt.kind {
            StmtKind::Let {
                mutable,
                name,
                value,
                ..
            } => {
                out.push_str(&format!(
                    "{pad}let {}{} = {}?;\n",
                    if *mutable { "mut " } else { "" },
                    rust_name(name),
                    emit_expr(value)
                ));
            }
            StmtKind::Assign { target, op, value } => {
                let name = rust_name(&target.segments[0]);
                match op {
                    AssignOp::Set => {
                        out.push_str(&format!("{pad}{name} = {}?;\n", emit_expr(value)))
                    }
                    AssignOp::Add => out.push_str(&format!(
                        "{pad}let cgx2_rhs = {}?;\n{pad}{name} = cgx2_add({name}, cgx2_rhs)?;\n",
                        emit_expr(value)
                    )),
                }
            }
            StmtKind::Return(Some(expr)) => {
                out.push_str(&format!("{pad}return Ok({}?);\n", emit_expr(expr)))
            }
            StmtKind::Return(None) => out.push_str(&format!("{pad}return Ok(());\n")),
            StmtKind::Expr(expr) if last => {
                out.push_str(&format!("{pad}return Ok({}?);\n", emit_expr(expr)))
            }
            StmtKind::Expr(expr) => out.push_str(&format!("{pad}let _ = {}?;\n", emit_expr(expr))),
            StmtKind::For { .. } | StmtKind::NoGc { .. } => unreachable!("validated source"),
        }
    }
    if !matches!(
        stmts.last().map(|s| &s.kind),
        Some(StmtKind::Expr(_) | StmtKind::Return(_))
    ) {
        out.push_str(&format!("{pad}Ok(())\n"));
    }
}

fn emit_expr(expr: &Expr) -> String {
    let value = match &expr.kind {
        ExprKind::Name(path) => {
            format!("Ok::<_, String>({}.clone())", rust_name(&path.segments[0]))
        }
        ExprKind::Integer(raw) => format!("Ok::<_, String>({}_i64)", raw.replace('_', "")),
        ExprKind::StringLiteral(raw) => format!(
            "Ok::<_, String>({}.to_owned())",
            quote_rust(&decode_string(raw).expect("validated string"))
        ),
        ExprKind::Boolean(value) => format!("Ok::<_, String>({value})"),
        ExprKind::Binary { left, op, right } => {
            let helper = match op {
                BinaryOp::Add => "cgx2_add",
                BinaryOp::Subtract => "cgx2_sub",
                BinaryOp::Multiply => "cgx2_mul",
                BinaryOp::Divide => "cgx2_div",
            };
            format!(
                "{{ let left = {}?; let right = {}?; {helper}(left, right) }}",
                emit_expr(left),
                emit_expr(right)
            )
        }
        ExprKind::Call { callee, arguments } => {
            let ExprKind::Name(path) = &callee.kind else {
                unreachable!("validated callee")
            };
            let mut prefix = String::new();
            for (index, arg) in arguments.iter().enumerate() {
                prefix.push_str(&format!(
                    "let cgx2_arg_{index} = {}?; ",
                    emit_expr(&arg.value)
                ));
            }
            let args = (0..arguments.len())
                .map(|i| format!("cgx2_arg_{i}"))
                .collect::<Vec<_>>()
                .join(", ");
            let call = match path.segments[0].as_str() {
                "arg_count" => "cgx2_arg_count(args)".to_owned(),
                "arg" => format!("cgx2_arg(args, {args})"),
                other => format!(
                    "{}(ctx, args{})",
                    rust_name(other),
                    if args.is_empty() {
                        String::new()
                    } else {
                        format!(", {args}")
                    }
                ),
            };
            format!("{{ {prefix}{call} }}")
        }
    };
    format!("{{ ctx.step()?; {value} }}")
}

fn decode_string(raw: &str) -> Option<String> {
    let content = raw.strip_prefix('"')?.strip_suffix('"')?;
    let mut chars = content.chars();
    let mut result = String::new();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            result.push(match chars.next()? {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                '"' => '"',
                _ => return None,
            });
        } else {
            result.push(ch);
        }
    }
    Some(result)
}

fn quote_rust(value: &str) -> String {
    let mut result = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            ch if ch.is_control() => result.push_str(&format!("\\u{{{:x}}}", ch as u32)),
            ch => result.push(ch),
        }
    }
    result.push('"');
    result
}
