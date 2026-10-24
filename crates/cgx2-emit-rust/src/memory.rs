//! Native lowering for the checked, literal-size memory script profile.
use cgx2_sema::{TypedModule, check_module};
use cgx2_syntax::{CallArg, Expr, ExprKind, Module, Span, Stmt, StmtKind};
use std::collections::BTreeMap;

use crate::{EmitDiagnostic, quote_rust};

pub fn is_memory_script(module: &Module) -> bool {
    fn expr(e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Constructor { arguments, .. } => arguments.iter().any(|arg| expr(&arg.value)),
            ExprKind::Collection(values) => values.iter().any(expr),
            ExprKind::Index { collection, index } => expr(collection) || expr(index),
            ExprKind::Name(path) => path.segments.first().is_some_and(|n| n == "memory"),
            ExprKind::Call { callee, arguments } => {
                expr(callee) || arguments.iter().any(|a| expr(&a.value))
            }
            ExprKind::Binary { left, right, .. } => expr(left) || expr(right),
            _ => false,
        }
    }
    fn has_memory(stmt: &Stmt) -> bool {
        match &stmt.kind {
            StmtKind::Match { scrutinee, arms } => {
                expr(scrutinee) || arms.iter().any(|arm| arm.body.iter().any(has_memory))
            }
            StmtKind::LoopContract { expression, .. } => expr(expression),
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => expr(condition) || then_body.iter().chain(else_body).any(has_memory),
            StmtKind::NoGc { .. } => true,
            StmtKind::Let { value, .. }
            | StmtKind::Assign { value, .. }
            | StmtKind::Expr(value)
            | StmtKind::Return(Some(value)) => expr(value),
            StmtKind::For { iter, body, .. } => expr(iter) || body.iter().any(has_memory),
            StmtKind::Return(None) => false,
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
fn scalar_expr(expr: &Expr, parameters: &BTreeMap<&str, usize>) -> Result<String, EmitDiagnostic> {
    match &expr.kind {
        ExprKind::Integer(raw) => raw
            .replace('_', "")
            .parse::<i64>()
            .map(|value| format!("{value}i64"))
            .map_err(|_| unsupported("i64 scalar out of range", expr.span)),
        ExprKind::Name(path) if path.segments.len() == 1 => parameters
            .get(path.segments[0].as_str())
            .map(|index| format!("cgx2_param_{index}"))
            .ok_or_else(|| unsupported("unknown scalar parameter", expr.span)),
        ExprKind::Binary { left, op, right } => {
            let left = scalar_expr(left, parameters)?;
            let right = scalar_expr(right, parameters)?;
            let method = match op {
                cgx2_syntax::BinaryOp::Add => "checked_add",
                cgx2_syntax::BinaryOp::Subtract => "checked_sub",
                cgx2_syntax::BinaryOp::Multiply => "checked_mul",
                cgx2_syntax::BinaryOp::Divide => "checked_div",
            };
            let message = quote_rust(&format!(
                "CGR013 at {}..{}: i64 scalar arithmetic failed",
                expr.span.start, expr.span.end
            ));
            Ok(format!(
                "({left}).{method}({right}).ok_or_else(|| String::from({message}))?"
            ))
        }
        _ => Err(unsupported(
            "memory call argument must be i64 scalar",
            expr.span,
        )),
    }
}
fn guard_expr(expr: &Expr, parameters: &BTreeMap<&str, usize>) -> Result<String, EmitDiagnostic> {
    if let ExprKind::Boolean(value) = &expr.kind {
        return Ok(value.to_string());
    }
    let ExprKind::Call { callee, arguments } = &expr.kind else {
        return Err(unsupported("invalid memory guard", expr.span));
    };
    let ExprKind::Name(path) = &callee.kind else {
        return Err(unsupported("invalid memory guard", expr.span));
    };
    let op = match path.segments.as_slice() {
        [name] => match name.as_str() {
            "$cgx2_cmp_eq" => "==",
            "$cgx2_cmp_ne" => "!=",
            "$cgx2_cmp_lt" => "<",
            "$cgx2_cmp_le" => "<=",
            "$cgx2_cmp_gt" => ">",
            "$cgx2_cmp_ge" => ">=",
            _ => return Err(unsupported("invalid memory comparison", expr.span)),
        },
        _ => return Err(unsupported("invalid memory comparison", expr.span)),
    };
    let args = positional(arguments, 2, expr.span)?;
    Ok(format!(
        "({} {op} {})",
        scalar_expr(args[0], parameters)?,
        scalar_expr(args[1], parameters)?
    ))
}

fn bound_name<'a>(
    expr: &'a Expr,
    names: &'a BTreeMap<String, String>,
) -> Result<&'a str, EmitDiagnostic> {
    let source = name(expr)?;
    Ok(names.get(source).map_or(source, String::as_str))
}

