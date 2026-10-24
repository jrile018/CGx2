use std::collections::BTreeMap;
use std::{collections::HashMap, fmt};

use crate::RunError;
use cgx2_foreign::{registry_from_source, ScalarValue, SourceCall, ValueShape, WorkerConfig};
use cgx2_syntax::{
    AssignOp, BinaryOp, Expr, ExprKind, Function, Module, Span, Stmt, StmtKind, TypeSyntax,
};

const STEP_LIMIT: usize = 1_000_000;
const DEPTH_LIMIT: usize = 64;
const OUTPUT_BYTE_LIMIT: usize = 1_048_576;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    I64(i64),
    Bool(bool),
    String(String),
    Variant {
        ty: cgx2_sema::Type,
        case_index: usize,
        case_name: String,
        payloads: Vec<Value>,
    },
    Record {
        name: String,
        fields: Vec<(String, Value)>,
    },
    Sequence {
        ty: cgx2_sema::Type,
        values: Vec<Value>,
    },
    Unit,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::I64(v) => write!(f, "{v}"),
            Self::Bool(v) => write!(f, "{v}"),
            Self::String(v) => f.write_str(v),
            Self::Variant {
                ty,
                case_name,
                payloads,
                ..
            } => {
                write!(f, "{ty}.{case_name}(")?;
                for (i, value) in payloads.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{value}")?;
                }
                f.write_str(")")
            }
            Self::Record { name, fields } => {
                write!(f, "{name}(")?;
                for (index, (field, value)) in fields.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{field}: {value}")?;
                }
                f.write_str(")")
            }
            Self::Sequence { ty, values } => {
                if matches!(ty, cgx2_sema::Type::Array(_)) {
                    f.write_str("Array")?;
                }
                f.write_str("[")?;
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{v}")?;
                }
                f.write_str("]")
            }
            Self::Unit => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunResult {
    pub value: Value,
}

#[derive(Clone)]
struct Binding {
    value: Value,
    mutable: bool,
    annotation: Option<TypeSyntax>,
}
struct Scope(Vec<HashMap<String, Binding>>);

impl Scope {
    fn new() -> Self {
        Self(vec![HashMap::new()])
    }
    fn push(&mut self) {
        self.0.push(HashMap::new());
    }
    fn pop(&mut self) {
        self.0.pop();
    }
    fn clear_current(&mut self) {
        self.0.last_mut().expect("scope").clear();
    }
    fn contains_key(&self, name: &str) -> bool {
        self.0.last().expect("scope").contains_key(name)
    }
    fn insert(&mut self, name: String, binding: Binding) {
        self.0.last_mut().expect("scope").insert(name, binding);
    }
    fn get(&self, name: &str) -> Option<&Binding> {
        self.0.iter().rev().find_map(|scope| scope.get(name))
    }
    fn get_mut(&mut self, name: &str) -> Option<&mut Binding> {
        self.0
            .iter_mut()
            .rev()
            .find_map(|scope| scope.get_mut(name))
    }
}
enum Flow {
    Continue(Value),
    Return(Value),
}

struct Evaluator<'a> {
    typed: Option<cgx2_sema::TypedModule>,
    owner: String,
    module: &'a Module,
    args: &'a [String],
    steps: usize,
    depth: usize,
    origins: BTreeMap<String, std::path::PathBuf>,
    failure_file: Option<std::path::PathBuf>,
}

pub(crate) fn run_module(module: &Module, args: &[String]) -> Result<RunResult, RunError> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn_scoped(scope, || run_module_inner(module, args))
            .map_err(|_| error("CGR010", "cannot allocate evaluator stack", module.span))?
            .join()
            .unwrap_or_else(|_| Err(error("CGR009", "evaluator failed", module.span)))
    })
}
fn run_module_inner(module: &Module, args: &[String]) -> Result<RunResult, RunError> {
    let prepared = prepare_data(module)?;
    let module = prepared.as_ref().unwrap_or(module);
    let typed = if prepared.is_some() {
        Some(
            cgx2_sema::check_module(module)
                .map_err(|e| error("CGR009", e[0].message.clone(), e[0].span))?,
        )
    } else {
        None
    };
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
    for function in &module.functions {
        if matches!(function.name.as_str(), "arg" | "arg_count") {
            return Err(error(
                "CGR009",
                format!("{} is a reserved builtin", function.name),
                function.span,
            ));
        }
        for parameter in &function.parameters {
            validate_type(&parameter.ty)?;
        }
        if let Some(ty) = &function.return_type {
            validate_type(ty)?;
        }
        reject_unsupported(&function.body)?;
    }
    reject_unsupported(&module.statements)?;
    let mut evaluator = Evaluator {
        typed,
        owner: String::new(),
        module,
        args,
        steps: 0,
        depth: 0,
        origins: BTreeMap::new(),
        failure_file: None,
    };
    let value = if module.functions.is_empty() {
        let mut scope = Scope::new();
        match evaluator.eval_block(&module.statements, &mut scope)? {
            Flow::Continue(value) => value,
            Flow::Return(_) => {
                return Err(error("CGR009", "return outside a function", module.span))
            }
        }
    } else {
        let mains: Vec<_> = module
            .functions
            .iter()
            .filter(|f| f.name == "main")
            .collect();
        match mains.as_slice() {
            [] => return Err(error("CGR008", "function module has no main", module.span)),
            [main] if main.parameters.is_empty() => {
                evaluator.call_function(main, Vec::new(), main.span)?
            }
            [main] => return Err(error("CGR008", "main must have zero parameters", main.span)),
            [first, ..] => return Err(error("CGR008", "ambiguous main definitions", first.span)),
        }
    };
    finalize_value(value, module.span)
}

