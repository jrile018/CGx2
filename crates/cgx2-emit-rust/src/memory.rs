//! Native lowering for the checked, literal-size memory script profile.
use cgx2_sema::{TypedModule, check_module};
use cgx2_syntax::{CallArg, Expr, ExprKind, Module, Span, Stmt, StmtKind};
use std::collections::BTreeMap;

use crate::{EmitDiagnostic, quote_rust};

pub fn is_memory_script(module: &Module) -> bool {
    fn has_memory(stmt: &Stmt) -> bool {
        match &stmt.kind {
            StmtKind::NoGc { .. } => true,
            StmtKind::Let { value, .. }
            | StmtKind::Assign { value, .. }
            | StmtKind::Expr(value) => {
                matches!(&value.kind, ExprKind::Call { callee, .. } if matches!(&callee.kind, ExprKind::Name(path) if path.segments.first().is_some_and(|s| s == "memory")))
            }
            _ => false,
        }
    }
    module.statements.iter().any(has_memory)
        || module
            .functions
            .iter()
            .any(|function| function.body.iter().any(has_memory))
}

fn unsupported(message: impl Into<String>, span: Span) -> EmitDiagnostic {
    EmitDiagnostic::new("CGRS001", message, span)
}

fn size(expr: &Expr) -> Result<usize, EmitDiagnostic> {
    let ExprKind::Integer(raw) = &expr.kind else {
        return Err(unsupported(
            "memory size must be a literal byte count",
            expr.span,
        ));
    };
    raw.replace('_', "")
        .parse::<usize>()
        .map_err(|_| unsupported("memory size is outside usize range", expr.span))
}

fn name(expr: &Expr) -> Result<&str, EmitDiagnostic> {
    match &expr.kind {
        ExprKind::Name(path) if path.segments.len() == 1 => Ok(&path.segments[0]),
        _ => Err(unsupported(
            "memory operand must be a binding name",
            expr.span,
        )),
    }
}

fn positional(args: &[CallArg], count: usize, span: Span) -> Result<Vec<&Expr>, EmitDiagnostic> {
    if args.len() != count || args.iter().any(|arg| arg.name.is_some()) {
        return Err(unsupported("invalid memory intrinsic arguments", span));
    }
    Ok(args.iter().map(|arg| &arg.value).collect())
}

fn call(expr: &Expr) -> Result<(&str, &[CallArg]), EmitDiagnostic> {
    let ExprKind::Call { callee, arguments } = &expr.kind else {
        return Err(unsupported(
            "memory script requires direct memory calls",
            expr.span,
        ));
    };
    let ExprKind::Name(path) = &callee.kind else {
        return Err(unsupported(
            "computed memory call is unsupported",
            callee.span,
        ));
    };
    let [namespace, operation] = path.segments.as_slice() else {
        return Err(unsupported("memory call must use memory.*", path.span));
    };
    if namespace != "memory" {
        return Err(unsupported(
            "mixed scalar and memory scripts are unsupported",
            path.span,
        ));
    }
    Ok((operation, arguments))
}

