//! Conservative source lowering for scripts and bounded local memory functions.
use std::collections::{BTreeMap, BTreeSet};

use cgx2_ownership::{check_ops, Op};
use cgx2_syntax::{CallArg, Expr, ExprKind, Module, Path, Span, Stmt, StmtKind};

use crate::{effects, FunctionSignature, SpanTypes, Type, TypeDiagnostic, TypedModule};
// Frozen Wave 12 runtime limits; keep these in sync with cgx2-runtime::region.
const MAX_OBJECT_PAYLOAD_BYTES: usize = 1 << 20;
const MAX_OBJECT_REFERENCE_SLOTS: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Region,
    Owned,
    Managed,
    Object,
    Reference,
    Borrow,
}

#[derive(Clone)]
struct ObjectShape {
    region: String,
    slots: usize,
}

#[derive(Default)]
struct Lowerer {
    scalar_parameters: BTreeSet<String>,
    bindings: BTreeMap<String, Kind>,
    names: BTreeMap<String, String>,
    branch_depth: usize,
    loop_depth: usize,
    pending_locals: BTreeSet<String>,
    objects: BTreeMap<String, ObjectShape>,
    references: BTreeMap<String, String>,
    closed_regions: std::collections::BTreeSet<String>,
    ops: Vec<Op>,
    diagnostics: Vec<TypeDiagnostic>,
}

#[derive(Clone)]
struct LocalCall {
    name: String,
    arguments: Vec<CallArg>,
    span: Span,
    in_no_gc: bool,
    indices: BTreeSet<String>,
}

/// Return `None` for the unchanged scalar checker, or check the bounded memory profile.
/// Direct `memory.*` intrinsics are lowered to ownership operations. Function calls
/// receive separate local scopes and are validated through the local call graph.
pub(crate) fn check_memory_module(
    module: &Module,
) -> Option<Result<TypedModule, Vec<TypeDiagnostic>>> {
    if !module.statements.iter().any(has_memory)
        && !module
            .functions
            .iter()
            .any(|f| f.body.iter().any(has_memory))
    {
        return None;
    }
    if !module.functions.is_empty() {
        return Some(check_memory_functions(module));
    }
    let mut lowerer = Lowerer::default();
    lowerer.block(&module.statements);
    if lowerer.diagnostics.is_empty() {
        if let Err(errors) = check_ops(&lowerer.ops) {
            lowerer.diagnostics.extend(
                errors
                    .into_iter()
                    .map(|error| TypeDiagnostic::new(error.code, error.message, error.span)),
            );
        }
    }
    Some(if lowerer.diagnostics.is_empty() {
        Ok(TypedModule {
            finite_values: false,
            bodies: Default::default(),
            calls: Default::default(),
            functions: BTreeMap::new(),
            expressions: SpanTypes::default(),
            effects: BTreeMap::new(),
        })
    } else {
        Err(lowerer.diagnostics)
    })
}

fn without_local_calls(
    statements: &[Stmt],
    in_no_gc: bool,
    indices: &BTreeSet<String>,
    calls: &mut Vec<LocalCall>,
    diagnostics: &mut Vec<TypeDiagnostic>,
) -> Vec<Stmt> {
    let mut retained = Vec::new();
    for stmt in statements {
        match &stmt.kind {
            StmtKind::For {
                binding,
                iter,
                body,
            } => {
                let mut nested = indices.clone();
                nested.insert(binding.clone());
                retained.push(Stmt {
                    kind: StmtKind::For {
                        binding: binding.clone(),
                        iter: iter.clone(),
                        body: without_local_calls(body, in_no_gc, &nested, calls, diagnostics),
                    },
                    span: stmt.span,
                });
            }
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => retained.push(Stmt {
                kind: StmtKind::If {
                    condition: condition.clone(),
                    then_body: without_local_calls(
                        then_body,
                        in_no_gc,
                        indices,
                        calls,
                        diagnostics,
                    ),
                    else_body: without_local_calls(
                        else_body,
                        in_no_gc,
                        indices,
                        calls,
                        diagnostics,
                    ),
                },
                span: stmt.span,
            }),
            StmtKind::Expr(Expr {
                kind: ExprKind::Name(_),
                ..
            }) => {
                diagnostics.push(TypeDiagnostic::new(
                    "CGM001",
                    "memory handles cannot be function result expressions; use memory.use(value)"
                        .into(),
                    stmt.span,
                ));
            }
            StmtKind::Expr(expr) => {
                if let ExprKind::Call { callee, arguments } = &expr.kind {
                    if let ExprKind::Name(path) = &callee.kind {
                        if let [name] = path.segments.as_slice() {
                            calls.push(LocalCall {
                                name: name.clone(),
                                arguments: arguments.clone(),
                                span: expr.span,
                                in_no_gc,
                                indices: indices.clone(),
                            });
                            continue;
                        }
                    }
                }
                retained.push(stmt.clone());
            }
            StmtKind::NoGc { budget, body } => retained.push(Stmt {
                kind: StmtKind::NoGc {
                    budget: budget.clone(),
                    body: without_local_calls(body, true, indices, calls, diagnostics),
                },
                span: stmt.span,
            }),
            _ => retained.push(stmt.clone()),
        }
    }
    retained
}