// Exercise the real formatter without allocating its output. Stop at the first
// write that would exceed the cap, including names and aggregate punctuation.
fn check_output_bound(value: &Value, span: Span) -> Result<(), RunError> {
    struct ByteBudget(usize);
    impl fmt::Write for ByteBudget {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            self.0 = self.0.checked_sub(text.len()).ok_or(fmt::Error)?;
            Ok(())
        }
    }
    fmt::write(&mut ByteBudget(OUTPUT_BYTE_LIMIT), format_args!("{value}"))
        .map_err(|_| error("CGR010", "output exceeds 1048576 bytes", span))
}

fn finalize_value(value: Value, span: Span) -> Result<RunResult, RunError> {
    check_value_bound(&value, span)?;
    check_output_bound(&value, span)?;
    Ok(RunResult { value })
}

pub(crate) fn run_managed_package(
    program: &cgx2_sema::default_managed::ManagedProgramV1,
    args: &[String],
    origins: &BTreeMap<String, std::path::PathBuf>,
) -> (Result<RunResult, RunError>, Option<std::path::PathBuf>) {
    if program.program().entry == usize::MAX {
        return (
            Err(error(
                "CGR008",
                "package requires main",
                program.entry_span(),
            )),
            None,
        );
    }
    let execution = match program.execute(args) {
        Ok(e) => e,
        Err(ds) => {
            return (
                Err(error(ds[0].code, ds[0].message.clone(), ds[0].span)),
                None,
            )
        }
    };
    if std::env::var_os("CGX2_MANAGED_EVIDENCE").is_some() {
        eprintln!(
            "CGX2_MANAGED_EVIDENCE {}",
            execution.evidence.json(&execution.memory)
        );
    }
    let file = execution
        .result
        .as_ref()
        .err()
        .and_then(|e| e.function)
        .and_then(|i| origins.get(&program.program().strings[program.program().functions[i].name]))
        .cloned();
    let ty = program
        .program()
        .functions
        .get(program.program().entry)
        .map_or(&cgx2_sema::Type::Unit, |f| &program.types()[f.result]);
    (
        execution
            .result
            .map(|v| RunResult {
                value: crate::default_managed::copy(v, ty, program),
            })
            .map_err(|e| {
                error(
                    e.code,
                    e.message,
                    cgx2_syntax::Span {
                        start: e.loc.start,
                        end: e.loc.end,
                    },
                )
            }),
        file,
    )
}

pub(crate) fn run_package_module(
    module: &Module,
    args: &[String],
    origins: &BTreeMap<String, std::path::PathBuf>,
) -> (Result<RunResult, RunError>, Option<std::path::PathBuf>) {
    if cgx2_sema::default_managed::profile(module) {
        if !module.functions.iter().any(|f| f.name == "main") {
            return (
                Err(error("CGR008", "package requires main", module.span)),
                None,
            );
        }
        let program = match cgx2_sema::default_managed::compile_ast(module) {
            Ok(p) => p,
            Err(ds) => {
                return (
                    Err(error(
                        if ds[0].code == "CGRS003" {
                            "CGR009"
                        } else {
                            ds[0].code
                        },
                        ds[0].message.clone(),
                        ds[0].span,
                    )),
                    None,
                )
            }
        };
        return run_managed_package(&program, args, origins);
    }
    let prepared = match prepare_data(module) {
        Ok(p) => p,
        Err(e) => return (Err(e), None),
    };
    let module = prepared.as_ref().unwrap_or(module);
    let typed = if prepared.is_some() {
        match cgx2_sema::check_module(module) {
            Ok(t) => Some(t),
            Err(es) => {
                return (
                    Err(error("CGR009", es[0].message.clone(), es[0].span)),
                    None,
                )
            }
        }
    } else {
        None
    };
    for function in &module.functions {
        if let Err(error) = reject_unsupported(&function.body) {
            return (Err(error), origins.get(&function.name).cloned());
        }
    }
    let mut evaluator = Evaluator {
        typed,
        owner: String::new(),
        module,
        args,
        steps: 0,
        depth: 0,
        origins: origins.clone(),
        failure_file: None,
    };
    let result = if let Some(main) = module.functions.iter().find(|f| f.name == "main") {
        evaluator
            .call_function(main, vec![], main.span)
            .and_then(|value| finalize_value(value, module.span))
    } else {
        Err(error("CGR008", "package requires main", module.span))
    };
    (result, evaluator.failure_file)
}