fn emit_block(
    stmts: &[Stmt],
    out: &mut String,
    indent: usize,
    functions: &BTreeMap<&str, usize>,
) -> Result<(), EmitDiagnostic> {
    let pad = "    ".repeat(indent);
    for stmt in stmts {
        let span = format!("({}, {})", stmt.span.start, stmt.span.end);
        out.push_str(&format!("{pad}step(steps, {span})?;\n"));
        match &stmt.kind {
            StmtKind::NoGc { budget, body } => {
                let budget = size(budget)?;
                out.push_str(&format!("{pad}{{\n{pad}    let mut cgx2_guard = session.no_gc({budget});\n{pad}    let session = &mut *cgx2_guard;\n"));
                emit_block(body, out, indent + 1, functions)?;
                out.push_str(&format!("{pad}}}\n"));
            }
            StmtKind::Let {
                mutable: false,
                name: dest,
                annotation: None,
                value,
            } => {
                let (op, args) = call(value)?;
                let dest = quote_rust(dest);
                let statement = match op {
                    "region" => {
                        let mode = args.iter().find(|arg| arg.name.as_deref() == Some("mode"));
                        let limit = args.iter().find(|arg| arg.name.as_deref() == Some("limit"));
                        if args.len() != 2 || mode.is_none() || limit.is_none() {
                            return Err(unsupported(
                                "memory.region requires mode and limit",
                                value.span,
                            ));
                        }
                        let mode = match name(&mode.expect("checked").value)? {
                            "manual" => "CollectionMode::Manual",
                            "auto" => "CollectionMode::Auto { trigger_bytes: 1 << 20 }",
                            _ => {
                                return Err(unsupported(
                                    "memory mode must be auto or manual",
                                    value.span,
                                ));
                            }
                        };
                        format!(
                            "open(session, &mut bindings, {dest}, {mode}, {}, {span})?;",
                            size(&limit.expect("checked").value)?
                        )
                    }
                    "owned_bytes" => format!(
                        "owned(session, &mut bindings, {dest}, {}, {span})?;",
                        size(positional(args, 1, value.span)?[0])?
                    ),
                    "move" => format!(
                        "move_owned(&mut bindings, {dest}, {}, {span})?;",
                        quote_rust(name(positional(args, 1, value.span)?[0])?)
                    ),
                    "promote_owned" => {
                        let args = positional(args, 2, value.span)?;
                        format!(
                            "promote(session, &mut bindings, {dest}, {}, {}, {span})?;",
                            quote_rust(name(args[0])?),
                            quote_rust(name(args[1])?)
                        )
                    }
                    "copy_to_owned" => format!(
                        "copy_owned(session, &mut bindings, {dest}, {}, {span})?;",
                        quote_rust(name(positional(args, 1, value.span)?[0])?)
                    ),
                    "alloc_bytes" => {
                        let args = positional(args, 2, value.span)?;
                        format!(
                            "alloc(session, &mut bindings, {dest}, {}, {}, {span})?;",
                            quote_rust(name(args[0])?),
                            size(args[1])?
                        )
                    }
                    "borrow" | "borrow_mut" => format!(
                        "borrow(&mut bindings, {dest}, {}, {span})?;",
                        quote_rust(name(positional(args, 1, value.span)?[0])?)
                    ),
                    _ => {
                        return Err(unsupported(
                            format!("unsupported memory operation `{op}`"),
                            value.span,
                        ));
                    }
                };
                out.push_str(&format!("{pad}{statement}\n"));
            }
            StmtKind::Expr(expr) => {
                if let ExprKind::Call { callee, arguments } = &expr.kind
                    && let ExprKind::Name(path) = &callee.kind
                    && let [local] = path.segments.as_slice()
                {
                    if !arguments.is_empty() {
                        return Err(unsupported(
                            "local memory calls take no arguments",
                            expr.span,
                        ));
                    }
                    let Some(index) = functions.get(local.as_str()) else {
                        return Err(unsupported(
                            format!("unknown local memory function `{local}`"),
                            callee.span,
                        ));
                    };
                    out.push_str(&format!(
                        "{pad}cgx2_memory_function_{index}(session, steps, {span})?;\n"
                    ));
                    continue;
                }
                let statement = if let ExprKind::Name(_) = &expr.kind {
                    format!(
                        "use_value(session, &bindings, {}, {span})?;",
                        quote_rust(name(expr)?)
                    )
                } else {
                    let (op, args) = call(expr)?;
                    let args = positional(args, 1, expr.span)?;
                    let operand = quote_rust(name(args[0])?);
                    match op {
                        "use" => format!("use_value(session, &bindings, {operand}, {span})?;"),
                        "drop" => {
                            format!("drop_value(session, &mut bindings, {operand}, {span})?;")
                        }
                        "end_borrow" => format!("end_borrow(&mut bindings, {operand}, {span})?;"),
                        "collect" => format!("collect(session, &bindings, {operand}, {span})?;"),
                        "close" => format!("close(session, &bindings, {operand}, {span})?;"),
                        _ => {
                            return Err(unsupported(
                                format!("unsupported memory operation `{op}`"),
                                expr.span,
                            ));
                        }
                    }
                };
                out.push_str(&format!("{pad}{statement}\n"));
            }
            _ => return Err(unsupported("unsupported memory script form", stmt.span)),
        }
    }
    Ok(())
}