fn size_expr(expr: &Expr, parameters: &BTreeMap<&str, usize>) -> Result<String, EmitDiagnostic> {
    if matches!(&expr.kind, ExprKind::Name(_)) {
        let scalar = scalar_expr(expr, parameters)?;
        let message = quote_rust(&format!(
            "CGR013 at {}..{}: byte size is negative or outside target usize range",
            expr.span.start, expr.span.end
        ));
        Ok(format!(
            "usize::try_from({scalar}).map_err(|_| String::from({message}))?"
        ))
    } else {
        Ok(size(expr)?.to_string())
    }
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

fn emit_checked_statement(out: &mut String, pad: &str, statement: &str) {
    out.push_str(&format!(
        "{pad}let cgx2_before = graph_snapshot(session, &bindings);\n{pad}let cgx2_step = (|| -> Result<(), String> {{ {statement} Ok(()) }})();\n{pad}if let Err(message) = cgx2_step {{ record_rollback(cgx2_before, session, &bindings); return Err(message); }}\n"
    ));
}

fn emit_block(
    stmts: &[Stmt],
    out: &mut String,
    indent: usize,
    functions: &BTreeMap<&str, (usize, usize)>,
    parameters: &BTreeMap<&str, usize>,
    names: &mut BTreeMap<String, String>,
    in_branch: bool,
) -> Result<(), EmitDiagnostic> {
    let pad = "    ".repeat(indent);
    for stmt in stmts {
        let span = format!("({}, {})", stmt.span.start, stmt.span.end);
        out.push_str(&format!("{pad}if let Err(message) = step(steps, {span}) {{ record_rollback(graph_snapshot(session, &bindings), session, &bindings); return Err(message); }}\n"));
        match &stmt.kind {
            StmtKind::For {
                binding,
                iter,
                body,
            } => {
                let ExprKind::Call { arguments, .. } = &iter.kind else {
                    return Err(unsupported("expected range", iter.span));
                };
                if arguments.len() != 2 {
                    return Err(unsupported("expected two endpoints", iter.span));
                }
                let start = scalar_expr(&arguments[0].value, parameters)?;
                let end = scalar_expr(&arguments[1].value, parameters)?;
                let index = parameters.len();
                let mut scoped = parameters.clone();
                scoped.insert(binding.as_str(), index);
                out.push_str(&format!("{pad}{{\n{pad}    let cgx2_start = {start};\n{pad}    let cgx2_end = {end};\n{pad}    for cgx2_param_{index} in cgx2_start..cgx2_end {{\n{pad}        if let Err(message) = enter_iteration(iterations, {span}) {{ record_rollback(graph_snapshot(session, &bindings), session, &bindings); return Err(message); }}\n{pad}        let cgx2_outer: HashSet<_> = bindings.keys().copied().collect();\n{pad}        let cgx2_iteration_result = (|| -> Result<(), String> {{\n"));
                emit_block(
                    body,
                    out,
                    indent + 3,
                    functions,
                    &scoped,
                    &mut names.clone(),
                    true,
                )?;
                out.push_str(&format!("{pad}            Ok(())\n{pad}        }})();\n{pad}        let cgx2_cleanup_error = cleanup_scope(session, &mut bindings, &cgx2_outer, {span});\n{pad}        cgx2_iteration_result?;\n{pad}        if let Some(failure) = cgx2_cleanup_error {{ return Err(failure); }}\n{pad}    }}\n{pad}}}\n"));
            }
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                let guard = guard_expr(condition, parameters)?;
                out.push_str(&format!("{pad}{{\n{pad}    let cgx2_condition = {guard};\n{pad}    let cgx2_outer: HashSet<_> = bindings.keys().copied().collect();\n{pad}    let cgx2_branch_result = (|| -> Result<(), String> {{\n{pad}        if cgx2_condition {{\n"));
                emit_block(
                    then_body,
                    out,
                    indent + 3,
                    functions,
                    parameters,
                    &mut names.clone(),
                    true,
                )?;
                out.push_str(&format!("{pad}        }} else {{\n"));
                emit_block(
                    else_body,
                    out,
                    indent + 3,
                    functions,
                    parameters,
                    &mut names.clone(),
                    true,
                )?;
                out.push_str(&format!("{pad}        }}\n{pad}        Ok(())\n{pad}    }})();\n{pad}    let cgx2_cleanup_error = cleanup_scope(session, &mut bindings, &cgx2_outer, {span});\n{pad}    cgx2_branch_result?;\n{pad}    if let Some(failure) = cgx2_cleanup_error {{ return Err(failure); }}\n{pad}}}\n"));
            }
            StmtKind::NoGc { budget, body } => {
                let budget = size(budget)?;
                out.push_str(&format!("{pad}{{\n{pad}    let mut cgx2_guard = session.no_gc({budget});\n{pad}    let session = &mut *cgx2_guard;\n"));
                emit_block(
                    body,
                    out,
                    indent + 1,
                    functions,
                    parameters,
                    names,
                    in_branch,
                )?;
                out.push_str(&format!("{pad}}}\n"));
            }
            StmtKind::Let {
                mutable: false,
                name: dest,
                annotation: None,
                value,
            } => {
                let (op, args) = call(value)?;
                let internal = if in_branch {
                    format!("$arm{}:{dest}", stmt.span.start)
                } else {
                    dest.clone()
                };
                names.insert(dest.clone(), internal.clone());
                let dest = quote_rust(&internal);
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
                    "owned_bytes" => {
                        let size_arg = positional(args, 1, value.span)?[0];
                        let failure_span = if matches!(&size_arg.kind, ExprKind::Name(_)) {
                            format!("({}, {})", size_arg.span.start, size_arg.span.end)
                        } else {
                            span.clone()
                        };
                        format!(
                            "owned(session, &mut bindings, {dest}, {}, {failure_span})?;",
                            size_expr(size_arg, parameters)?
                        )
                    }
                    "move" => format!(
                        "move_owned(&mut bindings, {dest}, {}, {span})?;",
                        quote_rust(bound_name(positional(args, 1, value.span)?[0], names)?)
                    ),
                    "promote_owned" => {
                        let args = positional(args, 2, value.span)?;
                        format!(
                            "promote(session, &mut bindings, {dest}, {}, {}, {span})?;",
                            quote_rust(bound_name(args[0], names)?),
                            quote_rust(bound_name(args[1], names)?)
                        )
                    }
                    "copy_to_owned" => format!(
                        "copy_owned(session, &mut bindings, {dest}, {}, {span})?;",
                        quote_rust(bound_name(positional(args, 1, value.span)?[0], names)?)
                    ),
                    "alloc_bytes" => {
                        let args = positional(args, 2, value.span)?;
                        let failure_span = if matches!(&args[1].kind, ExprKind::Name(_)) {
                            format!("({}, {})", args[1].span.start, args[1].span.end)
                        } else {
                            span.clone()
                        };
                        format!(
                            "alloc(session, &mut bindings, {dest}, {}, {}, {failure_span})?;",
                            quote_rust(bound_name(args[0], names)?),
                            size_expr(args[1], parameters)?
                        )
                    }
                    "alloc_object" | "alloc_graph" => {
                        let args = positional(args, 3, value.span)?;
                        format!(
                            "{op}(session, &mut bindings, {dest}, {}, {}, {}, {span})?;",
                            quote_rust(bound_name(args[0], names)?),
                            size(args[1])?,
                            size(args[2])?
                        )
                    }
                    "reference" | "graph_reference" => {
                        let args = positional(args, 2, value.span)?;
                        format!(
                            "{op}(session, &mut bindings, {dest}, {}, {}, {span})?;",
                            quote_rust(bound_name(args[0], names)?),
                            size(args[1])?
                        )
                    }
                    "borrow" | "borrow_mut" => format!(
                        "borrow(&mut bindings, {dest}, {}, {span})?;",
                        quote_rust(bound_name(positional(args, 1, value.span)?[0], names)?)
                    ),
                    _ => {
                        return Err(unsupported(
                            format!("unsupported memory operation `{op}`"),
                            value.span,
                        ));
                    }
                };
                emit_checked_statement(out, &pad, &statement);
            }
            StmtKind::Expr(expr) => {
                if let ExprKind::Call { callee, arguments } = &expr.kind
                    && let ExprKind::Name(path) = &callee.kind
                    && let [local] = path.segments.as_slice()
                {
                    let Some(&(index, arity)) = functions.get(local.as_str()) else {
                        return Err(unsupported(
                            format!("unknown local memory function `{local}`"),
                            callee.span,
                        ));
                    };
                    if arguments.len() != arity || arguments.iter().any(|arg| arg.name.is_some()) {
                        return Err(unsupported("local memory call arity mismatch", expr.span));
                    }
                    let values = arguments
                        .iter()
                        .map(|arg| scalar_expr(&arg.value, parameters))
                        .collect::<Result<Vec<_>, _>>()?;
                    let values = values
                        .iter()
                        .map(|value| format!(", {value}"))
                        .collect::<String>();
                    emit_checked_statement(
                        out,
                        &pad,
                        &format!(
                            "cgx2_memory_function_{index}(session, steps, iterations, {span}{values})?;"
                        ),
                    );
                    continue;
                }
                let statement = if let ExprKind::Name(_) = &expr.kind {
                    format!(
                        "use_value(session, &bindings, {}, {span})?;",
                        quote_rust(bound_name(expr, names)?)
                    )
                } else {
                    let (op, args) = call(expr)?;
                    if matches!(
                        op,
                        "set_reference"
                            | "clear_reference"
                            | "set_graph_reference"
                            | "clear_graph_reference"
                    ) {
                        let args = positional(
                            args,
                            if op.starts_with("set_") { 3 } else { 2 },
                            expr.span,
                        )?;
                        let owner = quote_rust(bound_name(args[0], names)?);
                        let index = size(args[1])?;
                        let target = if op.starts_with("set_") {
                            format!("Some({})", quote_rust(bound_name(args[2], names)?))
                        } else {
                            "None".to_owned()
                        };
                        let helper = if op.contains("graph") {
                            "set_graph_reference"
                        } else {
                            "set_reference"
                        };
                        format!(
                            "{helper}(session, &bindings, {owner}, {index}, {target}, {span})?;"
                        )
                    } else if matches!(
                        op,
                        "write_byte" | "expect_byte" | "write_graph_byte" | "expect_graph_byte"
                    ) {
                        let args = positional(args, 3, expr.span)?;
                        format!(
                            "memory_byte(session, &bindings, {}, {}, {}, {}, {}, {span})?;",
                            quote_rust(bound_name(args[0], names)?),
                            size(args[1])?,
                            size(args[2])?,
                            op.starts_with("write_"),
                            op.contains("graph")
                        )
                    } else {
                        let args = positional(args, 1, expr.span)?;
                        let operand = quote_rust(bound_name(args[0], names)?);
                        match op {
                            "use" => format!("use_value(session, &bindings, {operand}, {span})?;"),
                            "drop" => {
                                format!("drop_value(session, &mut bindings, {operand}, {span})?;")
                            }
                            "end_borrow" => {
                                format!("end_borrow(&mut bindings, {operand}, {span})?;")
                            }
                            "collect" => {
                                format!("collect(session, &bindings, {operand}, {span})?;")
                            }
                            "close" => format!("close(session, &bindings, {operand}, {span})?;"),
                            "use_reference" => {
                                format!("use_reference(session, &bindings, {operand}, {span})?;")
                            }
                            _ => {
                                return Err(unsupported(
                                    format!("unsupported memory operation `{op}`"),
                                    expr.span,
                                ));
                            }
                        }
                    }
                };
                emit_checked_statement(out, &pad, &statement);
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
    let functions: BTreeMap<&str, (usize, usize)> = module
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.name.as_str(), (index, function.parameters.len())))
        .collect();
    let mut out = String::from("// Generated CGx2 native memory program. Requires cgx2-runtime.\n");
    out.push_str(&format!(
        "const CGX2_GRAPH_PROFILE: bool = {};\n",
        cgx2_sema::graph_memory_profile(module)
    ));
    out.push_str(include_str!("memory_runtime.rs.txt"));
    out.push_str(include_str!("memory_report.rs.txt"));
    for (index, function) in module.functions.iter().enumerate() {
        let parameter_list = (0..function.parameters.len())
            .map(|index| format!(", cgx2_param_{index}: i64"))
            .collect::<String>();
        out.push_str(&format!(
            "\nfn cgx2_memory_function_{index}(session: &mut Session, steps: &mut usize, iterations: &mut usize, call_span: (usize, usize){parameter_list}) -> Result<(), String> {{\n    let mut bindings: HashMap<&'static str, Binding> = HashMap::new();\n    let result = (|| -> Result<(), String> {{\n"
        ));
        let parameters = function
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| (parameter.name.as_str(), index))
            .collect();
        emit_block(
            &function.body,
            &mut out,
            2,
            &functions,
            &parameters,
            &mut BTreeMap::new(),
            false,
        )
        .map_err(|error| vec![error])?;
        out.push_str("        Ok(())\n    })();\n    let mut cleanup_error = None;\n    for value in bindings.into_values() {\n        if let Binding::Managed(handle) = value {\n            if let Err(failure) = session.unroot(handle) {\n                CGX2_CLEANUP_ERRORS.with(|value| value.borrow_mut().push(format!(\"{failure:?}\")));\n                cleanup_error.get_or_insert_with(|| runtime(failure, call_span));\n            }\n        }\n    }\n    result?;\n    if let Some(failure) = cleanup_error { return Err(failure); }\n    Ok(())\n}\n");
    }
    out.push_str("\nfn main() {\n    let report_path = match cgx2_report_path() {\n        Ok(path) => path,\n        Err(message) => { eprintln!(\"{message}\"); std::process::exit(1); }\n    };\n    CGX2_OBSERVE.with(|value| value.set(report_path.is_some()));\n    let mut session = if CGX2_GRAPH_PROFILE { match Session::try_new() { Ok(session) => session, Err(e) => { eprintln!(\"CGR013: session entry failed: {e:?}\"); std::process::exit(1); } } } else { Session::new() };\n    #[cfg(feature = \"fault-injection\")]\n    session.inject_host_failure_at_reserve(option_env!(\"CGX2_TEST_FAIL_RESERVE\").expect(\"test fault index\").parse().expect(\"valid test fault index\"));\n    let mut bindings: HashMap<&'static str, Binding> = HashMap::new();\n    let mut steps = 0;\n    let mut iterations = 0;\n    let result = (|| -> Result<(), String> {\n        let session = &mut session;\n        let steps = &mut steps;\n        let iterations = &mut iterations;\n");
    if module.functions.is_empty() {
        emit_block(
            &module.statements,
            &mut out,
            2,
            &functions,
            &BTreeMap::new(),
            &mut BTreeMap::new(),
            false,
        )
        .map_err(|error| vec![error])?;
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
        let index = functions["main"].0;
        let span = format!("({}, {})", main.span.start, main.span.end);
        out.push_str(&format!(
            "        step(steps, {span})?;\n        cgx2_memory_function_{index}(session, steps, iterations, {span})?;\n"
        ));
    }
    out.push_str("        Ok(())\n    })();\n    if let Err(message) = &result { eprintln!(\"{message}\"); }\n    if result.is_err() {\n        for binding in bindings.into_values() {\n            if let Binding::Managed(handle) = binding {\n                if let Err(failure) = session.unroot(handle) {\n                    CGX2_CLEANUP_ERRORS.with(|value| value.borrow_mut().push(format!(\"{failure:?}\")));\n                }\n            }\n        }\n    }\n    if let Some(path) = report_path {\n        if let Err(message) = cgx2_write_report(&path, &session, &result) {\n            eprintln!(\"{message}\");\n            std::process::exit(1);\n        }\n    }\n    if result.is_err() { std::process::exit(1); }\n}\n");
    Ok(out)
}