fn reaches(
    target: &str,
    current: &str,
    calls: &BTreeMap<String, Vec<LocalCall>>,
    visited: &mut std::collections::BTreeSet<String>,
) -> bool {
    if current == target {
        return true;
    }
    if !visited.insert(current.into()) {
        return false;
    }
    calls.get(current).is_some_and(|next| {
        next.iter()
            .any(|call| reaches(target, &call.name, calls, visited))
    })
}

fn transitively_collects(
    current: &str,
    calls: &BTreeMap<String, Vec<LocalCall>>,
    direct: &BTreeMap<String, bool>,
    visited: &mut std::collections::BTreeSet<String>,
) -> bool {
    if !visited.insert(current.into()) {
        return false;
    }
    direct.get(current).copied().unwrap_or(false)
        || calls.get(current).is_some_and(|next| {
            next.iter()
                .any(|call| transitively_collects(&call.name, calls, direct, visited))
        })
}

// These walks deliberately inspect both possible paths, including literal guards.
fn contains_collect(ops: &[Op]) -> bool {
    ops.iter().any(|op| match op {
        Op::Collect { .. } => true,
        Op::Loop { body, .. } => contains_collect(body),
        Op::If {
            then_ops, else_ops, ..
        } => contains_collect(then_ops) || contains_collect(else_ops),
        _ => false,
    })
}

fn closes_region(ops: &[Op], region: &str) -> bool {
    ops.iter().any(|op| match op {
        Op::CloseRegion { region: closed, .. } => closed == region,
        Op::If {
            then_ops, else_ops, ..
        } => closes_region(then_ops, region) && closes_region(else_ops, region),
        _ => false,
    })
}

fn scalar_argument(expr: &Expr, parameters: &BTreeSet<String>) -> bool {
    match &expr.kind {
        ExprKind::Integer(raw) => raw.replace('_', "").parse::<i64>().is_ok(),
        ExprKind::Name(path) => {
            matches!(path.segments.as_slice(), [name] if parameters.contains(name))
        }
        ExprKind::Binary { left, right, .. } => {
            scalar_argument(left, parameters) && scalar_argument(right, parameters)
        }
        _ => false,
    }
}

