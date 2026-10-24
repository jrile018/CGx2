//! Readable, standalone Rust output for checked CGx2 scalar programs.

mod data;
mod memory;
mod ordinary_owned;
mod variants;
pub use ordinary_owned::emit_owned_rust;
use variants::{emit_variants, syntax_type};

pub use memory::is_memory_script;

use cgx2_package::CheckedPackage;
use cgx2_sema::{Type, TypedModule, check_module};
use cgx2_syntax::{
    AssignOp, BinaryOp, Expr, ExprKind, Function, Module, Record, Span, Stmt, StmtKind, TypeSyntax,
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
    if package.modules().iter().any(|m| m.typed().finite_values) {
        return data::package(package);
    }
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
    let canonical: BTreeMap<_, _> = package
        .modules()
        .iter()
        .map(|source| {
            (
                source.source().to_path_buf(),
                cgx2_syntax::canonical_module(source.syntax(), &known_variants),
            )
        })
        .collect();
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
    let Some(entry_name) = canonical[entry.source()]
        .name
        .as_ref()
        .map(|p| p.segments.join("."))
    else {
        return Err(vec![PackageEmitDiagnostic {
            file: entry.source().to_path_buf(),
            diagnostic: EmitDiagnostic::new(
                "CGRS001",
                "package entry requires a module declaration",
                canonical[entry.source()].span,
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
            .chain(
                source
                    .syntax()
                    .records
                    .iter()
                    .map(|record| record.name.as_str()),
            )
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
                    .map(|f| f.name.as_str())
                    .chain(
                        target
                            .syntax()
                            .records
                            .iter()
                            .filter(|r| r.public)
                            .map(|r| r.name.as_str()),
                    )
                    .map(move |name| format!("{imported}.{name}"))
            })
            .collect();
        if !canonical[source.source()].statements.is_empty() {
            errors.push(package_error(
                source,
                EmitDiagnostic::new(
                    "CGRS001",
                    "script-style package modules are unsupported",
                    canonical[source.source()].statements[0].span,
                ),
            ));
        }
        for function in &canonical[source.source()].functions {
            let mut local_errors = Vec::new();
            if let Some(binding) = &function.foreign {
                local_errors.push(EmitDiagnostic::new(
                    "CGRS001",
                    "foreign declarations are unsupported by native Rust emission",
                    binding.target.span,
                ));
            }
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
                true,
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
        for function in &canonical[source.source()].functions {
            find_cycle(
                &function.name,
                &canonical[source.source()],
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
                Type::I64
                    | Type::Bool
                    | Type::String
                    | Type::Unit
                    | Type::Record(_)
                    | Type::Variant { .. }
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
                canonical[entry.source()].span,
            ),
        )),
        _ => {}
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut out = String::from(PRELUDE);
    out.push_str(include_str!("data_runtime.rs.txt"));
    emit_variants(
        &package
            .modules()
            .iter()
            .map(|source| (&canonical[source.source()], source.typed()))
            .collect::<Vec<_>>(),
        &mut out,
    );
    for source in package.modules() {
        let module = source
            .syntax()
            .name
            .as_ref()
            .expect("checked named module")
            .segments
            .join(".");
        // Use the same checked canonical references as variants and functions,
        // including nominal identities nested inside Option/Result arguments.
        let records = canonical[source.source()]
            .records
            .iter()
            .cloned()
            .map(|mut record| {
                record.name = format!("{module}.{}", record.name);
                record
            })
            .collect::<Vec<_>>();
        emit_records(&records, &mut out);
    }
    for name in &package.index().topological_order {
        let source = by_name[name];
        for function in &canonical[source.source()].functions {
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
            emit_function_body(function, 2, &mut out, Some(name), source.typed());
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
    matches!(
        ty,
        Type::I64 | Type::Bool | Type::String | Type::Unit | Type::Record(_) | Type::Variant { .. }
    )
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
    if let Some(span) = cgx2_syntax::ordinary_owned_span(module) {
        return Err(vec![EmitDiagnostic::new(
            "CGT080",
            "ordinary ownership requires emit_owned_rust with an authenticated source program",
            span,
        )]);
    }
    if checked.finite_values {
        return data::emit(module, checked);
    }
    if memory::is_memory_script(module) {
        return memory::emit_memory_rust(module, checked);
    }
    let original_module = module;
    let known = module
        .variants
        .iter()
        .map(|v| {
            module.name.as_ref().map_or_else(
                || v.name.clone(),
                |p| format!("{}.{}", p.segments.join("."), v.name),
            )
        })
        .collect();
    let normalized = cgx2_syntax::canonical_module(module, &known);
    let module = &normalized;
    let mut errors = Vec::new();
    let functions = module
        .functions
        .iter()
        .map(|f| f.name.as_str())
        .chain(module.records.iter().map(|record| record.name.as_str()))
        .collect::<std::collections::BTreeSet<_>>();
    if !module.functions.is_empty() && !module.statements.is_empty() {
        errors.push(EmitDiagnostic::new(
            "CGRS001",
            "function items and script statements cannot be mixed",
            module.statements[0].span,
        ));
    }
    for function in &module.functions {
        if let Some(binding) = &function.foreign {
            errors.push(EmitDiagnostic::new(
                "CGRS001",
                "foreign declarations are unsupported by native Rust emission",
                binding.target.span,
            ));
        }
        for parameter in &function.parameters {
            validate_type(&parameter.ty, &mut errors);
        }
        if let Some(ty) = &function.return_type {
            validate_type(ty, &mut errors);
        }
        validate_block(
            &function.body,
            false,
            &functions,
            None,
            !module.records.is_empty(),
            &mut errors,
        );
        validate_contracts(function, &mut errors);
    }
    validate_block(
        &module.statements,
        true,
        &functions,
        None,
        !module.records.is_empty(),
        &mut errors,
    );
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
    match check_module(original_module) {
        Ok(fresh) if fresh == *checked => {}
        _ => {
            return Err(vec![EmitDiagnostic::new(
                "CGRS002",
                "typed summary does not match a fresh check of the syntax tree",
                original_module.span,
            )]);
        }
    }
    let mut out = String::from(PRELUDE);
    out.push_str(include_str!("data_runtime.rs.txt"));
    emit_variants(&[(module, checked)], &mut out);
    let mut records = module.records.clone();
    if let Some(name) = &module.name {
        for record in &mut records {
            record.name = format!("{}.{}", name.segments.join("."), record.name);
        }
    }
    emit_records(&records, &mut out);
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
        emit_function_body(function, 2, &mut out, None, checked);
        out.push_str("    })();\n    ctx.leave();\n    result\n}\n\n");
    }
    if module.functions.is_empty() {
        let ty = tail_type(&module.statements, checked);
        out.push_str(&format!(
            "fn cgx2_script(ctx: &mut Cgx2Context, args: &[String]) -> Result<{}, String> {{\n",
            rust_type(&ty)
        ));
        emit_block(&module.statements, 1, &mut out, checked);
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

const PRELUDE: &str = r#"// Generated from CGx2's checked scalar and concrete finite value profiles.
// This file compiles with rustc --edition=2024 and has no external dependencies.
#[derive(Default)]
struct Cgx2Context { steps: usize, depth: usize }

impl Cgx2Context {
    fn step(&mut self, start: usize, end: usize) -> Result<(), String> {
        if self.steps >= 1_000_000 { return Err(format!("CGR010 at {start}..{end}: execution step limit of 1000000 exceeded")); }
        self.steps += 1;
        Ok(())
    }
    fn enter(&mut self) -> Result<(), String> {
        if self.depth >= 64 { return Err("CGR011: function call depth limit of 64 exceeded".into()); }
        self.depth += 1;
        Ok(())
    }
    fn leave(&mut self) { self.depth -= 1; }
}

fn cgx2_at(error: String, start: usize, end: usize) -> String {
    let code = if error == "division by zero" { "CGR006" } else { "CGR005" };
    format!("{code} at {start}..{end}: {error}")
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
        && !matches!(ty.path.segments.join(".").as_str(), "Option" | "Result")
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
        ExprKind::Constructor { .. } | ExprKind::Collection(_) | ExprKind::Index { .. } => errors
            .push(EmitDiagnostic::new(
                "CGRS001",
                "new aggregate expressions are unsupported in scalar contracts",
                expr.span,
            )),
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
    allow_fields: bool,
    errors: &mut Vec<EmitDiagnostic>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Match { scrutinee, arms } => {
                validate_expr(scrutinee, functions, allowed_imports, allow_fields, errors);
                for arm in arms {
                    validate_block(
                        &arm.body,
                        script,
                        functions,
                        allowed_imports,
                        allow_fields,
                        errors,
                    );
                }
            }
            StmtKind::LoopContract { .. } => errors.push(EmitDiagnostic::new(
                "CGRS001",
                "annotated loops require S2b/S3-V",
                stmt.span,
            )),
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                validate_expr(condition, functions, allowed_imports, allow_fields, errors);
                validate_block(
                    then_body,
                    script,
                    functions,
                    allowed_imports,
                    allow_fields,
                    errors,
                );
                validate_block(
                    else_body,
                    script,
                    functions,
                    allowed_imports,
                    allow_fields,
                    errors,
                );
            }
            StmtKind::Let {
                annotation, value, ..
            } => {
                if let Some(ty) = annotation {
                    validate_type(ty, errors);
                }
                validate_expr(value, functions, allowed_imports, allow_fields, errors);
            }
            StmtKind::Assign { target, value, .. } => {
                if target.segments.len() != 1 {
                    errors.push(EmitDiagnostic::new(
                        "CGRS001",
                        "qualified assignment is unsupported by Rust emission",
                        target.span,
                    ));
                }
                validate_expr(value, functions, allowed_imports, allow_fields, errors);
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
                    validate_expr(expr, functions, allowed_imports, allow_fields, errors);
                }
            }
            StmtKind::Expr(expr) => {
                validate_expr(expr, functions, allowed_imports, allow_fields, errors)
            }
            StmtKind::For { iter, body, .. } => {
                if let ExprKind::Call { callee, arguments } = &iter.kind
                    && matches!(&callee.kind, ExprKind::Name(path) if path.segments.as_slice() == ["range"])
                    && arguments.len() == 2
                    && arguments.iter().all(|arg| arg.name.is_none())
                {
                    for arg in arguments {
                        validate_expr(&arg.value, functions, allowed_imports, allow_fields, errors);
                    }
                    validate_block(
                        body,
                        script,
                        functions,
                        allowed_imports,
                        allow_fields,
                        errors,
                    );
                    continue;
                }
                errors.push(EmitDiagnostic::new(
                    "CGRS001",
                    "for iterator must be range(start, end)",
                    stmt.span,
                ));
            }
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
    allow_fields: bool,
    errors: &mut Vec<EmitDiagnostic>,
) {
    match &expr.kind {
        ExprKind::Constructor { arguments, .. } => {
            for arg in arguments {
                validate_expr(&arg.value, functions, allowed_imports, allow_fields, errors);
            }
        }
        ExprKind::Collection(_) | ExprKind::Index { .. } => errors.push(EmitDiagnostic::new(
            "CGRS001",
            "collections/indexing require S2b",
            expr.span,
        )),

        ExprKind::Name(path)
            if path.segments.len() > 2 || (path.segments.len() == 2 && !allow_fields) =>
        {
            errors.push(EmitDiagnostic::new(
                "CGRS001",
                "qualified names are unsupported by Rust scalar emission",
                path.span,
            ))
        }
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
            validate_expr(left, functions, allowed_imports, allow_fields, errors);
            validate_expr(right, functions, allowed_imports, allow_fields, errors);
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
                    && !(allow_fields && arguments.iter().any(|arg| arg.name.is_some()))
                {
                    errors.push(EmitDiagnostic::new(
                        "CGRS001",
                        format!("unknown or unsupported call `{name}` for Rust scalar emission"),
                        callee.span,
                    ));
                }
            }
            for arg in arguments {
                if arg.name.is_some() && !allow_fields {
                    errors.push(EmitDiagnostic::new(
                        "CGRS001",
                        "named calls are unsupported by Rust scalar emission",
                        arg.span,
                    ));
                }
                validate_expr(&arg.value, functions, allowed_imports, allow_fields, errors);
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
        StmtKind::Match { scrutinee, arms } => {
            calls_in_expr(scrutinee, calls);
            for arm in arms {
                for stmt in &arm.body {
                    calls_in_stmt(stmt, calls);
                }
            }
        }
        StmtKind::LoopContract { expression, .. } => calls_in_expr(expression, calls),
        StmtKind::If {
            condition,
            then_body,
            else_body,
        } => {
            calls_in_expr(condition, calls);
            for nested in then_body.iter().chain(else_body) {
                calls_in_stmt(nested, calls);
            }
        }
        StmtKind::For { iter, body, .. } => {
            calls_in_expr(iter, calls);
            for nested in body {
                calls_in_stmt(nested, calls);
            }
        }
        StmtKind::NoGc { budget, body } => {
            calls_in_expr(budget, calls);
            for nested in body {
                calls_in_stmt(nested, calls);
            }
        }
        StmtKind::Let { value, .. } | StmtKind::Assign { value, .. } | StmtKind::Expr(value) => {
            calls_in_expr(value, calls)
        }
        StmtKind::Return(Some(value)) => calls_in_expr(value, calls),
        StmtKind::Return(None) => {}
    }
}

fn calls_in_expr<'a>(expr: &'a Expr, calls: &mut Vec<(&'a str, Span)>) {
    match &expr.kind {
        ExprKind::Constructor { arguments, .. } => {
            for arg in arguments {
                calls_in_expr(&arg.value, calls);
            }
        }
        ExprKind::Collection(values) => {
            for value in values {
                calls_in_expr(value, calls);
            }
        }
        ExprKind::Index { collection, index } => {
            calls_in_expr(collection, calls);
            calls_in_expr(index, calls);
        }
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

fn rust_type(ty: &Type) -> String {
    match ty {
        Type::I64 => "i64".into(),
        Type::Bool => "bool".into(),
        Type::String => "String".into(),
        Type::Unit => "()".into(),
        Type::List(t) => format!("Cgx2List<{}>", rust_type(t)),
        Type::Array(t) => format!("Cgx2Array<{}>", rust_type(t)),
        Type::AppliedRecord { .. } | Type::Parameter { .. } => {
            unreachable!("specialized generic type")
        }
        Type::Record(name) => rust_name(name),
        Type::Variant { .. } => rust_name(&ty.to_string()),
    }
}

fn emit_records(records: &[Record], out: &mut String) {
    for record in records {
        let name = rust_name(&record.name);
        let derive = if record
            .fields
            .iter()
            .all(|f| matches!(syntax_type(&f.ty), Type::I64 | Type::Bool | Type::Unit))
        {
            "Clone, Copy, PartialEq, Eq"
        } else {
            "Clone, PartialEq, Eq"
        };
        out.push_str(&format!("#[derive({derive})]\nstruct {name} {{\n"));
        for field in &record.fields {
            let ty = rust_type(&syntax_type(&field.ty));
            out.push_str(&format!("    {}: {ty},\n", rust_name(&field.name)));
        }
        out.push_str("}\n");
        out.push_str(&format!("impl std::fmt::Display for {name} {{\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{\n        write!(f, \"{}(\")?;\n", record.name));
        for (index, field) in record.fields.iter().enumerate() {
            let comma = if index == 0 { "" } else { ", " };
            if syntax_type(&field.ty) == Type::Unit {
                out.push_str(&format!(
                    "        f.write_str({})?;\n",
                    quote_rust(&format!("{comma}{}: ", field.name))
                ));
                continue;
            }
            out.push_str(&format!(
                "        write!(f, \"{comma}{}: {{}}\", self.{})?;\n",
                field.name,
                rust_name(&field.name)
            ));
        }
        out.push_str("        write!(f, \")\")\n    }\n}\n\n");
        out.push_str(&format!("impl Cgx2Show for {name} {{ fn cg_show(&self)->String {{format!(\"{{}}\",self)}} fn cg_nodes(&self)->usize {{ 1usize"));
        for field in &record.fields {
            out.push_str(&format!(
                ".saturating_add(self.{}.cg_nodes())",
                rust_name(&field.name)
            ));
        }
        out.push_str(" } fn cg_bytes(&self)->usize { 0usize");
        for field in &record.fields {
            out.push_str(&format!(
                ".saturating_add(self.{}.cg_bytes())",
                rust_name(&field.name)
            ));
        }
        out.push_str(" } }\n");
    }
}

fn tail_type(stmts: &[Stmt], typed: &TypedModule) -> Type {
    match stmts.last().map(|s| &s.kind) {
        Some(StmtKind::Match { arms, .. }) => arms
            .first()
            .map_or(Type::Unit, |arm| tail_type(&arm.body, typed)),
        Some(StmtKind::Expr(expr)) | Some(StmtKind::Return(Some(expr))) => typed
            .expressions
            .get(expr.span)
            .cloned()
            .unwrap_or(Type::Unit),
        _ => Type::Unit,
    }
}

fn emit_block(stmts: &[Stmt], indent: usize, out: &mut String, typed: &TypedModule) {
    emit_block_context(stmts, indent, out, None, true, typed);
}

fn emit_function_body(
    function: &Function,
    indent: usize,
    out: &mut String,
    module: Option<&str>,
    typed: &TypedModule,
) {
    if function.requires.is_empty() && function.ensures.is_empty() {
        emit_block_context(&function.body, indent, out, module, true, typed);
        return;
    }
    let pad = "    ".repeat(indent);
    for condition in &function.requires {
        out.push_str(&format!(
            "{pad}if !({}?) {{ return Err(\"CGR015 at {}..{}: requires condition failed\".into()); }}\n",
            emit_expr_context(condition, module, typed), condition.span.start, condition.span.end
        ));
    }
    out.push_str(&format!(
        "{pad}let cgx2_value = (|| -> Result<_, String> {{\n"
    ));
    emit_block_context(&function.body, indent + 1, out, module, true, typed);
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
                emit_expr_context(condition, module, typed), condition.span.start, condition.span.end
            ));
        }
    }
    out.push_str(&format!("{pad}Ok(cgx2_value)\n"));
}

fn emit_block_context(
    stmts: &[Stmt],
    indent: usize,
    out: &mut String,
    module: Option<&str>,
    return_tail: bool,
    typed: &TypedModule,
) {
    let pad = "    ".repeat(indent);
    for (index, stmt) in stmts.iter().enumerate() {
        out.push_str(&format!(
            "{pad}ctx.step({}, {})?;\n",
            stmt.span.start, stmt.span.end
        ));
        let last = index + 1 == stmts.len();
        match &stmt.kind {
            StmtKind::Match { scrutinee, arms } => {
                out.push_str(&format!(
                    "{pad}match {}? {{\n",
                    emit_expr_context(scrutinee, module, typed)
                ));
                for arm in arms {
                    let pattern = match &arm.pattern {
                        cgx2_syntax::Pattern::Wildcard(_) => "_".to_owned(),
                        cgx2_syntax::Pattern::Case {
                            ty, case, bindings, ..
                        } => {
                            let payloads = if bindings.is_empty() {
                                String::new()
                            } else {
                                format!(
                                    "({})",
                                    bindings
                                        .iter()
                                        .map(|b| if b.name == "_" {
                                            "_".to_owned()
                                        } else {
                                            rust_name(&b.name)
                                        })
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                )
                            };
                            format!(
                                "{}::{}{payloads}",
                                rust_type(&syntax_type(ty)),
                                rust_name(case)
                            )
                        }
                    };
                    out.push_str(&format!("{pad}    {pattern} => {{\n"));
                    emit_block_context(
                        &arm.body,
                        indent + 2,
                        out,
                        module,
                        last && return_tail,
                        typed,
                    );
                    out.push_str(&format!("{pad}    }},\n"));
                }
                out.push_str(&format!("{pad}}}\n"));
            }
            StmtKind::LoopContract { .. } => unreachable!("validated loop contract rejection"),
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                out.push_str(&format!(
                    "{pad}if {}? {{\n",
                    emit_expr_context(condition, module, typed)
                ));
                emit_block_context(then_body, indent + 1, out, module, false, typed);
                out.push_str(&format!("{pad}}} else {{\n"));
                emit_block_context(else_body, indent + 1, out, module, false, typed);
                out.push_str(&format!("{pad}}}\n"));
            }
            StmtKind::Let {
                mutable,
                name,
                value,
                ..
            } => {
                out.push_str(&format!(
                    "{pad}let {}{}: {} = {}?;\n",
                    if *mutable { "mut " } else { "" },
                    rust_name(name),
                    rust_type(
                        typed
                            .expressions
                            .get(value.span)
                            .expect("checked binding type")
                    ),
                    emit_expr_context(value, module, typed)
                ));
            }
            StmtKind::Assign { target, op, value } => {
                let name = rust_name(&target.segments[0]);
                match op {
                    AssignOp::Set => out.push_str(&format!(
                        "{pad}{name} = {}?;\n",
                        emit_expr_context(value, module, typed)
                    )),
                    AssignOp::Add => out.push_str(&format!(
                        "{pad}let cgx2_rhs = {}?;\n{pad}{name} = cgx2_add({name}, cgx2_rhs).map_err(|e| cgx2_at(e, {}, {}))?;\n",
                        emit_expr_context(value, module, typed), stmt.span.start, stmt.span.end
                    )),
                }
            }
            StmtKind::Return(Some(expr)) => out.push_str(&format!(
                "{pad}return Ok({}?);\n",
                emit_expr_context(expr, module, typed)
            )),
            StmtKind::Return(None) => out.push_str(&format!("{pad}return Ok(());\n")),
            StmtKind::Expr(expr) if last && return_tail => out.push_str(&format!(
                "{pad}return Ok({}?);\n",
                emit_expr_context(expr, module, typed)
            )),
            StmtKind::Expr(expr) => out.push_str(&format!(
                "{pad}let _ = {}?;\n",
                emit_expr_context(expr, module, typed)
            )),
            StmtKind::For {
                binding,
                iter,
                body,
            } => {
                if !matches!(&iter.kind, ExprKind::Call {callee,..} if matches!(&callee.kind,ExprKind::Name(p) if p.segments.as_slice()==["range"]))
                {
                    out.push_str(&format!(
                        "{pad}for {} in cgx2_items({}?) {{\n{pad}    ctx.step({}, {})?;\n",
                        rust_name(binding),
                        emit_expr_context(iter, module, typed),
                        stmt.span.start,
                        stmt.span.end
                    ));
                    emit_block_context(body, indent + 1, out, module, false, typed);
                    out.push_str(&format!("{pad}}}\n"));
                    continue;
                }
                let ExprKind::Call { arguments, .. } = &iter.kind else {
                    unreachable!("checked range")
                };
                out.push_str(&format!(
                    "{pad}{{\n{pad}    ctx.step({}, {})?;\n",
                    iter.span.start, iter.span.end
                ));
                out.push_str(&format!(
                    "{pad}    let cgx2_range_start = {}?;\n",
                    emit_expr_context(&arguments[0].value, module, typed)
                ));
                out.push_str(&format!(
                    "{pad}    let cgx2_range_end = {}?;\n",
                    emit_expr_context(&arguments[1].value, module, typed)
                ));
                out.push_str(&format!("{pad}    for cgx2_range_index in cgx2_range_start..cgx2_range_end {{\n{pad}        ctx.step({}, {})?;\n", stmt.span.start, stmt.span.end));
                out.push_str(&format!(
                    "{pad}        let {} = cgx2_range_index;\n",
                    rust_name(binding)
                ));
                out.push_str(&format!(
                    "{pad}        let _ = (|| -> Result<_, String> {{\n"
                ));
                emit_block_context(body, indent + 3, out, module, true, typed);
                out.push_str(&format!("{pad}        }})()?;\n{pad}    }}\n{pad}}}\n"));
            }
            StmtKind::NoGc { .. } => unreachable!("validated source"),
        }
        if block_returns(std::slice::from_ref(stmt)) {
            break;
        }
    }
    if return_tail
        && !block_returns(stmts)
        && !matches!(
            stmts.last().map(|s| &s.kind),
            Some(StmtKind::Expr(_) | StmtKind::Return(_) | StmtKind::Match { .. })
        )
    {
        out.push_str(&format!("{pad}Ok(())\n"));
    }
}

fn emit_expr_context(expr: &Expr, module: Option<&str>, typed: &TypedModule) -> String {
    if let Some(value) = data::expression(expr, module, typed) {
        return data::account(expr, value, typed.finite_values);
    }
    let value = match &expr.kind {
        ExprKind::Constructor {
            ty,
            case,
            arguments,
        } => variants::constructor(ty, case, arguments, module, typed),
        ExprKind::Collection(_) | ExprKind::Index { .. } => {
            unreachable!("validated collection rejection")
        }
        ExprKind::Name(path) => match path.segments.as_slice() {
            [name] => format!("Ok::<_, String>({}.clone())", rust_name(name)),
            [base, field] => format!(
                "Ok::<_, String>({}.{}.clone())",
                rust_name(base),
                rust_name(field)
            ),
            _ => unreachable!("validated field path"),
        },
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
                "{{ let left = {}?; let right = {}?; {helper}(left, right).map_err(|e| cgx2_at(e, {}, {})) }}",
                emit_expr_context(left, module, typed),
                emit_expr_context(right, module, typed),
                expr.span.start,
                expr.span.end
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
                    emit_expr_context(&arg.value, module, typed)
                ));
            }
            let args = (0..arguments.len())
                .map(|i| format!("cgx2_arg_{i}"))
                .collect::<Vec<_>>()
                .join(", ");
            let call = if arguments.iter().any(|arg| arg.name.is_some()) {
                let fields = arguments
                    .iter()
                    .enumerate()
                    .map(|(index, arg)| {
                        format!(
                            "{}: cgx2_arg_{index}",
                            rust_name(arg.name.as_deref().expect("checked record field"))
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "Ok::<_, String>({} {{ {fields} }})",
                    rust_name(
                        &if path.segments.len() > 1 || path.segments[0].contains('.') {
                            path.segments.join(".")
                        } else if let Some(module) = module {
                            format!("{module}.{}", path.segments[0])
                        } else {
                            path.segments[0].clone()
                        }
                    )
                )
            } else {
                match path.segments.as_slice() {
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
                }
            };
            format!("{{ {prefix}{call} }}")
        }
    };
    data::account(expr, value, typed.finite_values)
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

fn block_returns(body: &[Stmt]) -> bool {
    body.iter().any(|stmt| match &stmt.kind {
        StmtKind::Return(_) => true,
        StmtKind::Match { arms, .. } => {
            !arms.is_empty() && arms.iter().all(|arm| block_returns(&arm.body))
        }
        StmtKind::If {
            then_body,
            else_body,
            ..
        } => block_returns(then_body) && block_returns(else_body),
        _ => false,
    })
}
