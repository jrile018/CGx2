//! Execution of checked memory scripts and bounded local memory functions.
use std::collections::{HashMap, HashSet};

use cgx2_runtime::{
    AllocationError, CollectionMode, Handle, MemoryError, MemoryReport, OwnedBytes, RegionId,
    Session,
};
use cgx2_syntax::{CallArg, Expr, ExprKind, Module, Span, Stmt, StmtKind};

use crate::RunError;

pub(crate) struct MemoryFailure {
    pub error: RunError,
    pub report: Option<Box<MemoryReport>>,
    pub rollback_preserved: Option<bool>,
    pub operation_failure: Option<Box<FailureSnapshot>>,
    pub final_unwind: Option<Box<FailureSnapshot>>,
    pub cleanup_errors: Vec<String>,
}

#[derive(Default)]
struct FailureState {
    // Invocation-wide execution state: never reset by an arm, loop or local call.
    entered_iterations: usize,
    rollback: Option<bool>,
    operation: Option<FailureObservation>,
    cleanup_errors: Vec<String>,
    // Reporting metadata only; payloads/edges remain exclusively in Session.
    // Invocation-wide so dropping a binding or leaving a frame cannot erase
    // the provenance of a still-reachable child. Handles include session and
    // generation identity, so slot reuse cannot inherit an old layout.
    layouts: HashMap<Handle, SourceLayout>,
}

#[derive(Clone, Copy)]
enum SourceLayout {
    Bytes,
    Object { slots: usize },
    Graph,
}

/// Source roots are distinct live managed bindings in active source scopes.
/// Runtime roots count live `Session` registrations, including duplicates.
/// Edges are occupied reference slots on source roots; the memory report
/// supplies byte, failure, and collection counters.
pub struct FailureSnapshot {
    pub rooted_handles: Option<usize>,
    pub rooted_edges: Option<usize>,
    pub runtime_rooted_handles: Option<usize>,
    pub runtime_root_registrations: Option<usize>,
    pub runtime_root_census_error: Option<String>,
    pub memory: MemoryReport,
}

struct FailureObservation {
    handles: HashSet<Handle>,
    edges: usize,
    observable: bool,
    runtime_rooted_handles: Option<usize>,
    runtime_root_registrations: Option<usize>,
    runtime_root_census_error: Option<String>,
    memory: MemoryReport,
}

impl FailureObservation {
    fn new(
        session: &Session,
        bindings: &HashMap<String, Binding>,
        layouts: &HashMap<Handle, SourceLayout>,
    ) -> Self {
        let (runtime_rooted_handles, runtime_root_registrations, runtime_root_census_error) =
            match session.root_census() {
                Ok(census) => (
                    Some(census.rooted_handles),
                    Some(census.root_registrations),
                    None,
                ),
                Err(error) => (None, None, Some(format!("{error:?}"))),
            };
        let mut observation = Self {
            handles: HashSet::new(),
            edges: 0,
            observable: true,
            runtime_rooted_handles,
            runtime_root_registrations,
            runtime_root_census_error,
            memory: session.memory_report(),
        };
        observation.add_roots(session, bindings, layouts);
        observation
    }

    fn add_roots(
        &mut self,
        session: &Session,
        bindings: &HashMap<String, Binding>,
        layouts: &HashMap<Handle, SourceLayout>,
    ) {
        let Some(roots) = graph_snapshot(session, bindings, layouts) else {
            self.observable = false;
            return;
        };
        for (_, handle, live, references) in roots {
            if live && self.handles.insert(handle) {
                self.edges += references.iter().filter(|edge| edge.is_some()).count();
            }
        }
    }

    fn finish(self) -> FailureSnapshot {
        FailureSnapshot {
            rooted_handles: self.observable.then_some(self.handles.len()),
            rooted_edges: self.observable.then_some(self.edges),
            runtime_rooted_handles: self.runtime_rooted_handles,
            runtime_root_registrations: self.runtime_root_registrations,
            runtime_root_census_error: self.runtime_root_census_error,
            memory: self.memory,
        }
    }
}

type GraphSnapshot = Vec<(String, Handle, bool, Vec<Option<Handle>>)>;