fn check_memory_functions(module: &Module) -> Result<TypedModule, Vec<TypeDiagnostic>> {
    let mut diagnostics = Vec::new();
    if let Some(stmt) = module.statements.first() {
        diagnostics.push(TypeDiagnostic::new(
            "CGM001",
            "top-level statements are unsupported beside memory functions".into(),
            stmt.span,
        ));
    }
    let mut signatures = BTreeMap::new();
    let mut calls = BTreeMap::new();
    let mut direct_collect = BTreeMap::new();
    for function in &module.functions {
        if function
            .return_type
            .as_ref()
            .is_none_or(|ty| ty.to_string() != "Unit")
            || function.foreign.is_some()
            || !function.requires.is_empty()
            || !function.ensures.is_empty()
        {
            diagnostics.push(TypeDiagnostic::new(
                "CGM001",
                "memory functions require explicit Unit result and no foreign binding or contracts"
                    .into(),
                function.span,
            ));
        }
        let mut parameter_names = BTreeSet::new();
        for parameter in &function.parameters {
            if parameter.ty.to_string() != "i64" || !parameter_names.insert(parameter.name.clone())
            {
                diagnostics.push(TypeDiagnostic::new(
                    "CGM002",
                    "memory function parameters must be unique i64 scalars".into(),
                    parameter.span,
                ));
            }
        }
        if function.name == "main" && !function.parameters.is_empty() {
            diagnostics.push(TypeDiagnostic::new(
                "CGM002",
                "memory main() takes no parameters".into(),
                function.span,
            ));
        }
        if signatures.contains_key(&function.name) {
            diagnostics.push(TypeDiagnostic::new(
                "CGM001",
                format!("duplicate memory function `{}`", function.name),
                function.span,
            ));
        }
        signatures.insert(
            function.name.clone(),
            FunctionSignature {
                type_parameters: vec![],
                public: function.public,
                parameters: function.parameters.iter().map(|_| Type::I64).collect(),
                return_type: Type::Unit,
                span: function.span,
            },
        );
        let mut local_calls = Vec::new();
        let body = without_local_calls(
            &function.body,
            false,
            &BTreeSet::new(),
            &mut local_calls,
            &mut diagnostics,
        );
        let mut lowerer = Lowerer {
            scalar_parameters: parameter_names,
            ..Lowerer::default()
        };
        lowerer.block(&body);
        diagnostics.extend(lowerer.diagnostics);
        if let Err(errors) = check_ops(&lowerer.ops) {
            diagnostics.extend(
                errors
                    .into_iter()
                    .map(|error| TypeDiagnostic::new(error.code, error.message, error.span)),
            );
        }
        for op in &lowerer.ops {
            if let Op::OpenRegion { region, .. } = op {
                if !closes_region(&lowerer.ops, region) {
                    diagnostics.push(TypeDiagnostic::new(
                        "CGM001",
                        format!("local region `{region}` must be closed before return"),
                        function.span,
                    ));
                }
            }
        }
        direct_collect.insert(function.name.clone(), contains_collect(&lowerer.ops));
        calls.insert(function.name.clone(), local_calls);
    }
    if !signatures.contains_key("main") {
        diagnostics.push(TypeDiagnostic::new(
            "CGM001",
            "memory function module requires main()".into(),
            module.span,
        ));
    }
    for (caller, sites) in &calls {
        for site in sites {
            if let Some(target) = module.functions.iter().find(|f| f.name == site.name) {
                if site.arguments.len() != target.parameters.len()
                    || site.arguments.iter().any(|arg| arg.name.is_some())
                {
                    diagnostics.push(TypeDiagnostic::new(
                        "CGM002",
                        "memory call arity or argument labels do not match".into(),
                        site.span,
                    ));
                }
                let mut parameters: BTreeSet<String> = module
                    .functions
                    .iter()
                    .find(|f| f.name == *caller)
                    .map(|f| f.parameters.iter().map(|p| p.name.clone()).collect())
                    .unwrap_or_default();
                parameters.extend(site.indices.iter().cloned());
                for arg in &site.arguments {
                    if !scalar_argument(&arg.value, &parameters) {
                        diagnostics.push(TypeDiagnostic::new(
                            "CGM002",
                            "memory call argument must be an i64 scalar".into(),
                            arg.value.span,
                        ));
                    }
                }
            }
            if !signatures.contains_key(&site.name) {
                diagnostics.push(TypeDiagnostic::new(
                    "CGM001",
                    format!("unknown local memory function `{}`", site.name),
                    site.span,
                ));
            } else if reaches(caller, &site.name, &calls, &mut Default::default()) {
                diagnostics.push(TypeDiagnostic::new(
                    "CGM001",
                    "recursive memory functions are unsupported".into(),
                    site.span,
                ));
            }
            if site.in_no_gc
                && transitively_collects(
                    &site.name,
                    &calls,
                    &direct_collect,
                    &mut Default::default(),
                )
            {
                diagnostics.push(TypeDiagnostic::new(
                    "CGO004",
                    "memory.collect is forbidden across a no_gc call".into(),
                    site.span,
                ));
            }
        }
    }
    let (summaries, effect_errors) = effects::analyze(module);
    diagnostics.extend(effect_errors);
    for function in &module.functions {
        if function.effects.is_none() {
            diagnostics.push(TypeDiagnostic::new(
                "CGT012",
                "memory functions require explicit effects".into(),
                function.span,
            ));
        }
        if function.capabilities.is_none()
            && summaries
                .get(&function.name)
                .is_some_and(|summary| !summary.capabilities.is_empty())
        {
            diagnostics.push(TypeDiagnostic::new(
                "CGT013",
                "memory function requires explicit capabilities".into(),
                function.span,
            ));
        }
    }
    if diagnostics.is_empty() {
        Ok(TypedModule {
            finite_values: false,
            bodies: Default::default(),
            calls: Default::default(),
            functions: signatures,
            expressions: SpanTypes::default(),
            effects: summaries,
        })
    } else {
        Err(diagnostics)
    }
}

