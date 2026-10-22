//! Execution of checked memory scripts and bounded local memory functions.
use std::collections::HashMap;

use cgx2_runtime::{
    AllocationError, CollectionMode, Handle, MemoryError, MemoryReport, OwnedBytes, RegionId,
    Session,
};
use cgx2_syntax::{CallArg, Expr, ExprKind, Module, Span, Stmt, StmtKind};

use crate::RunError;

const AUTO_TRIGGER_BYTES: usize = 1 << 20;
const STEP_LIMIT: usize = 1_000_000;

enum Op {
    Call {
        name: String,
    },
    NoGc {
        budget: usize,
        body: Vec<SpannedOp>,
    },
    Region {
        name: String,
        mode: CollectionMode,
        limit: usize,
    },
    Owned {
        name: String,
        size: usize,
    },
    Move {
        name: String,
        from: String,
    },
    Promote {
        name: String,
        region: String,
        from: String,
    },
    CopyToOwned {
        name: String,
        from: String,
    },
    Alloc {
        name: String,
        region: String,
        size: usize,
    },
    Borrow {
        name: String,
        target: String,
    },
    Use {
        name: String,
    },
    Drop {
        name: String,
    },
    EndBorrow {
        name: String,
    },
    Collect {
        region: String,
    },
    Close {
        region: String,
    },
}

struct SpannedOp {
    op: Op,
    span: Span,
}

enum Binding {
    Region(RegionId),
    Owned(OwnedBytes),
    Managed(Handle),
    Borrow(String),
}

fn error(code: &'static str, message: impl Into<String>, span: Span) -> RunError {
    RunError {
        code,
        message: message.into(),
        span: Some(span),
    }
}

fn unsupported(span: Span) -> RunError {
    error("CGM001", "unsupported memory source form", span)
}

fn argument_error(span: Span) -> RunError {
    error("CGM002", "invalid memory intrinsic arguments", span)
}

fn bytes(expr: &Expr) -> Result<usize, RunError> {
    match &expr.kind {
        ExprKind::Integer(raw) => raw
            .replace('_', "")
            .parse()
            .map_err(|_| argument_error(expr.span)),
        _ => Err(argument_error(expr.span)),
    }
}

fn name(expr: &Expr) -> Result<String, RunError> {
    match &expr.kind {
        ExprKind::Name(path) if path.segments.len() == 1 => Ok(path.segments[0].clone()),
        _ => Err(argument_error(expr.span)),
    }
}

fn positional(args: &[CallArg], count: usize, span: Span) -> Result<Vec<&Expr>, RunError> {
    if args.len() != count || args.iter().any(|arg| arg.name.is_some()) {
        return Err(argument_error(span));
    }
    Ok(args.iter().map(|arg| &arg.value).collect())
}

fn lower(statements: &[Stmt]) -> Result<Vec<SpannedOp>, RunError> {
    statements.iter().map(lower_stmt).collect()
}