fn error(code: &'static str, message: impl Into<String>, span: Span) -> RunError {
    RunError {
        code,
        message: message.into(),
        span: Some(span),
    }
}

impl Evaluator<'_> {
    fn step(&mut self, span: Span) -> Result<(), RunError> {
        if self.steps >= STEP_LIMIT {
            return Err(error(
                "CGR010",
                format!("execution step limit of {STEP_LIMIT} exceeded"),
                span,
            ));
        }
        self.steps += 1;
        Ok(())
    }

    fn eval_block(&mut self, statements: &[Stmt], scope: &mut Scope) -> Result<Flow, RunError> {
        let mut tail = Value::Unit;
        for stmt in statements {
            self.step(stmt.span)?;
            match &stmt.kind {
                StmtKind::Match { scrutinee, arms } => {
                    let value = self.eval_expr(scrutinee, scope)?;
                    let Value::Variant {
                        ty,
                        case_index,
                        payloads,
                        ..
                    } = value
                    else {
                        return Err(error("CGR009", "match requires a variant", scrutinee.span));
                    };
                    let cases = cgx2_sema::variant_cases(self.module, &Default::default(), &ty)
                        .ok_or_else(|| {
                            error("CGR009", "unknown variant identity", scrutinee.span)
                        })?;
                    let arm = arms
                        .iter()
                        .find(|arm| match &arm.pattern {
                            cgx2_syntax::Pattern::Wildcard(_) => true,
                            cgx2_syntax::Pattern::Case { case, .. } => {
                                cases.get(case_index).is_some_and(|(name, _)| name == case)
                            }
                        })
                        .ok_or_else(|| error("CGR009", "nonexhaustive match", stmt.span))?;
                    scope.push();
                    if let cgx2_syntax::Pattern::Case { bindings, .. } = &arm.pattern {
                        for (binding, value) in bindings.iter().zip(payloads) {
                            if binding.name != "_" {
                                scope.insert(
                                    binding.name.clone(),
                                    Binding {
                                        value,
                                        mutable: false,
                                        annotation: None,
                                    },
                                );
                            }
                        }
                    }
                    let flow = self.eval_block(&arm.body, scope);
                    scope.pop();
                    match flow? {
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                        Flow::Continue(value) => tail = value,
                    }
                }
                StmtKind::LoopContract { .. } => {
                    return Err(error(
                        "CGR009",
                        "annotated loops require S2b/S3-V",
                        stmt.span,
                    ))
                }
                StmtKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    let Value::Bool(condition) = self.eval_expr(condition, scope)? else {
                        return Err(error("CGR009", "if condition must be Bool", condition.span));
                    };
                    scope.push();
                    let flow =
                        self.eval_block(if condition { then_body } else { else_body }, scope);
                    scope.pop();
                    if let Flow::Return(value) = flow? {
                        return Ok(Flow::Return(value));
                    }
                    tail = Value::Unit;
                }
                StmtKind::Let {
                    mutable,
                    name,
                    annotation,
                    value,
                } => {
                    let value = self.eval_expr(value, scope)?;
                    if let Some(ty) = annotation {
                        check_type(self.module, ty, &value)?;
                    }
                    if scope.contains_key(name) {
                        return Err(error(
                            "CGR009",
                            format!("duplicate binding {name}"),
                            stmt.span,
                        ));
                    }
                    scope.insert(
                        name.clone(),
                        Binding {
                            value,
                            mutable: *mutable,
                            annotation: annotation.clone(),
                        },
                    );
                    tail = Value::Unit;
                }
                StmtKind::Assign { target, op, value } => {
                    let name = single_name(&target.segments, target.span)?;
                    let new_value = self.eval_expr(value, scope)?;
                    let binding = scope.get_mut(name).ok_or_else(|| {
                        error("CGR001", format!("unknown name {name}"), target.span)
                    })?;
                    if !binding.mutable {
                        return Err(error(
                            "CGR003",
                            format!("cannot assign to immutable binding {name}"),
                            target.span,
                        ));
                    }
                    let assigned = match op {
                        AssignOp::Set => new_value,
                        AssignOp::Add => {
                            binary_values(&binding.value, &new_value, BinaryOp::Add, stmt.span)?
                        }
                    };
                    if let Some(ty) = &binding.annotation {
                        check_type(self.module, ty, &assigned)?;
                    } else if std::mem::discriminant(&binding.value)
                        != std::mem::discriminant(&assigned)
                    {
                        return Err(error(
                            "CGR004",
                            format!("assignment changes the type of {name}"),
                            stmt.span,
                        ));
                    }
                    binding.value = assigned;
                    tail = Value::Unit;
                }
                StmtKind::Return(value) => {
                    let value = match value {
                        Some(expr) => self.eval_expr(expr, scope)?,
                        None => Value::Unit,
                    };
                    return Ok(Flow::Return(value));
                }
                StmtKind::Expr(expr) => tail = self.eval_expr(expr, scope)?,
                StmtKind::For {
                    binding,
                    iter,
                    body,
                } => {
                    if !matches!(&iter.kind,ExprKind::Call{callee,..} if matches!(&callee.kind,ExprKind::Name(p) if p.segments.as_slice()==["range"]))
                    {
                        let values = match self.eval_expr(iter, scope)? {
                            Value::Sequence { values, .. } => values,
                            Value::String(s) => {
                                s.chars().map(|c| Value::String(c.to_string())).collect()
                            }
                            _ => {
                                return Err(error(
                                    "CGR009",
                                    "invalid collection iterator",
                                    iter.span,
                                ))
                            }
                        };
                        for value in values {
                            self.step(stmt.span)?;
                            scope.push();
                            scope.insert(
                                binding.clone(),
                                Binding {
                                    value,
                                    mutable: false,
                                    annotation: None,
                                },
                            );
                            let flow = self.eval_block(body, scope);
                            scope.pop();
                            if let Flow::Return(v) = flow? {
                                return Ok(Flow::Return(v));
                            }
                        }
                        tail = Value::Unit;
                        continue;
                    }
                    let ExprKind::Call { arguments, .. } = &iter.kind else {
                        unreachable!("checked range")
                    };
                    self.step(iter.span)?;
                    let start = self.eval_expr(&arguments[0].value, scope)?;
                    let end = self.eval_expr(&arguments[1].value, scope)?;
                    let (Value::I64(start), Value::I64(end)) = (start, end) else {
                        unreachable!("checked i64 range")
                    };
                    scope.push();
                    scope.insert(
                        binding.clone(),
                        Binding {
                            value: Value::I64(start),
                            mutable: false,
                            annotation: None,
                        },
                    );
                    scope.push();
                    for current in start..end {
                        self.step(stmt.span)?;
                        let index = scope.0.len() - 2;
                        scope.0[index]
                            .get_mut(binding)
                            .expect("range binding")
                            .value = Value::I64(current);
                        let flow = self.eval_block(body, scope);
                        scope.clear_current();
                        if let Flow::Return(value) = flow? {
                            return Ok(Flow::Return(value));
                        }
                    }
                    scope.pop();
                    scope.pop();
                    tail = Value::Unit;
                }
                StmtKind::NoGc { .. } => {
                    return Err(error(
                        "CGR009",
                        "memory execution is unsupported",
                        stmt.span,
                    ))
                }
            }
        }
        Ok(Flow::Continue(tail))
    }

    fn eval_expr(&mut self, expr: &Expr, scope: &Scope) -> Result<Value, RunError> {
        self.step(expr.span)?;
        let value = self.eval_expr_inner(expr, scope)?;
        check_value_bound(&value, expr.span)?;
        if self.typed.as_ref().is_some_and(|t| t.finite_values) {
            let (nodes, bytes) = value_size(&value);
            let work = nodes.saturating_add(bytes);
            if self.steps.saturating_add(work) > STEP_LIMIT {
                return Err(error("CGR010", "execution work budget exceeded", expr.span));
            }
            self.steps += work;
        }
        Ok(value)
    }
    fn eval_expr_inner(&mut self, expr: &Expr, scope: &Scope) -> Result<Value, RunError> {
        match &expr.kind {
            ExprKind::Constructor {
                ty,
                case,
                arguments,
            } => {
                let resolved = cgx2_sema::resolve_type(self.module, &Default::default(), ty)
                    .map_err(|_| error("CGR009", "invalid constructor type", ty.span))?;
                let cases = cgx2_sema::variant_cases(self.module, &Default::default(), &resolved)
                    .ok_or_else(|| {
                    error("CGR009", "constructor requires a variant", ty.span)
                })?;
                let case_index = cases
                    .iter()
                    .position(|(name, _)| name == case)
                    .ok_or_else(|| error("CGR009", "unknown constructor case", expr.span))?;
                let payloads: Result<Vec<_>, _> = arguments
                    .iter()
                    .map(|arg| self.eval_expr(&arg.value, scope))
                    .collect();
                Ok(Value::Variant {
                    ty: resolved,
                    case_index,
                    case_name: case.clone(),
                    payloads: payloads?,
                })
            }
            ExprKind::Collection(items) => {
                let values = items
                    .iter()
                    .map(|e| self.eval_expr(e, scope))
                    .collect::<Result<Vec<_>, _>>()?;
                let ty = self
                    .expression_type(expr.span)
                    .cloned()
                    .ok_or_else(|| error("CGR009", "collection lacks checked type", expr.span))?;
                let v = Value::Sequence { ty, values };
                check_value_bound(&v, expr.span)?;
                Ok(v)
            }
            ExprKind::Index { collection, index } => {
                let xs = self.eval_expr(collection, scope)?;
                let i = self.eval_expr(index, scope)?;
                let Value::I64(i) = i else {
                    return Err(error("CGR004", "index requires i64", index.span));
                };
                self.option(expr.span, access(&xs, i))
            }
            ExprKind::Integer(raw) => raw
                .replace('_', "")
                .parse::<i64>()
                .map(Value::I64)
                .map_err(|_| error("CGR005", "integer literal exceeds i64 range", expr.span)),
            ExprKind::StringLiteral(raw) => decode_string(raw, expr.span).map(Value::String),
            ExprKind::Boolean(value) => Ok(Value::Bool(*value)),
            ExprKind::Name(path) => match path.segments.as_slice() {
                [name] => scope
                    .get(name)
                    .map(|b| b.value.clone())
                    .ok_or_else(|| error("CGR001", format!("unknown name {name}"), path.span)),
                [base, field] => match scope.get(base).map(|b| &b.value) {
                    Some(Value::Record { fields, .. }) => fields
                        .iter()
                        .find(|(name, _)| name == field)
                        .map(|(_, value)| value.clone())
                        .ok_or_else(|| {
                            error("CGR001", format!("unknown field {field}"), path.span)
                        }),
                    _ => Err(error("CGR001", format!("unknown record {base}"), path.span)),
                },
                _ => Err(error("CGR001", "unsupported field path", path.span)),
            },
            ExprKind::Binary { left, op, right } => {
                let left = self.eval_expr(left, scope)?;
                let right = self.eval_expr(right, scope)?;
                binary_values(&left, &right, *op, expr.span)
            }
            ExprKind::Call { callee, arguments } => {
                let ExprKind::Name(path) = &callee.kind else {
                    return Err(error(
                        "CGR009",
                        "computed calls are unsupported",
                        callee.span,
                    ));
                };
                let name = single_name(&path.segments, path.span)?;
                if let Some(record) = self.module.records.iter().find(|record| {
                    record.name == name
                        || self.module.name.as_ref().is_some_and(|p| {
                            format!("{}.{}", p.segments.join("."), record.name) == name
                        })
                }) {
                    let mut values = HashMap::new();
                    for arg in arguments {
                        let value = self.eval_expr(&arg.value, scope)?;
                        if let Some(field) = &arg.name {
                            values.insert(field.clone(), value);
                        }
                    }
                    let fields = record
                        .fields
                        .iter()
                        .map(|field| {
                            (
                                field.name.clone(),
                                values.remove(&field.name).expect("checked record field"),
                            )
                        })
                        .collect();
                    return Ok(Value::Record {
                        name: self
                            .module
                            .name
                            .as_ref()
                            .filter(|_| !record.name.contains('.'))
                            .map_or_else(
                                || record.name.clone(),
                                |p| format!("{}.{}", p.segments.join("."), record.name),
                            ),
                        fields,
                    });
                }
                if matches!(
                    name,
                    "$cgx2_cmp_eq"
                        | "$cgx2_cmp_ne"
                        | "$cgx2_cmp_lt"
                        | "$cgx2_cmp_le"
                        | "$cgx2_cmp_gt"
                        | "$cgx2_cmp_ge"
                ) {
                    let values = arguments
                        .iter()
                        .map(|a| self.eval_expr(&a.value, scope))
                        .collect::<Result<Vec<_>, _>>()?;
                    return comparison_values(name, &values, expr.span);
                }
                if is_data_call(name) && !self.module.functions.iter().any(|f| f.name == name) {
                    let values = arguments
                        .iter()
                        .map(|a| self.eval_expr(&a.value, scope))
                        .collect::<Result<Vec<_>, _>>()?;
                    return self.data_call(name, values, expr.span);
                }
                let function = if matches!(name, "arg" | "arg_count") {
                    None
                } else {
                    let mut matches = self.module.functions.iter().filter(|f| f.name == name);
                    let first = matches.next().ok_or_else(|| {
                        error("CGR002", format!("unknown function {name}"), callee.span)
                    })?;
                    if matches.next().is_some() {
                        return Err(error(
                            "CGR002",
                            format!("ambiguous function {name}"),
                            callee.span,
                        ));
                    }
                    Some(first.clone())
                };
                let values = arguments
                    .iter()
                    .map(|a| self.eval_expr(&a.value, scope))
                    .collect::<Result<Vec<_>, _>>()?;
                if let Some(foreign) = function
                    .as_ref()
                    .filter(|function| function.foreign.is_some())
                {
                    return self.call_foreign(foreign, values, expr.span);
                }
                match name {
                    "arg_count" => {
                        expect_arity(name, 0, values.len(), expr.span)?;
                        i64::try_from(self.args.len()).map(Value::I64).map_err(|_| {
                            error("CGR005", "argument count exceeds i64 range", expr.span)
                        })
                    }
                    "arg" => {
                        expect_arity(name, 1, values.len(), expr.span)?;
                        let Value::I64(index) = values[0] else {
                            return Err(error(
                                "CGR004",
                                "arg index must be i64",
                                arguments[0].span,
                            ));
                        };
                        let index = usize::try_from(index).map_err(|_| {
                            error("CGR007", "arg index must be nonnegative", arguments[0].span)
                        })?;
                        self.args
                            .get(index)
                            .cloned()
                            .map(Value::String)
                            .ok_or_else(|| {
                                error(
                                    "CGR007",
                                    format!("arg index {index} is out of range"),
                                    arguments[0].span,
                                )
                            })
                    }
                    _ => {
                        self.call_function(&function.expect("resolved function"), values, expr.span)
                    }
                }
            }
        }
    }

    fn call_foreign(
        &self,
        function: &Function,
        args: Vec<Value>,
        span: Span,
    ) -> Result<Value, RunError> {
        expect_arity(&function.name, function.parameters.len(), args.len(), span)?;
        let binding = function.foreign.as_ref().expect("foreign function");
        let target = &binding.target.segments;
        let module = target[..target.len() - 1].join(".");
        let attribute = target.last().expect("checked Python target");
        let positional: Vec<_> = function
            .parameters
            .iter()
            .map(|parameter| foreign_shape(&parameter.ty))
            .collect();
        let call = SourceCall {
            key: &function.name,
            module: &module,
            attribute,
            positional: &positional,
            returns: foreign_shape(
                function
                    .return_type
                    .as_ref()
                    .expect("checked foreign return"),
            ),
        };
        let registry =
            registry_from_source(&[call]).map_err(|err| error("CGR017", err.to_string(), span))?;
        let values: Vec<_> = args
            .into_iter()
            .map(|value| match value {
                Value::I64(value) => ScalarValue::I64(value),
                Value::Bool(value) => ScalarValue::Bool(value),
                Value::String(value) => ScalarValue::String(value),
                Value::Record { .. } | Value::Variant { .. } | Value::Sequence { .. } => {
                    unreachable!("checked scalar argument")
                }
                Value::Unit => unreachable!("checked scalar argument"),
            })
            .collect();
        let returned = registry
            .call_worker(&WorkerConfig::default(), &function.name, &values)
            .map_err(|err| {
                let code = match err.category() {
                    "module_unavailable" => "CGR018",
                    "attribute_missing" => "CGR019",
                    "python_exception" => "CGR020",
                    "return_conversion" => "CGR021",
                    _ => "CGR017",
                };
                error(code, err.to_string(), span)
            })?;
        if std::env::var_os("CGX2_PYTHON_TRACE").as_deref() == Some(std::ffi::OsStr::new("1")) {
            eprintln!(
                "Python worker executable={:?} version={:?} prefix={:?}",
                returned.provenance.executable,
                returned.provenance.version,
                returned.provenance.prefix
            );
        }
        match returned.value {
            ScalarValue::I64(value) => Ok(Value::I64(value)),
            ScalarValue::Bool(value) => Ok(Value::Bool(value)),
            ScalarValue::String(value) => Ok(Value::String(value)),
        }
    }

    fn call_function(
        &mut self,
        function: &Function,
        args: Vec<Value>,
        span: Span,
    ) -> Result<Value, RunError> {
        expect_arity(&function.name, function.parameters.len(), args.len(), span)?;
        if self.depth >= DEPTH_LIMIT {
            return Err(error(
                "CGR011",
                format!("function call depth limit of {DEPTH_LIMIT} exceeded"),
                span,
            ));
        }
        self.depth += 1;
        let prior_owner = std::mem::replace(&mut self.owner, function.name.clone());
        let mut scope = Scope::new();
        for (param, value) in function.parameters.iter().zip(args) {
            check_type(self.module, &param.ty, &value)?;
            scope.insert(
                param.name.clone(),
                Binding {
                    value,
                    mutable: false,
                    annotation: Some(param.ty.clone()),
                },
            );
        }
        let result = (|| {
            for condition in &function.requires {
                if self.eval_expr(condition, &scope)? != Value::Bool(true) {
                    return Err(error("CGR015", "requires condition failed", condition.span));
                }
            }
            let value = match self.eval_block(&function.body, &mut scope)? {
                Flow::Continue(value) | Flow::Return(value) => value,
            };
            if let Some(ty) = &function.return_type {
                check_type(self.module, ty, &value)?;
            }
            scope.insert(
                "result".into(),
                Binding {
                    value: value.clone(),
                    mutable: false,
                    annotation: None,
                },
            );
            for condition in &function.ensures {
                if self.eval_expr(condition, &scope)? != Value::Bool(true) {
                    return Err(error("CGR016", "ensures condition failed", condition.span));
                }
            }
            Ok(value)
        })();
        self.depth -= 1;
        self.owner = prior_owner;
        if result.is_err() && self.failure_file.is_none() {
            self.failure_file = self.origins.get(&function.name).cloned();
        }
        result
    }
}