fn has_memory(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Match { scrutinee, arms } => {
            expr_has_memory(scrutinee) || arms.iter().any(|arm| arm.body.iter().any(has_memory))
        }
        StmtKind::LoopContract { expression, .. } => expr_has_memory(expression),
        StmtKind::If {
            condition,
            then_body,
            else_body,
        } => expr_has_memory(condition) || then_body.iter().chain(else_body).any(has_memory),
        StmtKind::NoGc { .. } => true,
        StmtKind::Let { value, .. } | StmtKind::Assign { value, .. } | StmtKind::Expr(value) => {
            expr_has_memory(value)
        }
        StmtKind::Return(Some(value)) => expr_has_memory(value),
        StmtKind::For { iter, body, .. } => expr_has_memory(iter) || body.iter().any(has_memory),
        StmtKind::Return(None) => false,
    }
}

fn expr_has_memory(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Constructor { arguments, .. } => {
            arguments.iter().any(|arg| expr_has_memory(&arg.value))
        }
        ExprKind::Collection(values) => values.iter().any(expr_has_memory),
        ExprKind::Index { collection, index } => {
            expr_has_memory(collection) || expr_has_memory(index)
        }
        ExprKind::Name(path) => path.segments.first().is_some_and(|s| s == "memory"),
        ExprKind::Call { callee, arguments } => {
            expr_has_memory(callee) || arguments.iter().any(|a| expr_has_memory(&a.value))
        }
        ExprKind::Binary { left, right, .. } => expr_has_memory(left) || expr_has_memory(right),
        _ => false,
    }
}

fn guard_operand(expr: &Expr, parameters: &BTreeSet<String>) -> bool {
    match &expr.kind {
        ExprKind::Integer(raw) => raw.replace('_', "").parse::<i64>().is_ok(),
        ExprKind::Name(path) => {
            matches!(path.segments.as_slice(), [name] if parameters.contains(name))
        }
        _ => false,
    }
}

fn branch_initializer(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::Call { callee, .. }
        if matches!(&callee.kind, ExprKind::Name(path) if matches!(path.segments.as_slice(), [namespace, op]
            if namespace == "memory" && matches!(op.as_str(), "owned_bytes" | "alloc_bytes" | "move" | "promote_owned" | "copy_to_owned" | "borrow" | "borrow_mut"))))
}

impl Lowerer {
    fn report(&mut self, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(TypeDiagnostic::new("CGM001", message.into(), span));
    }

    fn argument_error(&mut self, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(TypeDiagnostic::new("CGM002", message.into(), span));
    }