fn lower_stmt(stmt: &Stmt) -> Result<SpannedOp, RunError> {
    let op = match &stmt.kind {
        StmtKind::NoGc { budget, body } => Op::NoGc {
            budget: bytes(budget)?,
            body: lower(body)?,
        },
        StmtKind::Let {
            mutable: false,
            name: dest,
            annotation: None,
            value,
        } => {
            let ExprKind::Call { callee, arguments } = &value.kind else {
                return Err(unsupported(value.span));
            };
            let ExprKind::Name(path) = &callee.kind else {
                return Err(unsupported(callee.span));
            };
            let [namespace, operation] = path.segments.as_slice() else {
                return Err(unsupported(path.span));
            };
            if namespace != "memory" {
                return Err(unsupported(path.span));
            }
            match operation.as_str() {
                "region" => {
                    if arguments.len() != 2
                        || arguments
                            .iter()
                            .filter(|a| a.name.as_deref() == Some("mode"))
                            .count()
                            != 1
                        || arguments
                            .iter()
                            .filter(|a| a.name.as_deref() == Some("limit"))
                            .count()
                            != 1
                    {
                        return Err(argument_error(value.span));
                    }
                    let mode_expr = &arguments
                        .iter()
                        .find(|a| a.name.as_deref() == Some("mode"))
                        .expect("counted mode")
                        .value;
                    let limit_expr = &arguments
                        .iter()
                        .find(|a| a.name.as_deref() == Some("limit"))
                        .expect("counted limit")
                        .value;
                    let mode = match name(mode_expr)?.as_str() {
                        "auto" => CollectionMode::Auto {
                            trigger_bytes: AUTO_TRIGGER_BYTES,
                        },
                        "manual" => CollectionMode::Manual,
                        _ => return Err(argument_error(mode_expr.span)),
                    };
                    Op::Region {
                        name: dest.clone(),
                        mode,
                        limit: bytes(limit_expr)?,
                    }
                }
                "owned_bytes" => {
                    let args = positional(arguments, 1, value.span)?;
                    Op::Owned {
                        name: dest.clone(),
                        size: bytes(args[0])?,
                    }
                }
                "move" => {
                    let args = positional(arguments, 1, value.span)?;
                    Op::Move {
                        name: dest.clone(),
                        from: name(args[0])?,
                    }
                }
                "promote_owned" => {
                    let args = positional(arguments, 2, value.span)?;
                    Op::Promote {
                        name: dest.clone(),
                        region: name(args[0])?,
                        from: name(args[1])?,
                    }
                }
                "copy_to_owned" => {
                    let args = positional(arguments, 1, value.span)?;
                    Op::CopyToOwned {
                        name: dest.clone(),
                        from: name(args[0])?,
                    }
                }
                "alloc_bytes" => {
                    let args = positional(arguments, 2, value.span)?;
                    Op::Alloc {
                        name: dest.clone(),
                        region: name(args[0])?,
                        size: bytes(args[1])?,
                    }
                }
                "borrow" | "borrow_mut" => {
                    let args = positional(arguments, 1, value.span)?;
                    Op::Borrow {
                        name: dest.clone(),
                        target: name(args[0])?,
                    }
                }
                _ => return Err(unsupported(value.span)),
            }
        }
        StmtKind::Expr(expr) => match &expr.kind {
            ExprKind::Name(_) => Op::Use { name: name(expr)? },
            ExprKind::Call { callee, arguments } => {
                let ExprKind::Name(path) = &callee.kind else {
                    return Err(unsupported(callee.span));
                };
                if let [local] = path.segments.as_slice() {
                    if !arguments.is_empty() {
                        return Err(argument_error(expr.span));
                    }
                    return Ok(SpannedOp {
                        op: Op::Call {
                            name: local.clone(),
                        },
                        span: expr.span,
                    });
                }
                let [namespace, operation] = path.segments.as_slice() else {
                    return Err(unsupported(path.span));
                };
                if namespace != "memory" {
                    return Err(unsupported(path.span));
                }
                let args = positional(arguments, 1, expr.span)?;
                let operand = name(args[0])?;
                match operation.as_str() {
                    "use" => Op::Use { name: operand },
                    "drop" => Op::Drop { name: operand },
                    "end_borrow" => Op::EndBorrow { name: operand },
                    "collect" => Op::Collect { region: operand },
                    "close" => Op::Close { region: operand },
                    _ => return Err(unsupported(expr.span)),
                }
            }
            _ => return Err(unsupported(expr.span)),
        },
        _ => return Err(unsupported(stmt.span)),
    };
    Ok(SpannedOp {
        op,
        span: stmt.span,
    })
}

fn allocation(error_value: AllocationError, span: Span) -> RunError {
    error(
        "CGR013",
        format!("memory allocation failed: {error_value:?}"),
        span,
    )
}

fn runtime(error_value: MemoryError, span: Span) -> RunError {
    error(
        "CGR014",
        format!("memory operation failed: {error_value:?}"),
        span,
    )
}

fn binding<'a>(
    bindings: &'a HashMap<String, Binding>,
    key: &str,
    span: Span,
) -> Result<&'a Binding, RunError> {
    bindings
        .get(key)
        .ok_or_else(|| error("CGR014", format!("inactive memory binding `{key}`"), span))
}

fn region(
    bindings: &HashMap<String, Binding>,
    key: &str,
    span: Span,
) -> Result<RegionId, RunError> {
    match binding(bindings, key, span)? {
        Binding::Region(id) => Ok(*id),
        _ => Err(error("CGR014", format!("`{key}` is not a region"), span)),
    }
}

