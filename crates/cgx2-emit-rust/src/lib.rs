//! Readable, standalone Rust output for checked CGx2 scalar programs.

mod memory;

pub use memory::is_memory_script;

use cgx2_package::CheckedPackage;
use cgx2_sema::{Type, TypedModule, check_module};
use cgx2_syntax::{
    AssignOp, BinaryOp, Expr, ExprKind, Function, Module, Span, Stmt, StmtKind, TypeSyntax,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmitDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageEmitDiagnostic {
    pub file: PathBuf,
    pub diagnostic: EmitDiagnostic,
}

/// Emit one standalone Rust program from a checked, immutable local package snapshot.
pub fn emit_package_rust(package: &CheckedPackage) -> Result<String, Vec<PackageEmitDiagnostic>> {
    let mut errors = Vec::new();
    let by_name: BTreeMap<_, _> = package
        .modules()
        .iter()
        .filter_map(|source| {
            source
                .syntax()
                .name
                .as_ref()
                .map(|name| (name.segments.join("."), source))
        })
        .collect();
    let entry = package
        .modules()
        .iter()
        .find(|source| source.source() == package.index().entry);
    let Some(entry) = entry else {
        return Err(vec![PackageEmitDiagnostic {
            file: package.index().entry.clone(),
            diagnostic: EmitDiagnostic::new(
                "CGRS001",
                "package entry is not a checked module",
                Span { start: 0, end: 0 },
            ),
        }]);
    };
    let Some(entry_name) = entry.syntax().name.as_ref().map(|p| p.segments.join(".")) else {
        return Err(vec![PackageEmitDiagnostic {
            file: entry.source().to_path_buf(),
            diagnostic: EmitDiagnostic::new(
                "CGRS001",
                "package entry requires a module declaration",
                entry.syntax().span,
            ),
        }]);
    };
    for name in &package.index().topological_order {
        let Some(source) = by_name.get(name) else {
            continue;
        };
        let local: BTreeSet<_> = source
            .syntax()
            .functions
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        let allowed: BTreeSet<String> = source
            .syntax()
            .imports
            .iter()
            .filter_map(|path| {
                let imported = path.segments.join(".");
                by_name.get(&imported).map(|target| (imported, target))
            })
            .flat_map(|(imported, target)| {
                target
                    .syntax()
                    .functions
                    .iter()
                    .filter(|f| f.public)
                    .map(move |f| format!("{imported}.{}", f.name))
            })
            .collect();
        if !source.syntax().statements.is_empty() {
            errors.push(package_error(
                source,
                EmitDiagnostic::new(
                    "CGRS001",
                    "script-style package modules are unsupported",
                    source.syntax().statements[0].span,
                ),
            ));
        }
        for function in &source.syntax().functions {
            let mut local_errors = Vec::new();
            for parameter in &function.parameters {
                validate_type(&parameter.ty, &mut local_errors);
            }
            if let Some(ty) = &function.return_type {
                validate_type(ty, &mut local_errors);
            }
            validate_block(
                &function.body,
                false,
                &local,
                Some(&allowed),
                &mut local_errors,
            );
            validate_contracts(function, &mut local_errors);
            errors.extend(local_errors.into_iter().map(|d| package_error(source, d)));
        }
        for signature in source.typed().functions.values() {
            if signature.parameters.iter().any(|ty| !scalar_type(ty))
                || !scalar_type(&signature.return_type)
            {
                errors.push(package_error(
                    source,
                    EmitDiagnostic::new(
                        "CGRS001",
                        "function uses a non-scalar inferred type",
                        signature.span,
                    ),
                ));
            }
        }
        for (span, ty) in source.typed().expressions.iter() {
            if !scalar_type(ty) {
                errors.push(package_error(
                    source,
                    EmitDiagnostic::new("CGRS001", "expression uses a non-scalar type", span),
                ));
            }
        }
        let mut local_errors = Vec::new();
        let mut stack = Vec::new();
        let mut seen = BTreeSet::new();
        for function in &source.syntax().functions {
            find_cycle(
                &function.name,
                source.syntax(),
                &mut stack,
                &mut seen,
                &mut local_errors,
            );
        }
        errors.extend(local_errors.into_iter().map(|d| package_error(source, d)));
    }
    match entry.typed().functions.get("main") {
        Some(main) if !main.parameters.is_empty() => errors.push(package_error(
            entry,
            EmitDiagnostic::new(
                "CGRS001",
                "package main must have zero parameters",
                main.span,
            ),
        )),
        Some(main)
            if !matches!(
                main.return_type,
                Type::I64 | Type::Bool | Type::String | Type::Unit
            ) =>
        {
            errors.push(package_error(
                entry,
                EmitDiagnostic::new(
                    "CGRS001",
                    "package main return type is unsupported",
                    main.span,
                ),
            ))
        }
        None => errors.push(package_error(
            entry,
            EmitDiagnostic::new(
                "CGRS001",
                "package entry requires a main function",
                entry.syntax().span,
            ),
        )),
        _ => {}
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut out = String::from(PRELUDE);
    for name in &package.index().topological_order {
        let source = by_name[name];
        for function in &source.syntax().functions {
            let signature = &source.typed().functions[&function.name];
            let params = function
                .parameters
                .iter()
                .zip(&signature.parameters)
                .map(|(p, ty)| format!(", {}: {}", rust_name(&p.name), rust_type(ty)))
                .collect::<String>();
            out.push_str(&format!(
                "fn {}(ctx: &mut Cgx2Context, args: &[String]{params}) -> Result<{}, String> {{\n",
                rust_name(&format!("{name}.{}", function.name)),
                rust_type(&signature.return_type)
            ));
            out.push_str("    ctx.enter()?;\n    let result = (|| -> Result<_, String> {\n");
            emit_function_body(function, 2, &mut out, Some(name));
            out.push_str("    })();\n    ctx.leave();\n    result\n}\n\n");
        }
    }
    let result_type = &entry.typed().functions["main"].return_type;
    out.push_str("fn main() {\n    let args: Vec<String> = std::env::args().skip(1).collect();\n    let mut ctx = Cgx2Context::default();\n");
    out.push_str(&format!(
        "    match {}(&mut ctx, &args) {{\n",
        rust_name(&format!("{entry_name}.main"))
    ));
    if *result_type == Type::Unit {
        out.push_str("        Ok(()) => {},\n");
    } else {
        out.push_str("        Ok(value) => println!(\"{value}\"),\n");
    }
    out.push_str(
        "        Err(message) => { eprintln!(\"{message}\"); std::process::exit(1); }\n    }\n}\n",
    );
    Ok(out)
}

fn package_error(
    source: &cgx2_package::CheckedModule,
    diagnostic: EmitDiagnostic,
) -> PackageEmitDiagnostic {
    PackageEmitDiagnostic {
        file: source.source().to_path_buf(),
        diagnostic,
    }
}

fn scalar_type(ty: &Type) -> bool {
    matches!(ty, Type::I64 | Type::Bool | Type::String | Type::Unit)
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
    if memory::is_memory_script(module) {
        return memory::emit_memory_rust(module, checked);
    }
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
        validate_block(&function.body, false, &functions, None, &mut errors);
        validate_contracts(function, &mut errors);
    }
    validate_block(&module.statements, true, &functions, None, &mut errors);
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
        emit_function_body(function, 2, &mut out, None);
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

fn validate_contracts(function: &Function, errors: &mut Vec<EmitDiagnostic>) {
    for condition in function.requires.iter().chain(&function.ensures) {
        validate_contract_expr(condition, errors);
    }
}

fn comparison_operator(name: &str) -> Option<&'static str> {
    match name {
        "$cgx2_cmp_eq" => Some("=="),
        "$cgx2_cmp_ne" => Some("!="),
        "$cgx2_cmp_lt" => Some("<"),
        "$cgx2_cmp_le" => Some("<="),
        "$cgx2_cmp_gt" => Some(">"),
        "$cgx2_cmp_ge" => Some(">="),
        _ => None,
    }
}

fn validate_contract_expr(expr: &Expr, errors: &mut Vec<EmitDiagnostic>) {
    match &expr.kind {
        ExprKind::Name(path) if path.segments.len() == 1 => (),
        ExprKind::Integer(raw) if raw.replace('_', "").parse::<i64>().is_ok() => (),
        ExprKind::Boolean(_) => (),
        ExprKind::Binary { left, right, .. } => {
            validate_contract_expr(left, errors);
            validate_contract_expr(right, errors);
        }
        ExprKind::Call { callee, arguments }
            if matches!(&callee.kind, ExprKind::Name(path) if path.segments.len() == 1 && comparison_operator(&path.segments[0]).is_some())
                && arguments.len() == 2
                && arguments.iter().all(|arg| arg.name.is_none()) =>
        {
            for arg in arguments {
                validate_contract_expr(&arg.value, errors);
            }
        }
        _ => errors.push(EmitDiagnostic::new(
            "CGRS001",
            "contract expression is unsupported by Rust emission",
            expr.span,
        )),
    }
}

fn validate_block(
    stmts: &[Stmt],
    script: bool,
    functions: &std::collections::BTreeSet<&str>,
    allowed_imports: Option<&BTreeSet<String>>,
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
                validate_expr(value, functions, allowed_imports, errors);
            }
            StmtKind::Assign { target, value, .. } => {
                if target.segments.len() != 1 {
                    errors.push(EmitDiagnostic::new(
                        "CGRS001",
                        "qualified assignment is unsupported by Rust emission",
                        target.span,
                    ));
                }
                validate_expr(value, functions, allowed_imports, errors);
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
                    validate_expr(expr, functions, allowed_imports, errors);
                }
            }
            StmtKind::Expr(expr) => validate_expr(expr, functions, allowed_imports, errors),
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
    allowed_imports: Option<&BTreeSet<String>>,
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
            validate_expr(left, functions, allowed_imports, errors);
            validate_expr(right, functions, allowed_imports, errors);
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
            if !matches!(&callee.kind, ExprKind::Name(path) if path.segments.len() == 1 || allowed_imports.is_some_and(|allowed| allowed.contains(&path.segments.join("."))))
            {
                errors.push(EmitDiagnostic::new(
                    "CGRS001",
                    "computed or unlisted qualified calls are unsupported by Rust scalar emission",
                    callee.span,
                ));
            } else if let ExprKind::Name(path) = &callee.kind {
                let name = path.segments.join(".");
                if path.segments.len() == 1
                    && !matches!(name.as_str(), "arg" | "arg_count")
                    && comparison_operator(&name).is_none()
                    && !functions.contains(name.as_str())
                {
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
                validate_expr(&arg.value, functions, allowed_imports, errors);
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
    emit_block_context(stmts, indent, out, None);
}

fn emit_function_body(function: &Function, indent: usize, out: &mut String, module: Option<&str>) {
    if function.requires.is_empty() && function.ensures.is_empty() {
        emit_block_context(&function.body, indent, out, module);
        return;
    }
    let pad = "    ".repeat(indent);
    for condition in &function.requires {
        out.push_str(&format!(
            "{pad}if !({}?) {{ return Err(\"CGR015 at {}..{}: requires condition failed\".into()); }}\n",
            emit_expr_context(condition, module), condition.span.start, condition.span.end
        ));
    }
    out.push_str(&format!(
        "{pad}let cgx2_value = (|| -> Result<_, String> {{\n"
    ));
    emit_block_context(&function.body, indent + 1, out, module);
    out.push_str(&format!(
        "{pad}}})();\n{pad}let cgx2_value = cgx2_value?;\n"
    ));
    if !function.ensures.is_empty() {
        out.push_str(&format!(
            "{pad}let {} = cgx2_value.clone();\n",
            rust_name("result")
        ));
        for condition in &function.ensures {
            out.push_str(&format!(
                "{pad}if !({}?) {{ return Err(\"CGR016 at {}..{}: ensures condition failed\".into()); }}\n",
                emit_expr_context(condition, module), condition.span.start, condition.span.end
            ));
        }
    }
    out.push_str(&format!("{pad}Ok(cgx2_value)\n"));
}

fn emit_block_context(stmts: &[Stmt], indent: usize, out: &mut String, module: Option<&str>) {
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
                    emit_expr_context(value, module)
                ));
            }
            StmtKind::Assign { target, op, value } => {
                let name = rust_name(&target.segments[0]);
                match op {
                    AssignOp::Set => out.push_str(&format!(
                        "{pad}{name} = {}?;\n",
                        emit_expr_context(value, module)
                    )),
                    AssignOp::Add => out.push_str(&format!(
                        "{pad}let cgx2_rhs = {}?;\n{pad}{name} = cgx2_add({name}, cgx2_rhs)?;\n",
                        emit_expr_context(value, module)
                    )),
                }
            }
            StmtKind::Return(Some(expr)) => out.push_str(&format!(
                "{pad}return Ok({}?);\n",
                emit_expr_context(expr, module)
            )),
            StmtKind::Return(None) => out.push_str(&format!("{pad}return Ok(());\n")),
            StmtKind::Expr(expr) if last => out.push_str(&format!(
                "{pad}return Ok({}?);\n",
                emit_expr_context(expr, module)
            )),
            StmtKind::Expr(expr) => out.push_str(&format!(
                "{pad}let _ = {}?;\n",
                emit_expr_context(expr, module)
            )),
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

fn emit_expr_context(expr: &Expr, module: Option<&str>) -> String {
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
                emit_expr_context(left, module),
                emit_expr_context(right, module)
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
                    emit_expr_context(&arg.value, module)
                ));
            }
            let args = (0..arguments.len())
                .map(|i| format!("cgx2_arg_{i}"))
                .collect::<Vec<_>>()
                .join(", ");
            let call = match path.segments.as_slice() {
                [name] if name == "arg_count" => "cgx2_arg_count(args)".to_owned(),
                [name] if name == "arg" => format!("cgx2_arg(args, {args})"),
                [name] if comparison_operator(name).is_some() => {
                    let op = comparison_operator(name).expect("validated comparison");
                    format!("Ok::<_, String>(cgx2_arg_0 {op} cgx2_arg_1)")
                }
                _ => {
                    let full_name = if path.segments.len() > 1 {
                        path.segments.join(".")
                    } else if let Some(module) = module {
                        format!("{module}.{}", path.segments[0])
                    } else {
                        path.segments[0].clone()
                    };
                    format!(
                        "{}(ctx, args{})",
                        rust_name(&full_name),
                        if args.is_empty() {
                            String::new()
                        } else {
                            format!(", {args}")
                        }
                    )
                }
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