fn foreign_shape(ty: &TypeSyntax) -> ValueShape {
    match ty.to_string().as_str() {
        "i64" => ValueShape::I64,
        "Bool" => ValueShape::Bool,
        "String" => ValueShape::String,
        _ => unreachable!("foreign types statically checked"),
    }
}

fn reject_unsupported(statements: &[Stmt]) -> Result<(), RunError> {
    for stmt in statements {
        if let StmtKind::Match { arms, .. } = &stmt.kind {
            for arm in arms {
                reject_unsupported(&arm.body)?;
            }
        }
        if let StmtKind::LoopContract { .. } = &stmt.kind {
            return Err(error(
                "CGR009",
                "annotated loops require S2b/S3-V",
                stmt.span,
            ));
        }
        if let StmtKind::If {
            then_body,
            else_body,
            ..
        } = &stmt.kind
        {
            reject_unsupported(then_body)?;
            reject_unsupported(else_body)?;
        }
        if stmt_has_memory(stmt) {
            return Err(error(
                "CGR009",
                "memory execution is unsupported",
                stmt.span,
            ));
        }
        if let StmtKind::For { iter, body, .. } = &stmt.kind {
            let valid = !matches!(
                iter.kind,
                ExprKind::Integer(_) | ExprKind::Boolean(_) | ExprKind::Binary { .. }
            );
            if !valid {
                return Err(error(
                    "CGR009",
                    "only range(start, end) loops execute in the scalar preview",
                    stmt.span,
                ));
            }
            reject_unsupported(body)?;
        }
    }
    Ok(())
}