fn execute_with_functions(
    ops: &[SpannedOp],
    session: &mut Session,
    bindings: &mut HashMap<String, Binding>,
    steps: &mut usize,
    functions: &HashMap<String, Vec<SpannedOp>>,
) -> Result<(), RunError> {
    for SpannedOp { op, span } in ops {
        if *steps >= STEP_LIMIT {
            return Err(error(
                "CGR010",
                format!("execution step limit of {STEP_LIMIT} exceeded"),
                *span,
            ));
        }
        *steps += 1;
        match op {
            Op::Call { name } => {
                let body = functions.get(name).ok_or_else(|| {
                    error(
                        "CGR014",
                        format!("unknown local memory function `{name}`"),
                        *span,
                    )
                })?;
                let mut locals = HashMap::new();
                let result = execute_with_functions(body, session, &mut locals, steps, functions);
                let mut cleanup_error = None;
                for binding in locals.into_values() {
                    if let Binding::Managed(handle) = binding {
                        if let Err(failure) = session.unroot(handle) {
                            cleanup_error.get_or_insert_with(|| runtime(failure, *span));
                        }
                    }
                }
                result?;
                if let Some(failure) = cleanup_error {
                    return Err(failure);
                }
            }
            Op::NoGc { budget, body } => {
                let mut guard = session.no_gc(*budget);
                execute_with_functions(body, &mut guard, bindings, steps, functions)?;
            }
            Op::Region { name, mode, limit } => {
                let id = session
                    .create_region(*mode, *limit)
                    .map_err(|e| allocation(e, *span))?;
                bindings.insert(name.clone(), Binding::Region(id));
            }
            Op::Owned { name, size } => {
                let value = session
                    .alloc_owned_bytes(*size)
                    .map_err(|e| allocation(e, *span))?;
                bindings.insert(name.clone(), Binding::Owned(value));
            }
            Op::Move { name, from } => {
                if !matches!(binding(bindings, from, *span)?, Binding::Owned(_)) {
                    return Err(error("CGR014", "move requires owned bytes", *span));
                }
                let value = bindings.remove(from).expect("checked owned binding");
                bindings.insert(name.clone(), value);
            }
            Op::Promote {
                name,
                region: region_name,
                from,
            } => {
                let id = region(bindings, region_name, *span)?;
                if !matches!(binding(bindings, from, *span)?, Binding::Owned(_)) {
                    return Err(error("CGR014", "promotion requires owned bytes", *span));
                }
                let Binding::Owned(value) = bindings.remove(from).expect("checked owned binding")
                else {
                    unreachable!()
                };
                let handle = match session.promote_owned(id, value) {
                    Ok(handle) => handle,
                    Err((failure, original)) => {
                        bindings.insert(from.clone(), Binding::Owned(original));
                        return Err(allocation(failure, *span));
                    }
                };
                session.root(handle).map_err(|e| runtime(e, *span))?;
                bindings.insert(name.clone(), Binding::Managed(handle));
            }
            Op::CopyToOwned { name, from } => {
                let Binding::Managed(handle) = binding(bindings, from, *span)? else {
                    return Err(error("CGR014", "copy requires a managed handle", *span));
                };
                let value = session
                    .copy_managed_to_owned(*handle)
                    .map_err(|e| match e {
                        MemoryError::Allocation(failure) => allocation(failure, *span),
                        other => runtime(other, *span),
                    })?;
                bindings.insert(name.clone(), Binding::Owned(value));
            }
            Op::Alloc {
                name,
                region: region_name,
                size,
            } => {
                let id = region(bindings, region_name, *span)?;
                let handle = session
                    .alloc_bytes(id, *size)
                    .map_err(|e| allocation(e, *span))?;
                session.root(handle).map_err(|e| runtime(e, *span))?;
                bindings.insert(name.clone(), Binding::Managed(handle));
            }
            Op::Borrow { name, target } => {
                match binding(bindings, target, *span)? {
                    Binding::Owned(_) | Binding::Managed(_) => (),
                    _ => return Err(error("CGR014", "borrow requires a live value", *span)),
                }
                bindings.insert(name.clone(), Binding::Borrow(target.clone()));
            }
            Op::Use { name } => match binding(bindings, name, *span)? {
                Binding::Owned(bytes) => {
                    let _ = bytes.as_slice();
                }
                Binding::Managed(handle) => {
                    session.read_bytes(*handle).map_err(|e| runtime(e, *span))?;
                }
                _ => return Err(error("CGR014", "use requires a live value", *span)),
            },
            Op::Drop { name } => {
                if let Binding::Managed(handle) = binding(bindings, name, *span)? {
                    session.unroot(*handle).map_err(|e| runtime(e, *span))?;
                }
                if !matches!(
                    binding(bindings, name, *span)?,
                    Binding::Owned(_) | Binding::Managed(_)
                ) {
                    return Err(error("CGR014", "drop requires a live value", *span));
                }
                bindings.remove(name);
            }
            Op::EndBorrow { name } => {
                match binding(bindings, name, *span)? {
                    Binding::Borrow(target) => {
                        binding(bindings, target, *span)?;
                    }
                    _ => return Err(error("CGR014", "end_borrow requires a token", *span)),
                }
                bindings.remove(name);
            }
            Op::Collect {
                region: region_name,
            } => {
                let id = region(bindings, region_name, *span)?;
                session.collect(id).map_err(|e| runtime(e, *span))?;
            }
            Op::Close {
                region: region_name,
            } => {
                let id = region(bindings, region_name, *span)?;
                session.close_region(id).map_err(|e| runtime(e, *span))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
fn execute(
    ops: &[SpannedOp],
    session: &mut Session,
    bindings: &mut HashMap<String, Binding>,
    steps: &mut usize,
) -> Result<(), RunError> {
    execute_with_functions(ops, session, bindings, steps, &HashMap::new())
}

pub(crate) fn run_module(module: &Module) -> Result<MemoryReport, RunError> {
    // Lower the entire checked profile before creating a session or mutating any region.
    let ops = lower(&module.statements)?;
    let functions = module
        .functions
        .iter()
        .map(|function| lower(&function.body).map(|body| (function.name.clone(), body)))
        .collect::<Result<HashMap<_, _>, _>>()?;
    let mut session = Session::new();
    let mut bindings = HashMap::new();
    let mut steps = 0;
    if let Some(main) = module
        .functions
        .iter()
        .find(|function| function.name == "main")
    {
        let entry = [SpannedOp {
            op: Op::Call {
                name: "main".into(),
            },
            span: main.span,
        }];
        execute_with_functions(&entry, &mut session, &mut bindings, &mut steps, &functions)?;
    } else {
        execute_with_functions(&ops, &mut session, &mut bindings, &mut steps, &functions)?;
    }
    Ok(session.memory_report())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_nested_guard_keeps_zero_collection_delta() {
        let source = "let a = memory.region(mode: auto, limit: 2097152)\nlet b = memory.region(mode: manual, limit: 256)\nno_gc(budget: 191):\n    let x = memory.alloc_bytes(a, 32)\n    no_gc(budget: 96):\n        let y = memory.alloc_bytes(b, 32)\n";
        let module = cgx2_syntax::parse_module(source).unwrap();
        cgx2_sema::check_module(&module).unwrap();
        let ops = lower(&module.statements).unwrap();
        let mut session = Session::new();
        let mut bindings = HashMap::new();
        let mut steps = 0;
        let failure = execute(&ops, &mut session, &mut bindings, &mut steps).unwrap_err();
        assert_eq!(failure.code, "CGR013");
        let report = session.memory_report();
        assert_eq!(report.totals.collection_count, 0);
        assert_eq!(report.regions[1].allocation_failures, 0);
        assert_eq!(report.regions[2].allocation_failures, 1);
        assert_eq!(report.regions[1].live_bytes, 96);
        assert_eq!(report.regions[2].live_bytes, 0);
    }

    #[test]
    fn failed_promotion_retains_sole_owned_binding_and_does_not_collect() {
        let source = "let r = memory.region(mode: manual, limit: 256)\nlet o = memory.owned_bytes(8)\nno_gc(budget: 63):\n    let m = memory.promote_owned(r, o)\n";
        let module = cgx2_syntax::parse_module(source).unwrap();
        cgx2_sema::check_module(&module).unwrap();
        let ops = lower(&module.statements).unwrap();
        let mut session = Session::new();
        let mut bindings = HashMap::new();
        let mut steps = 0;
        assert_eq!(
            execute(&ops, &mut session, &mut bindings, &mut steps)
                .unwrap_err()
                .code,
            "CGR013"
        );
        assert!(matches!(bindings.get("o"), Some(Binding::Owned(bytes)) if bytes.len() == 8));
        assert!(!bindings.contains_key("m"));
        let region = region(&bindings, "r", Span { start: 0, end: 0 }).unwrap();
        let stats = session.stats(region).unwrap();
        assert_eq!(stats.committed_bytes, 0);
        assert_eq!(stats.allocation_failures, 1);
        assert_eq!(stats.collection_count, 0);
    }

    #[test]
    fn copied_owned_buffer_has_distinct_storage_from_managed_source() {
        let source = "let r = memory.region(mode: manual, limit: 256)\nlet o = memory.owned_bytes(8)\nlet m = memory.promote_owned(r, o)\nlet copy = memory.copy_to_owned(m)\n";
        let module = cgx2_syntax::parse_module(source).unwrap();
        cgx2_sema::check_module(&module).unwrap();
        let ops = lower(&module.statements).unwrap();
        let mut session = Session::new();
        let mut bindings = HashMap::new();
        let mut steps = 0;
        execute(&ops, &mut session, &mut bindings, &mut steps).unwrap();
        let Binding::Managed(handle) = bindings.get("m").unwrap() else {
            panic!("managed binding missing")
        };
        let Binding::Owned(copy) = bindings.get("copy").unwrap() else {
            panic!("owned copy missing")
        };
        assert_ne!(
            session.read_bytes(*handle).unwrap().as_ptr(),
            copy.as_slice().as_ptr()
        );
        assert_eq!(copy.as_slice(), &[0; 8]);
        session.unroot(*handle).unwrap();
        session
            .collect(region(&bindings, "r", Span { start: 0, end: 0 }).unwrap())
            .unwrap();
        assert_eq!(copy.as_slice(), &[0; 8]);
    }
}