    fn block(&mut self, statements: &[Stmt]) {
        for stmt in statements {
            match &stmt.kind {
                StmtKind::Match { .. } | StmtKind::LoopContract { .. } => self.report(
                    "variants/matching/loop contracts are outside the memory script profile",
                    stmt.span,
                ),
                StmtKind::For {
                    binding,
                    iter,
                    body,
                } => {
                    if !matches!(&iter.kind, ExprKind::Call { callee, .. }
                        if matches!(&callee.kind, ExprKind::Name(path) if path.segments.as_slice() == ["range"]))
                    {
                        self.report(
                            "only range(start, end) iteration is supported in memory programs",
                            stmt.span,
                        );
                        continue;
                    }
                    let valid = matches!(&iter.kind, ExprKind::Call { callee, arguments }
                        if matches!(&callee.kind, ExprKind::Name(path) if path.segments.as_slice() == ["range"])
                        && arguments.len() == 2 && arguments.iter().all(|arg|
                            arg.name.is_none() && guard_operand(&arg.value, &self.scalar_parameters)));
                    if !valid {
                        self.argument_error("memory range requires exactly two positional i64 literals, parameters or enclosing indices", iter.span);
                    }
                    if self.bindings.contains_key(binding)
                        || self.scalar_parameters.contains(binding)
                    {
                        self.report(format!("duplicate loop index `{binding}`"), stmt.span);
                        continue;
                    }
                    self.scalar_parameters.insert(binding.clone());
                    self.loop_depth += 1;
                    let (body, _, _) = self.arm(body, stmt.span);
                    self.loop_depth -= 1;
                    self.scalar_parameters.remove(binding);
                    self.ops.push(Op::Loop {
                        body,
                        span: stmt.span,
                    });
                }
                StmtKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.guard(condition);
                    let (then_ops, then_closed, then_pending) = self.arm(then_body, stmt.span);
                    let (else_ops, else_closed, else_pending) = self.arm(else_body, stmt.span);
                    self.closed_regions
                        .extend(then_closed.into_iter().chain(else_closed));
                    self.pending_locals = then_pending.union(&else_pending).cloned().collect();
                    self.ops.push(Op::If {
                        then_ops,
                        else_ops,
                        span: stmt.span,
                    });
                }
                StmtKind::NoGc { budget, body } => {
                    self.bytes(budget, "budget");
                    self.ops.push(Op::EnterNoGc { span: stmt.span });
                    self.block(body);
                    self.ops.push(Op::ExitNoGc { span: stmt.span });
                }
                StmtKind::Let {
                    mutable,
                    name,
                    annotation,
                    value,
                } => {
                    if *mutable || annotation.is_some() {
                        self.report(
                            "memory bindings cannot be mutable or annotated in this slice",
                            stmt.span,
                        );
                        continue;
                    }
                    if self.bindings.contains_key(name) || self.scalar_parameters.contains(name) {
                        self.report(format!("duplicate memory binding `{name}`"), stmt.span);
                        continue;
                    }
                    if self.branch_depth > 0 && !branch_initializer(value) {
                        self.report(
                            "scoped memory declarations require byte temporaries or borrow tokens",
                            stmt.span,
                        );
                        continue;
                    }
                    let internal = if self.branch_depth > 0 {
                        format!("$arm{}:{name}", stmt.span.start)
                    } else {
                        name.clone()
                    };
                    if let Some(kind) = self.call(value, Some(&internal)) {
                        self.bindings.insert(name.clone(), kind);
                        self.names.insert(name.clone(), internal.clone());
                        if self.branch_depth > 0 {
                            self.pending_locals.insert(internal);
                        }
                    }
                }
                StmtKind::Expr(expr) => {
                    if let ExprKind::Name(path) = &expr.kind {
                        if let Some(name) =
                            self.name(path, &[Kind::Owned, Kind::Managed, Kind::Object])
                        {
                            self.ops.push(Op::Use {
                                value: name,
                                span: expr.span,
                            });
                        }
                    } else {
                        self.call(expr, None);
                    }
                }
                StmtKind::Assign { .. } | StmtKind::Return(_) => {
                    self.report(
                        "assignment, control flow, and returns are unsupported in memory scripts",
                        stmt.span,
                    );
                }
            }
        }
    }

    fn guard(&mut self, expr: &Expr) {
        let valid = match &expr.kind {
            ExprKind::Boolean(_) => true,
            ExprKind::Call { callee, arguments } => {
                matches!(&callee.kind, ExprKind::Name(path) if matches!(path.segments.as_slice(), [name] if matches!(name.as_str(), "$cgx2_cmp_eq" | "$cgx2_cmp_ne" | "$cgx2_cmp_lt" | "$cgx2_cmp_le" | "$cgx2_cmp_gt" | "$cgx2_cmp_ge")))
                    && arguments.len() == 2
                    && arguments.iter().all(|arg| {
                        arg.name.is_none() && guard_operand(&arg.value, &self.scalar_parameters)
                    })
            }
            _ => false,
        };
        if !valid {
            self.argument_error("conditional memory guard requires Bool literal or one comparison of two i64 literals/parameters", expr.span);
        }
    }

    // Each arm gets a separate source-name environment and internal names. Only
    // outer ownership facts flow through Op::If; locals must finish explicitly.
    fn arm(&mut self, body: &[Stmt], span: Span) -> (Vec<Op>, BTreeSet<String>, BTreeSet<String>) {
        let bindings = self.bindings.clone();
        let names = self.names.clone();
        let objects = self.objects.clone();
        let references = self.references.clone();
        let closed = self.closed_regions.clone();
        let pending = self.pending_locals.clone();
        let parent_ops = std::mem::take(&mut self.ops);
        self.branch_depth += 1;
        self.block(body);
        self.branch_depth -= 1;
        let leaked: Vec<_> = self.pending_locals.difference(&pending).cloned().collect();
        for name in leaked {
            self.report(
                format!(
                    "scoped memory local `{name}` requires explicit drop/end_borrow on every path"
                ),
                span,
            );
        }
        let inherited_pending = self
            .pending_locals
            .intersection(&pending)
            .cloned()
            .collect();
        let ops = std::mem::replace(&mut self.ops, parent_ops);
        let arm_closed = std::mem::replace(&mut self.closed_regions, closed);
        self.bindings = bindings;
        self.names = names;
        self.objects = objects;
        self.references = references;
        self.pending_locals = pending;
        (ops, arm_closed, inherited_pending)
    }

    fn call(&mut self, expr: &Expr, destination: Option<&str>) -> Option<Kind> {
        let ExprKind::Call { callee, arguments } = &expr.kind else {
            self.report(
                "memory binding requires a direct memory intrinsic call",
                expr.span,
            );
            return None;
        };
        let ExprKind::Name(path) = &callee.kind else {
            self.report(
                "computed calls are unsupported in memory scripts",
                callee.span,
            );
            return None;
        };
        let [namespace, operation] = path.segments.as_slice() else {
            self.report(
                "only direct memory.* calls are supported in memory scripts",
                path.span,
            );
            return None;
        };
        if namespace != "memory" {
            self.report(
                "calls outside reserved memory namespace are unsupported in memory scripts",
                path.span,
            );
            return None;
        }
        if self.loop_depth > 0
            && matches!(
                operation.as_str(),
                "region" | "alloc_object" | "reference" | "set_reference" | "clear_reference"
            )
        {
            self.report("region/object/reference declarations and graph mutation are unsupported in memory loops", expr.span);
            return None;
        }
        match operation.as_str() {
            "region" if destination.is_some() => {
                if arguments.len() != 2 {
                    self.argument_error("memory.region requires mode and limit", expr.span);
                    return None;
                }
                let mode = self.named(arguments, "mode")?;
                let limit = self.named(arguments, "limit")?;
                if !matches!(&mode.kind, ExprKind::Name(p) if p.segments.as_slice() == ["auto"] || p.segments.as_slice() == ["manual"])
                {
                    self.argument_error("mode must be auto or manual", mode.span);
                    return None;
                }
                self.bytes(limit, "limit")?;
                self.ops.push(Op::OpenRegion {
                    region: destination.unwrap_or_default().into(),
                    span: expr.span,
                });
                Some(Kind::Region)
            }
            "owned_bytes" if destination.is_some() => {
                let args = self.positional(arguments, 1, expr.span)?;
                self.size(args[0])?;
                self.ops.push(Op::CreateOwned {
                    value: destination.unwrap_or_default().into(),
                    span: expr.span,
                });
                Some(Kind::Owned)
            }
            "move" if destination.is_some() => {
                let args = self.positional(arguments, 1, expr.span)?;
                let from = self.reference(args[0], &[Kind::Owned])?;
                self.pending_locals.remove(&from);
                self.ops.push(Op::Move {
                    from,
                    to: destination.unwrap_or_default().into(),
                    span: expr.span,
                });
                Some(Kind::Owned)
            }
            "promote_owned" if destination.is_some() => {
                let args = self.positional(arguments, 2, expr.span)?;
                let region = self.reference(args[0], &[Kind::Region])?;
                let from = self.reference(args[1], &[Kind::Owned])?;
                self.pending_locals.remove(&from);
                self.ops.push(Op::Promote {
                    from,
                    to: destination.unwrap_or_default().into(),
                    region,
                    span: expr.span,
                });
                Some(Kind::Managed)
            }
            "copy_to_owned" if destination.is_some() => {
                let args = self.positional(arguments, 1, expr.span)?;
                let from = self.reference(args[0], &[Kind::Managed])?;
                self.ops.push(Op::CopyToOwned {
                    from,
                    to: destination.unwrap_or_default().into(),
                    span: expr.span,
                });
                Some(Kind::Owned)
            }
            "alloc_bytes" if destination.is_some() => {
                let args = self.positional(arguments, 2, expr.span)?;
                let region = self.reference(args[0], &[Kind::Region])?;
                self.size(args[1])?;
                self.ops.push(Op::CreateManaged {
                    value: destination.unwrap_or_default().into(),
                    region,
                    span: expr.span,
                });
                Some(Kind::Managed)
            }
            "alloc_object" if destination.is_some() => {
                let args = self.positional(arguments, 3, expr.span)?;
                let region = self.reference(args[0], &[Kind::Region])?;
                let payload = self.bytes(args[1], "payload size")?;
                let slots = self.bytes(args[2], "reference slots")?;
                if payload > MAX_OBJECT_PAYLOAD_BYTES || slots > MAX_OBJECT_REFERENCE_SLOTS {
                    self.argument_error("object shape exceeds runtime limits", expr.span);
                    return None;
                }
                let value = destination.unwrap_or_default().to_owned();
                self.objects.insert(
                    value.clone(),
                    ObjectShape {
                        region: region.clone(),
                        slots,
                    },
                );
                self.ops.push(Op::CreateManaged {
                    value,
                    region,
                    span: expr.span,
                });
                Some(Kind::Object)
            }
            "reference" if destination.is_some() => {
                let args = self.positional(arguments, 2, expr.span)?;
                let (owner, region) = self.object_slot(args[0], args[1])?;
                self.ops.push(Op::Use {
                    value: owner,
                    span: expr.span,
                });
                self.references
                    .insert(destination.unwrap_or_default().to_owned(), region);
                Some(Kind::Reference)
            }
            "set_reference" | "clear_reference" if destination.is_none() => {
                let count = if operation == "set_reference" { 3 } else { 2 };
                let args = self.positional(arguments, count, expr.span)?;
                let (owner, region) = self.object_slot(args[0], args[1])?;
                if operation == "set_reference" {
                    let target = self.reference(args[2], &[Kind::Object])?;
                    if self.objects[&target].region != region {
                        self.argument_error("reference edge must stay in one region", args[2].span);
                        return None;
                    }
                    self.ops.push(Op::Use {
                        value: target,
                        span: expr.span,
                    });
                }
                self.ops.push(Op::Use {
                    value: owner,
                    span: expr.span,
                });
                None
            }
            "use_reference" if destination.is_none() => {
                let args = self.positional(arguments, 1, expr.span)?;
                let reference = self.reference(args[0], &[Kind::Reference])?;
                if self.closed_regions.contains(&self.references[&reference]) {
                    self.argument_error("reference used after region close", expr.span);
                }
                None
            }
            "borrow" | "borrow_mut" if destination.is_some() => {
                let args = self.positional(arguments, 1, expr.span)?;
                let value = self.reference(args[0], &[Kind::Owned, Kind::Managed])?;
                let token = destination.unwrap_or_default().to_owned();
                self.ops.push(if operation == "borrow" {
                    Op::BorrowShared {
                        value,
                        token,
                        span: expr.span,
                    }
                } else {
                    Op::BorrowMut {
                        value,
                        token,
                        span: expr.span,
                    }
                });
                Some(Kind::Borrow)
            }
            "collect" | "close" | "drop" | "use" | "end_borrow" if destination.is_none() => {
                let args = self.positional(arguments, 1, expr.span)?;
                let expected = match operation.as_str() {
                    "collect" | "close" => &[Kind::Region][..],
                    "end_borrow" => &[Kind::Borrow][..],
                    _ => &[Kind::Owned, Kind::Managed, Kind::Object][..],
                };
                let name = self.reference(args[0], expected)?;
                if operation == "drop" || operation == "end_borrow" {
                    self.pending_locals.remove(&name);
                }
                if operation == "close" {
                    self.closed_regions.insert(name.clone());
                }
                self.ops.push(match operation.as_str() {
                    "collect" => Op::Collect {
                        region: name,
                        span: expr.span,
                    },
                    "close" => Op::CloseRegion {
                        region: name,
                        span: expr.span,
                    },
                    "drop" => Op::DropValue {
                        value: name,
                        span: expr.span,
                    },
                    "use" => Op::Use {
                        value: name,
                        span: expr.span,
                    },
                    "end_borrow" => Op::EndBorrow {
                        token: name,
                        span: expr.span,
                    },
                    _ => unreachable!(),
                });
                None
            }
            _ => {
                self.report(
                    format!(
                        "unsupported memory operation `memory.{operation}` or use in this position"
                    ),
                    expr.span,
                );
                None
            }
        }
    }

    fn object_slot(&mut self, owner: &Expr, index: &Expr) -> Option<(String, String)> {
        let name = self.reference(owner, &[Kind::Object])?;
        let slot = self.bytes(index, "reference index")?;
        let shape = &self.objects[&name];
        if slot >= shape.slots {
            self.argument_error("reference index outside object shape", index.span);
            return None;
        }
        Some((name, shape.region.clone()))
    }

    fn named<'a>(&mut self, args: &'a [CallArg], label: &str) -> Option<&'a Expr> {
        if let Some(arg) = args.iter().find(|arg| arg.name.as_deref() == Some(label)) {
            Some(&arg.value)
        } else {
            self.argument_error(
                format!("missing named argument `{label}`"),
                args.first().map_or(Span { start: 0, end: 0 }, |a| a.span),
            );
            None
        }
    }

    fn positional<'a>(
        &mut self,
        args: &'a [CallArg],
        count: usize,
        span: Span,
    ) -> Option<Vec<&'a Expr>> {
        if args.len() != count || args.iter().any(|a| a.name.is_some()) {
            self.argument_error(format!("expected {count} positional argument(s)"), span);
            None
        } else {
            Some(args.iter().map(|a| &a.value).collect())
        }
    }

    fn bytes(&mut self, expr: &Expr, label: &str) -> Option<usize> {
        if let ExprKind::Integer(raw) = &expr.kind {
            if let Ok(value) = raw.replace('_', "").parse::<usize>() {
                return Some(value);
            }
        }
        self.argument_error(
            format!("{label} must be a nonnegative integer literal within byte range"),
            expr.span,
        );
        None
    }
    fn size(&mut self, expr: &Expr) -> Option<()> {
        if let ExprKind::Name(path) = &expr.kind {
            if matches!(path.segments.as_slice(), [name] if self.scalar_parameters.contains(name)) {
                return Some(());
            }
        }
        self.bytes(expr, "size").map(|_| ())
    }

    fn reference(&mut self, expr: &Expr, allowed: &[Kind]) -> Option<String> {
        if let ExprKind::Name(path) = &expr.kind {
            self.name(path, allowed)
        } else {
            self.argument_error("expected a memory handle name", expr.span);
            None
        }
    }

    fn name(&mut self, path: &Path, allowed: &[Kind]) -> Option<String> {
        if let [name] = path.segments.as_slice() {
            if self
                .bindings
                .get(name)
                .is_some_and(|kind| allowed.contains(kind))
            {
                return Some(self.names.get(name).unwrap_or(name).clone());
            }
        }
        self.argument_error(
            format!(
                "unknown memory handle or wrong handle type `{}`",
                path.segments.join(".")
            ),
            path.span,
        );
        None
    }
}