pub(crate) fn emit_memory_rust(
    module: &Module,
    checked: &TypedModule,
) -> Result<String, Vec<EmitDiagnostic>> {
    if !module.imports.is_empty() {
        return Err(vec![unsupported(
            "native memory functions cannot contain imports",
            module.span,
        )]);
    }
    match check_module(module) {
        Ok(fresh) if fresh == *checked => (),
        _ => {
            return Err(vec![EmitDiagnostic::new(
                "CGRS002",
                "typed summary does not match a fresh check of the memory script",
                module.span,
            )]);
        }
    }
    let functions: BTreeMap<&str, usize> = module
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.name.as_str(), index))
        .collect();
    let mut out = String::from("// Generated CGx2 native memory program. Requires cgx2-runtime.\n");
    out.push_str(include_str!("memory_runtime.rs.txt"));
    out.push_str(include_str!("memory_report.rs.txt"));
    for (index, function) in module.functions.iter().enumerate() {
        out.push_str(&format!(
            "\nfn cgx2_memory_function_{index}(session: &mut Session, steps: &mut usize, call_span: (usize, usize)) -> Result<(), String> {{\n    let mut bindings: HashMap<&'static str, Binding> = HashMap::new();\n    let result = (|| -> Result<(), String> {{\n"
        ));
        emit_block(&function.body, &mut out, 2, &functions).map_err(|error| vec![error])?;
        out.push_str("        Ok(())\n    })();\n    let mut cleanup_error = None;\n    for value in bindings.into_values() {\n        if let Binding::Managed(handle) = value {\n            if let Err(failure) = session.unroot(handle) {\n                cleanup_error.get_or_insert_with(|| runtime(failure, call_span));\n            }\n        }\n    }\n    result?;\n    if let Some(failure) = cleanup_error { return Err(failure); }\n    Ok(())\n}\n");
    }
    out.push_str("\nfn main() {\n    let report_path = match cgx2_report_path() {\n        Ok(path) => path,\n        Err(message) => { eprintln!(\"{message}\"); std::process::exit(1); }\n    };\n    let mut session = Session::new();\n    let mut bindings: HashMap<&'static str, Binding> = HashMap::new();\n    let mut steps = 0;\n    let result = (|| -> Result<(), String> {\n        let session = &mut session;\n        let steps = &mut steps;\n");
    if module.functions.is_empty() {
        emit_block(&module.statements, &mut out, 2, &functions).map_err(|error| vec![error])?;
    } else {
        let main = module
            .functions
            .iter()
            .find(|function| function.name == "main")
            .ok_or_else(|| {
                vec![unsupported(
                    "memory function module requires main()",
                    module.span,
                )]
            })?;
        let index = functions["main"];
        let span = format!("({}, {})", main.span.start, main.span.end);
        out.push_str(&format!(
            "        step(steps, {span})?;\n        cgx2_memory_function_{index}(session, steps, {span})?;\n"
        ));
    }
    out.push_str("        Ok(())\n    })();\n    if let Err(message) = &result { eprintln!(\"{message}\"); }\n    if let Some(path) = report_path {\n        if let Err(message) = cgx2_write_report(&path, &session, &result) {\n            eprintln!(\"{message}\");\n            std::process::exit(1);\n        }\n    }\n    if result.is_err() { std::process::exit(1); }\n}\n");
    Ok(out)
}