fn graph_snapshot(
    session: &Session,
    bindings: &HashMap<String, Binding>,
    layouts: &HashMap<Handle, SourceLayout>,
) -> Option<GraphSnapshot> {
    let mut roots = Vec::new();
    for (name, binding) in bindings {
        let Binding::Managed(handle) = binding else {
            continue;
        };
        let live = session.read_bytes(*handle).is_ok();
        let mut references = Vec::new();
        if live {
            // Mutable bytes can validate as a graph without having been
            // allocated as one. Only source construction selects the visitor.
            match layouts.get(handle)? {
                SourceLayout::Bytes => (),
                SourceLayout::Object { slots } => {
                    for index in 0..*slots {
                        references.push(session.reference(*handle, index).ok()?);
                    }
                }
                SourceLayout::Graph => session
                    .visit_graph_references(*handle, |_, child| references.push(child))
                    .ok()?,
            }
        }
        roots.push((name.clone(), *handle, live, references));
    }
    roots.sort_by(|a, b| a.0.cmp(&b.0));
    Some(roots)
}

const AUTO_TRIGGER_BYTES: usize = 1 << 20;
const STEP_LIMIT: usize = 1_000_000;

enum Op {
    Loop {
        index: String,
        start: Expr,
        end: Expr,
        body: Vec<SpannedOp>,
    },
    If {
        condition: Expr,
        then_ops: Vec<SpannedOp>,
        else_ops: Vec<SpannedOp>,
    },
    Call {
        name: String,
        arguments: Vec<Expr>,
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
        size: Expr,
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
        size: Expr,
    },
    AllocObject {
        name: String,
        region: String,
        payload: usize,
        slots: usize,
        graph: bool,
    },
    SetReference {
        owner: String,
        index: usize,
        target: Option<String>,
        graph: bool,
    },
    Reference {
        name: String,
        owner: String,
        index: usize,
        graph: bool,
    },
    Byte {
        owner: String,
        offset: usize,
        value: u8,
        write: bool,
        graph: bool,
    },
    UseReference {
        name: String,
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
    Reference(Option<Handle>),
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
fn scalar(expr: &Expr, parameters: &HashMap<String, i64>) -> Result<i64, RunError> {
    match &expr.kind {
        ExprKind::Integer(raw) => raw
            .replace('_', "")
            .parse::<i64>()
            .map_err(|_| error("CGR013", "i64 scalar out of range", expr.span)),
        ExprKind::Name(path) if path.segments.len() == 1 => parameters
            .get(&path.segments[0])
            .copied()
            .ok_or_else(|| argument_error(expr.span)),
        ExprKind::Binary { left, op, right } => {
            let a = scalar(left, parameters)?;
            let b = scalar(right, parameters)?;
            let result = match op {
                cgx2_syntax::BinaryOp::Add => a.checked_add(b),
                cgx2_syntax::BinaryOp::Subtract => a.checked_sub(b),
                cgx2_syntax::BinaryOp::Multiply => a.checked_mul(b),
                cgx2_syntax::BinaryOp::Divide => a.checked_div(b),
            };
            result.ok_or_else(|| error("CGR013", "i64 scalar arithmetic failed", expr.span))
        }
        _ => Err(argument_error(expr.span)),
    }
}
fn condition(expr: &Expr, parameters: &HashMap<String, i64>) -> Result<bool, RunError> {
    if let ExprKind::Boolean(value) = &expr.kind {
        return Ok(*value);
    }
    let ExprKind::Call { callee, arguments } = &expr.kind else {
        return Err(argument_error(expr.span));
    };
    let ExprKind::Name(path) = &callee.kind else {
        return Err(argument_error(expr.span));
    };
    let args = positional(arguments, 2, expr.span)?;
    let a = scalar(args[0], parameters)?;
    let b = scalar(args[1], parameters)?;
    match path.segments.as_slice() {
        [op] => match op.as_str() {
            "$cgx2_cmp_eq" => Ok(a == b),
            "$cgx2_cmp_ne" => Ok(a != b),
            "$cgx2_cmp_lt" => Ok(a < b),
            "$cgx2_cmp_le" => Ok(a <= b),
            "$cgx2_cmp_gt" => Ok(a > b),
            "$cgx2_cmp_ge" => Ok(a >= b),
            _ => Err(argument_error(expr.span)),
        },
        _ => Err(argument_error(expr.span)),
    }
}

fn checked_size(expr: &Expr, parameters: &HashMap<String, i64>) -> Result<usize, RunError> {
    if let ExprKind::Name(path) = &expr.kind {
        if path.segments.len() == 1 {
            let value = scalar(expr, parameters)?;
            return usize::try_from(value).map_err(|_| {
                error(
                    "CGR013",
                    "byte size is negative or outside target usize range",
                    expr.span,
                )
            });
        }
    }
    bytes(expr)
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
    lower_scope(statements, &mut HashMap::new(), false)
}

fn lower_scope(
    statements: &[Stmt],
    names: &mut HashMap<String, String>,
    in_branch: bool,
) -> Result<Vec<SpannedOp>, RunError> {
    statements
        .iter()
        .map(|stmt| lower_stmt(stmt, names, in_branch))
        .collect()
}

fn scoped_name(expr: &Expr, names: &HashMap<String, String>) -> Result<String, RunError> {
    let source = name(expr)?;
    Ok(names.get(&source).unwrap_or(&source).clone())
}

fn lower_stmt(
    stmt: &Stmt,
    names: &mut HashMap<String, String>,
    in_branch: bool,
) -> Result<SpannedOp, RunError> {
    let op = match &stmt.kind {
        StmtKind::For {
            binding,
            iter,
            body,
        } => {
            let ExprKind::Call { arguments, .. } = &iter.kind else {
                return Err(unsupported(iter.span));
            };
            if arguments.len() != 2 {
                return Err(argument_error(iter.span));
            }
            Op::Loop {
                index: binding.clone(),
                start: arguments[0].value.clone(),
                end: arguments[1].value.clone(),
                body: lower_scope(body, &mut names.clone(), true)?,
            }
        }
        StmtKind::If {
            condition,
            then_body,
            else_body,
        } => Op::If {
            condition: condition.clone(),
            then_ops: lower_scope(then_body, &mut names.clone(), true)?,
            else_ops: lower_scope(else_body, &mut names.clone(), true)?,
        },
        StmtKind::NoGc { budget, body } => Op::NoGc {
            budget: bytes(budget)?,
            body: lower_scope(body, names, in_branch)?,
        },
        StmtKind::Let {
            mutable: false,
            name: dest,
            annotation: None,
            value,
        } => {
            let internal = if in_branch {
                format!("$arm{}:{dest}", stmt.span.start)
            } else {
                dest.clone()
            };
            names.insert(dest.clone(), internal.clone());
            let dest = &internal;
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
                    let mode = match scoped_name(mode_expr, names)?.as_str() {
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
                        size: args[0].clone(),
                    }
                }
                "move" => {
                    let args = positional(arguments, 1, value.span)?;
                    Op::Move {
                        name: dest.clone(),
                        from: scoped_name(args[0], names)?,
                    }
                }
                "promote_owned" => {
                    let args = positional(arguments, 2, value.span)?;
                    Op::Promote {
                        name: dest.clone(),
                        region: scoped_name(args[0], names)?,
                        from: scoped_name(args[1], names)?,
                    }
                }
                "copy_to_owned" => {
                    let args = positional(arguments, 1, value.span)?;
                    Op::CopyToOwned {
                        name: dest.clone(),
                        from: scoped_name(args[0], names)?,
                    }
                }
                "alloc_bytes" => {
                    let args = positional(arguments, 2, value.span)?;
                    Op::Alloc {
                        name: dest.clone(),
                        region: scoped_name(args[0], names)?,
                        size: args[1].clone(),
                    }
                }
                "alloc_object" | "alloc_graph" => {
                    let args = positional(arguments, 3, value.span)?;
                    Op::AllocObject {
                        name: dest.clone(),
                        region: scoped_name(args[0], names)?,
                        payload: bytes(args[1])?,
                        slots: bytes(args[2])?,
                        graph: operation == "alloc_graph",
                    }
                }
                "reference" | "graph_reference" => {
                    let args = positional(arguments, 2, value.span)?;
                    Op::Reference {
                        name: dest.clone(),
                        owner: scoped_name(args[0], names)?,
                        index: bytes(args[1])?,
                        graph: operation == "graph_reference",
                    }
                }
                "borrow" | "borrow_mut" => {
                    let args = positional(arguments, 1, value.span)?;
                    Op::Borrow {
                        name: dest.clone(),
                        target: scoped_name(args[0], names)?,
                    }
                }
                _ => return Err(unsupported(value.span)),
            }
        }
        StmtKind::Expr(expr) => match &expr.kind {
            ExprKind::Name(_) => Op::Use {
                name: scoped_name(expr, names)?,
            },
            ExprKind::Call { callee, arguments } => {
                let ExprKind::Name(path) = &callee.kind else {
                    return Err(unsupported(callee.span));
                };
                if let [local] = path.segments.as_slice() {
                    return Ok(SpannedOp {
                        op: Op::Call {
                            name: local.clone(),
                            arguments: arguments.iter().map(|arg| arg.value.clone()).collect(),
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
                if matches!(
                    operation.as_str(),
                    "set_reference"
                        | "clear_reference"
                        | "set_graph_reference"
                        | "clear_graph_reference"
                ) {
                    let count = if operation.starts_with("set_") { 3 } else { 2 };
                    let args = positional(arguments, count, expr.span)?;
                    return Ok(SpannedOp {
                        op: Op::SetReference {
                            owner: scoped_name(args[0], names)?,
                            index: bytes(args[1])?,
                            graph: operation.contains("graph"),
                            target: if count == 3 {
                                Some(scoped_name(args[2], names)?)
                            } else {
                                None
                            },
                        },
                        span: expr.span,
                    });
                }
                if matches!(
                    operation.as_str(),
                    "write_byte" | "expect_byte" | "write_graph_byte" | "expect_graph_byte"
                ) {
                    let args = positional(arguments, 3, expr.span)?;
                    return Ok(SpannedOp {
                        op: Op::Byte {
                            owner: scoped_name(args[0], names)?,
                            offset: bytes(args[1])?,
                            value: u8::try_from(bytes(args[2])?)
                                .map_err(|_| unsupported(expr.span))?,
                            write: operation.starts_with("write_"),
                            graph: operation.contains("graph"),
                        },
                        span: expr.span,
                    });
                }
                let args = positional(arguments, 1, expr.span)?;
                let operand = scoped_name(args[0], names)?;
                match operation.as_str() {
                    "use" => Op::Use { name: operand },
                    "drop" => Op::Drop { name: operand },
                    "end_borrow" => Op::EndBorrow { name: operand },
                    "collect" => Op::Collect { region: operand },
                    "close" => Op::Close { region: operand },
                    "use_reference" => Op::UseReference { name: operand },
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

type FunctionBodies = HashMap<String, (Vec<String>, Vec<SpannedOp>)>;
fn cleanup_scope(
    session: &mut Session,
    bindings: &mut HashMap<String, Binding>,
    outer: &HashSet<String>,
    span: Span,
    failure_state: &mut FailureState,
) -> Option<RunError> {
    let locals: Vec<_> = bindings
        .keys()
        .filter(|name| !outer.contains(*name))
        .cloned()
        .collect();
    let mut cleanup_error = None;
    for name in locals {
        if let Some(Binding::Managed(handle)) = bindings.remove(&name) {
            if let Err(failure) = session.unroot(handle) {
                failure_state.cleanup_errors.push(format!("{failure:?}"));
                cleanup_error.get_or_insert_with(|| runtime(failure, span));
            }
        }
    }
    cleanup_error
}

struct CallContext<'a> {
    fallible_lifetimes: bool,
    functions: &'a FunctionBodies,
    scalars: &'a HashMap<String, i64>,
}
fn execute_with_functions(
    ops: &[SpannedOp],
    session: &mut Session,
    bindings: &mut HashMap<String, Binding>,
    steps: &mut usize,
    context: &CallContext<'_>,
    failure_state: &mut FailureState,
    observe_rollback: bool,
) -> Result<(), RunError> {
    for SpannedOp { op, span } in ops {
        if *steps >= STEP_LIMIT {
            if observe_rollback {
                if let Some(snapshot) = failure_state.operation.as_mut() {
                    snapshot.add_roots(session, bindings, &failure_state.layouts);
                } else {
                    failure_state.operation = Some(FailureObservation::new(
                        session,
                        bindings,
                        &failure_state.layouts,
                    ));
                }
            }
            if observe_rollback {
                failure_state.rollback.get_or_insert(true);
            }
            return Err(error(
                "CGR010",
                format!("execution step limit of {STEP_LIMIT} exceeded"),
                *span,
            ));
        }
        *steps += 1;
        let before = observe_rollback
            .then(|| graph_snapshot(session, bindings, &failure_state.layouts))
            .flatten();
        let result = (|| -> Result<(), RunError> {
            match op {
                Op::Loop {
                    index,
                    start,
                    end,
                    body,
                } => {
                    let start = scalar(start, context.scalars)?;
                    let end = scalar(end, context.scalars)?;
                    let outer: HashSet<_> = bindings.keys().cloned().collect();
                    let mut scalars = context.scalars.clone();
                    for current in start..end {
                        if failure_state.entered_iterations >= 100_000 {
                            if observe_rollback {
                                failure_state
                                    .operation
                                    .get_or_insert_with(|| {
                                        FailureObservation::new(
                                            session,
                                            bindings,
                                            &failure_state.layouts,
                                        )
                                    })
                                    .add_roots(session, bindings, &failure_state.layouts);
                                failure_state.rollback.get_or_insert(true);
                            }
                            return Err(error(
                                "CGR026",
                                "memory entered-iteration limit of 100000 exceeded",
                                *span,
                            ));
                        }
                        failure_state.entered_iterations += 1;
                        scalars.insert(index.clone(), current);
                        let result = execute_with_functions(
                            body,
                            session,
                            bindings,
                            steps,
                            &CallContext {
                                functions: context.functions,
                                fallible_lifetimes: context.fallible_lifetimes,
                                scalars: &scalars,
                            },
                            failure_state,
                            observe_rollback,
                        );
                        let cleanup_error =
                            cleanup_scope(session, bindings, &outer, *span, failure_state);
                        result?;
                        if let Some(failure) = cleanup_error {
                            return Err(failure);
                        }
                    }
                }
                Op::If {
                    condition: guard,
                    then_ops,
                    else_ops,
                } => {
                    let selected = if condition(guard, context.scalars)? {
                        then_ops
                    } else {
                        else_ops
                    };
                    let outer: HashSet<_> = bindings.keys().cloned().collect();
                    let result = execute_with_functions(
                        selected,
                        session,
                        bindings,
                        steps,
                        context,
                        failure_state,
                        observe_rollback,
                    );
                    // Inner execution records the failure census before these roots
                    // are removed. Always restore the outer environment on unwind.
                    let cleanup_error =
                        cleanup_scope(session, bindings, &outer, *span, failure_state);
                    result?;
                    if let Some(failure) = cleanup_error {
                        return Err(failure);
                    }
                }
                Op::Call { name, arguments } => {
                    let (parameters, body) = context.functions.get(name).ok_or_else(|| {
                        error(
                            "CGR014",
                            format!("unknown local memory function `{name}`"),
                            *span,
                        )
                    })?;
                    if parameters.len() != arguments.len() {
                        return Err(argument_error(*span));
                    }
                    let values = arguments
                        .iter()
                        .map(|arg| scalar(arg, context.scalars))
                        .collect::<Result<Vec<_>, _>>()?;
                    let local_scalars: HashMap<String, i64> =
                        parameters.iter().cloned().zip(values).collect();
                    let mut locals = HashMap::new();
                    let result = execute_with_functions(
                        body,
                        session,
                        &mut locals,
                        steps,
                        &CallContext {
                            functions: context.functions,
                            fallible_lifetimes: context.fallible_lifetimes,
                            scalars: &local_scalars,
                        },
                        failure_state,
                        observe_rollback,
                    );
                    let mut cleanup_error = None;
                    for binding in locals.into_values() {
                        if let Binding::Managed(handle) = binding {
                            if let Err(failure) = session.unroot(handle) {
                                failure_state.cleanup_errors.push(format!("{failure:?}"));
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
                    let mut guard = if context.fallible_lifetimes {
                        session
                            .try_no_gc(*budget)
                            .map_err(|e| allocation(e, *span))?
                    } else {
                        session.no_gc(*budget)
                    };
                    execute_with_functions(
                        body,
                        &mut guard,
                        bindings,
                        steps,
                        context,
                        failure_state,
                        observe_rollback,
                    )?;
                }
                Op::Region { name, mode, limit } => {
                    let id = session
                        .create_region(*mode, *limit)
                        .map_err(|e| allocation(e, *span))?;
                    bindings.insert(name.clone(), Binding::Region(id));
                }
                Op::Owned { name, size } => {
                    let failure_span = if matches!(&size.kind, ExprKind::Name(_)) {
                        size.span
                    } else {
                        *span
                    };
                    let size = checked_size(size, context.scalars)?;
                    let value = session
                        .alloc_owned_bytes(size)
                        .map_err(|e| allocation(e, failure_span))?;
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
                    let Binding::Owned(value) =
                        bindings.remove(from).expect("checked owned binding")
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
                    if observe_rollback {
                        failure_state.layouts.insert(handle, SourceLayout::Bytes);
                    }
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
                    let failure_span = if matches!(&size.kind, ExprKind::Name(_)) {
                        size.span
                    } else {
                        *span
                    };
                    let size = checked_size(size, context.scalars)?;
                    let id = region(bindings, region_name, *span)?;
                    let handle = session
                        .alloc_bytes(id, size)
                        .map_err(|e| allocation(e, failure_span))?;
                    session.root(handle).map_err(|e| runtime(e, *span))?;
                    if observe_rollback {
                        failure_state.layouts.insert(handle, SourceLayout::Bytes);
                    }
                    bindings.insert(name.clone(), Binding::Managed(handle));
                }
                Op::AllocObject {
                    name,
                    region: region_name,
                    payload,
                    slots,
                    graph,
                } => {
                    let id = region(bindings, region_name, *span)?;
                    let handle = if *graph {
                        session
                            .alloc_zeroed_graph(id, *payload, *slots)
                            .map_err(|e| graph_error(e, *span))?
                    } else {
                        session
                            .alloc_object(id, *payload, *slots)
                            .map_err(|e| allocation(e, *span))?
                    };
                    session.root(handle).map_err(|e| runtime(e, *span))?;
                    if observe_rollback {
                        failure_state.layouts.insert(
                            handle,
                            if *graph {
                                SourceLayout::Graph
                            } else {
                                SourceLayout::Object { slots: *slots }
                            },
                        );
                    }
                    bindings.insert(name.clone(), Binding::Managed(handle));
                }
                Op::SetReference {
                    owner,
                    index,
                    target,
                    graph,
                } => {
                    let Binding::Managed(owner_handle) = binding(bindings, owner, *span)? else {
                        return Err(error("CGR014", "reference owner requires object", *span));
                    };
                    let target_handle = match target {
                        Some(target) => {
                            let Binding::Managed(handle) = binding(bindings, target, *span)? else {
                                return Err(error(
                                    "CGR014",
                                    "reference target requires object",
                                    *span,
                                ));
                            };
                            Some(*handle)
                        }
                        None => None,
                    };
                    if *graph {
                        session
                            .set_graph_reference(*owner_handle, *index, target_handle)
                            .map_err(|e| graph_error(e, *span))?;
                    } else {
                        session
                            .set_reference(*owner_handle, *index, target_handle)
                            .map_err(|e| runtime(e, *span))?;
                    }
                }
                Op::Byte {
                    owner,
                    offset,
                    value,
                    write,
                    graph,
                } => {
                    let Binding::Managed(handle) = binding(bindings, owner, *span)? else {
                        return Err(unsupported(*span));
                    };
                    if *write {
                        if *graph {
                            session
                                .write_graph_bytes(*handle, *offset, &[*value])
                                .map_err(|e| graph_error(e, *span))?;
                        } else {
                            session
                                .write_bytes(*handle, *offset, &[*value])
                                .map_err(|e| error("CGR014", format!("{e:?}"), *span))?;
                        }
                    } else {
                        let mut actual = [0];
                        if *graph {
                            session
                                .read_graph_bytes(*handle, *offset, &mut actual)
                                .map_err(|e| graph_error(e, *span))?;
                        } else {
                            actual[0] = *session
                                .read_bytes(*handle)
                                .map_err(|e| runtime(e, *span))?
                                .get(*offset)
                                .ok_or_else(|| unsupported(*span))?;
                        }
                        if actual[0] != *value {
                            return Err(error("CGR015", "memory byte expectation failed", *span));
                        }
                    }
                }
                Op::Reference {
                    name,
                    owner,
                    index,
                    graph,
                } => {
                    let Binding::Managed(handle) = binding(bindings, owner, *span)? else {
                        return Err(error("CGR014", "reference owner requires object", *span));
                    };
                    let reference = if *graph {
                        session
                            .graph_reference(*handle, *index)
                            .map_err(|e| graph_error(e, *span))?
                    } else {
                        session
                            .reference(*handle, *index)
                            .map_err(|e| runtime(e, *span))?
                    };
                    bindings.insert(name.clone(), Binding::Reference(reference));
                }
                Op::UseReference { name } => {
                    let Binding::Reference(reference) = binding(bindings, name, *span)? else {
                        return Err(error("CGR014", "expected optional object reference", *span));
                    };
                    let handle = reference
                        .ok_or_else(|| error("CGR014", "empty object reference", *span))?;
                    session.read_bytes(handle).map_err(|e| runtime(e, *span))?;
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
            Ok(())
        })();
        if let Err(error) = result {
            if observe_rollback {
                if let Some(snapshot) = failure_state.operation.as_mut() {
                    snapshot.add_roots(session, bindings, &failure_state.layouts);
                } else {
                    failure_state.operation = Some(FailureObservation::new(
                        session,
                        bindings,
                        &failure_state.layouts,
                    ));
                }
            }
            if observe_rollback && failure_state.rollback.is_none() {
                failure_state.rollback = before
                    .zip(graph_snapshot(session, bindings, &failure_state.layouts))
                    .map(|(a, b)| a == b);
            }
            return Err(error);
        }
    }
    Ok(())
}

fn graph_error(error_value: cgx2_runtime::GraphError, span: Span) -> RunError {
    match error_value {
        cgx2_runtime::GraphError::Memory(MemoryError::Allocation(e)) => allocation(e, span),
        cgx2_runtime::GraphError::Memory(e) => runtime(e, span),
        e => error("CGR014", format!("{e:?}"), span),
    }
}

#[cfg(test)]
fn execute(
    ops: &[SpannedOp],
    session: &mut Session,
    bindings: &mut HashMap<String, Binding>,
    steps: &mut usize,
) -> Result<(), RunError> {
    let functions = HashMap::new();
    let scalars = HashMap::new();
    execute_with_functions(
        ops,
        session,
        bindings,
        steps,
        &CallContext {
            functions: &functions,
            fallible_lifetimes: false,
            scalars: &scalars,
        },
        &mut FailureState::default(),
        false,
    )
}

pub(crate) fn run_module_with_failure(
    module: &Module,
    host_fail_reserve: Option<usize>,
    observe_rollback: bool,
) -> Result<MemoryReport, MemoryFailure> {
    // Lower the checked profile before mutating any region.
    let lower_failure = |error| MemoryFailure {
        error,
        report: None,
        rollback_preserved: None,
        operation_failure: None,
        final_unwind: None,
        cleanup_errors: Vec::new(),
    };
    let ops = lower(&module.statements).map_err(lower_failure)?;
    let functions = module
        .functions
        .iter()
        .map(|function| {
            lower(&function.body).map(|body| {
                (
                    function.name.clone(),
                    (
                        function.parameters.iter().map(|p| p.name.clone()).collect(),
                        body,
                    ),
                )
            })
        })
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(lower_failure)?;
    let fallible_lifetimes = cgx2_sema::graph_memory_profile(module);
    let mut session = if fallible_lifetimes {
        Session::try_new().map_err(|e| {
            lower_failure(error(
                "CGR013",
                format!("session entry failed: {e:?}"),
                module.span,
            ))
        })?
    } else {
        Session::new()
    };
    #[cfg(feature = "failure-injection-test")]
    if let Some(index) = host_fail_reserve {
        session.inject_host_failure_at_reserve(index);
    }
    #[cfg(not(feature = "failure-injection-test"))]
    let _ = host_fail_reserve;
    let mut bindings = HashMap::new();
    let mut steps = 0;
    let mut failure_state = FailureState::default();
    let empty_scalars = HashMap::new();
    let context = CallContext {
        functions: &functions,
        fallible_lifetimes,
        scalars: &empty_scalars,
    };
    let result = if let Some(main) = module
        .functions
        .iter()
        .find(|function| function.name == "main")
    {
        let entry = [SpannedOp {
            op: Op::Call {
                name: "main".into(),
                arguments: Vec::new(),
            },
            span: main.span,
        }];
        execute_with_functions(
            &entry,
            &mut session,
            &mut bindings,
            &mut steps,
            &context,
            &mut failure_state,
            observe_rollback,
        )
    } else {
        execute_with_functions(
            &ops,
            &mut session,
            &mut bindings,
            &mut steps,
            &context,
            &mut failure_state,
            observe_rollback,
        )
    };
    if let Err(error) = result {
        let operation_failure = failure_state
            .operation
            .map(FailureObservation::finish)
            .map(Box::new);
        for binding in bindings.into_values() {
            if let Binding::Managed(handle) = binding {
                // Keep the original source diagnostic even if cleanup fails.
                if let Err(failure) = session.unroot(handle) {
                    failure_state.cleanup_errors.push(format!("{failure:?}"));
                }
            }
        }
        let final_unwind = observe_rollback.then(|| {
            Box::new(
                FailureObservation::new(&session, &HashMap::new(), &failure_state.layouts).finish(),
            )
        });
        return Err(MemoryFailure {
            error,
            report: Some(Box::new(session.memory_report())),
            rollback_preserved: failure_state.rollback,
            operation_failure,
            final_unwind,
            cleanup_errors: failure_state.cleanup_errors,
        });
    }
    Ok(session.memory_report())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_alias_provenance_is_not_inferred_from_payload_or_reference_api() {
        let mut session = Session::new();
        let r = session
            .create_region(CollectionMode::Manual, 8 << 20)
            .unwrap();
        let raw = session.alloc_object(r, 24, 1).unwrap();
        let graph = session.alloc_zeroed_graph(r, 0, 65536).unwrap();
        session.set_reference(raw, 0, Some(graph)).unwrap();
        session
            .set_graph_reference(graph, 65535, Some(raw))
            .unwrap();
        let mut layouts = HashMap::from([
            (raw, SourceLayout::Object { slots: 1 }),
            (graph, SourceLayout::Graph),
        ]);
        for child in [
            session.reference(raw, 0).unwrap().unwrap(),
            session.graph_reference(graph, 65535).unwrap().unwrap(),
        ] {
            let aliases = HashMap::from([
                ("alias".to_owned(), Binding::Managed(child)),
                ("duplicate".to_owned(), Binding::Managed(child)),
            ]);
            let snapshot = graph_snapshot(&session, &aliases, &layouts).unwrap();
            assert_eq!(snapshot[0].3.iter().flatten().count(), 1);
            let census = FailureObservation::new(&session, &aliases, &layouts).finish();
            assert_eq!(census.rooted_handles, Some(1));
            assert_eq!(census.rooted_edges, Some(1));
            let layout = layouts.remove(&child).unwrap();
            assert!(graph_snapshot(&session, &aliases, &layouts).is_none());
            let unknown = FailureObservation::new(&session, &aliases, &layouts).finish();
            assert_eq!(unknown.rooted_handles, None);
            assert_eq!(unknown.rooted_edges, None);
            layouts.insert(child, layout);
        }
        session.write_bytes(graph, 0, &[0]).unwrap();
        let bindings = HashMap::from([("graph".to_owned(), Binding::Managed(graph))]);
        assert!(graph_snapshot(&session, &bindings, &layouts).is_none());
        assert_eq!(session.read_bytes(graph).unwrap()[0], 0);
    }

    #[test]
    fn runtime_census_counts_duplicate_registrations_beyond_source_bindings() {
        let source = "let r = memory.region(mode: manual, limit: 256)\nlet value = memory.alloc_object(r, 8, 0)\n";
        let module = cgx2_syntax::parse_module(source).unwrap();
        cgx2_sema::check_module(&module).unwrap();
        let ops = lower(&module.statements).unwrap();
        let mut session = Session::new();
        let mut bindings = HashMap::new();
        let mut steps = 0;
        execute(&ops, &mut session, &mut bindings, &mut steps).unwrap();
        let Binding::Managed(handle) = bindings["value"] else {
            panic!("managed value missing")
        };
        session.root(handle).unwrap();
        let snapshot = FailureObservation::new(
            &session,
            &bindings,
            &HashMap::from([(handle, SourceLayout::Object { slots: 0 })]),
        )
        .finish();
        assert_eq!(snapshot.rooted_handles, Some(1));
        assert_eq!(snapshot.runtime_rooted_handles, Some(1));
        assert_eq!(snapshot.runtime_root_registrations, Some(2));
        assert_eq!(snapshot.runtime_root_census_error, None);
        session.unroot(handle).unwrap();
        session.unroot(handle).unwrap();
    }

    #[test]
    fn failed_object_allocation_keeps_prior_reference_and_region_state() {
        let source = "let r = memory.region(mode: manual, limit: 1024)\nlet parent = memory.alloc_object(r, 8, 1)\nlet child = memory.alloc_object(r, 4, 0)\nmemory.set_reference(parent, 0, child)\nno_gc(budget: 0):\n    let failed = memory.alloc_object(r, 1, 0)\n";
        let module = cgx2_syntax::parse_module(source).unwrap();
        cgx2_sema::check_module(&module).unwrap();
        let ops = lower(&module.statements).unwrap();
        let mut session = Session::new();
        let mut bindings = HashMap::new();
        let mut steps = 0;
        let failure = execute(&ops, &mut session, &mut bindings, &mut steps).unwrap_err();
        assert_eq!(failure.code, "CGR013");
        assert!(
            source[failure.span.unwrap().start..failure.span.unwrap().end].contains("alloc_object")
        );
        let Binding::Managed(parent) = bindings["parent"] else {
            panic!("parent missing")
        };
        let Binding::Managed(child) = bindings["child"] else {
            panic!("child missing")
        };
        assert_eq!(session.reference(parent, 0).unwrap(), Some(child));
        assert!(!bindings.contains_key("failed"));
        let id = region(&bindings, "r", Span { start: 0, end: 0 }).unwrap();
        let stats = session.stats(id).unwrap();
        assert_eq!(stats.allocation_failures, 1);
        assert_eq!(stats.collection_count, 0);
        assert!(stats.live_bytes > 0);
    }

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