pub(crate) fn stmt_has_memory(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Match { scrutinee, arms } => {
            expr_has_memory(scrutinee)
                || arms.iter().any(|arm| arm.body.iter().any(stmt_has_memory))
        }
        StmtKind::LoopContract { expression, .. } => expr_has_memory(expression),
        StmtKind::If {
            condition,
            then_body,
            else_body,
        } => expr_has_memory(condition) || then_body.iter().chain(else_body).any(stmt_has_memory),
        StmtKind::Let { value, .. } | StmtKind::Assign { value, .. } | StmtKind::Expr(value) => {
            expr_has_memory(value)
        }
        StmtKind::Return(Some(value)) => expr_has_memory(value),
        StmtKind::For { iter, body, .. } => {
            expr_has_memory(iter) || body.iter().any(stmt_has_memory)
        }
        StmtKind::NoGc { .. } => true,
        StmtKind::Return(None) => false,
    }
}

pub(crate) fn stmt_has_for(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::If {
            then_body,
            else_body,
            ..
        } => then_body.iter().chain(else_body).any(stmt_has_for),
        StmtKind::For { iter, body, .. } => {
            matches!(&iter.kind, ExprKind::Call { callee, .. }
                if matches!(&callee.kind, ExprKind::Name(path) if path.segments.as_slice() == ["range"]))
                || body.iter().any(stmt_has_for)
        }
        StmtKind::NoGc { body, .. } => body.iter().any(stmt_has_for),
        _ => false,
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

fn validate_type(ty: &TypeSyntax) -> Result<(), RunError> {
    if ty.arguments.is_empty()
        || matches!(
            ty.path.segments.join(".").as_str(),
            "Option" | "Result" | "List" | "Array"
        )
    {
        for argument in &ty.arguments {
            validate_type(argument)?;
        }
        Ok(())
    } else {
        Err(error(
            "CGR009",
            format!("type {ty} is unsupported in this execution profile"),
            ty.span,
        ))
    }
}

fn check_type(module: &Module, ty: &TypeSyntax, value: &Value) -> Result<(), RunError> {
    validate_type(ty)?;
    let resolved = cgx2_sema::resolve_type(module, &Default::default(), ty)
        .map_err(|_| error("CGR004", format!("unknown declared type {ty}"), ty.span))?;
    let valid = match (&resolved, value) {
        (cgx2_sema::Type::I64, Value::I64(_))
        | (cgx2_sema::Type::Bool, Value::Bool(_))
        | (cgx2_sema::Type::String, Value::String(_))
        | (cgx2_sema::Type::Unit, Value::Unit) => true,
        (cgx2_sema::Type::Record(expected), Value::Record { name, .. }) => expected == name,
        (cgx2_sema::Type::Variant { .. }, Value::Variant { ty: actual, .. }) => actual == &resolved,
        (cgx2_sema::Type::List(_) | cgx2_sema::Type::Array(_), Value::Sequence { ty, .. }) => {
            ty == &resolved
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(error(
            "CGR004",
            format!("value does not match declared type {ty}"),
            ty.span,
        ))
    }
}

fn expect_arity(name: &str, expected: usize, actual: usize, span: Span) -> Result<(), RunError> {
    if expected == actual {
        Ok(())
    } else {
        Err(error(
            "CGR012",
            format!("{name} expects {expected} argument(s), got {actual}"),
            span,
        ))
    }
}

fn single_name(segments: &[String], span: Span) -> Result<&str, RunError> {
    if segments.len() == 1 {
        Ok(&segments[0])
    } else {
        Err(error(
            "CGR009",
            "qualified names are unsupported in the execution preview",
            span,
        ))
    }
}

fn binary_values(left: &Value, right: &Value, op: BinaryOp, span: Span) -> Result<Value, RunError> {
    let (Value::I64(left), Value::I64(right)) = (left, right) else {
        return Err(error(
            "CGR004",
            "arithmetic operands must both be i64",
            span,
        ));
    };
    let result = match op {
        BinaryOp::Add => left.checked_add(*right),
        BinaryOp::Subtract => left.checked_sub(*right),
        BinaryOp::Multiply => left.checked_mul(*right),
        BinaryOp::Divide if *right == 0 => return Err(error("CGR006", "division by zero", span)),
        BinaryOp::Divide => left.checked_div(*right),
    };
    result
        .map(Value::I64)
        .ok_or_else(|| error("CGR005", "i64 arithmetic overflow", span))
}

fn comparison_values(name: &str, values: &[Value], span: Span) -> Result<Value, RunError> {
    expect_arity(name, 2, values.len(), span)?;
    if matches!(name, "$cgx2_cmp_eq" | "$cgx2_cmp_ne") {
        return Ok(Value::Bool(if name == "$cgx2_cmp_eq" {
            values[0] == values[1]
        } else {
            values[0] != values[1]
        }));
    }
    let outcome = match (&values[0], &values[1]) {
        (Value::I64(a), Value::I64(b)) => match name {
            "$cgx2_cmp_eq" => a == b,
            "$cgx2_cmp_ne" => a != b,
            "$cgx2_cmp_lt" => a < b,
            "$cgx2_cmp_le" => a <= b,
            "$cgx2_cmp_gt" => a > b,
            "$cgx2_cmp_ge" => a >= b,
            _ => return Err(error("CGR009", "unknown comparison", span)),
        },
        (Value::Bool(a), Value::Bool(b)) => match name {
            "$cgx2_cmp_eq" => a == b,
            "$cgx2_cmp_ne" => a != b,
            _ => {
                return Err(error(
                    "CGR004",
                    "ordered comparison requires i64 operands",
                    span,
                ))
            }
        },
        _ => {
            return Err(error(
                "CGR004",
                "comparison requires compatible scalar operands",
                span,
            ))
        }
    };
    Ok(Value::Bool(outcome))
}

fn decode_string(raw: &str, span: Span) -> Result<String, RunError> {
    let mut result = String::new();
    let mut chars = raw[1..raw.len() - 1].chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        result.push(match chars.next() {
            Some('n') => '\n',
            Some('r') => '\r',
            Some('t') => '\t',
            Some('\\') => '\\',
            Some('"') => '"',
            _ => return Err(error("CGR009", "unsupported string escape", span)),
        });
    }
    Ok(result)
}

include!("data_eval.rs");

#[cfg(test)]
#[path = "output_tests.rs"]
mod output_tests;
