use std::{collections::HashMap, fmt};

use crate::RunError;
use cgx2_foreign::{registry_from_source, BridgeValue, PythonBridge, SourceCall, ValueShape};
use cgx2_syntax::{
    AssignOp, BinaryOp, Expr, ExprKind, Function, Module, Span, Stmt, StmtKind, TypeSyntax,
};

const STEP_LIMIT: usize = 1_000_000;
const DEPTH_LIMIT: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    I64(i64),
    Bool(bool),
    String(String),
    Record {
        name: String,
        fields: Vec<(String, Value)>,
    },
    Unit,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::I64(v) => write!(f, "{v}"),
            Self::Bool(v) => write!(f, "{v}"),
            Self::String(v) => f.write_str(v),
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
    module: &'a Module,
    args: &'a [String],
    steps: usize,
    depth: usize,
}

pub(crate) fn run_module(module: &Module, args: &[String]) -> Result<RunResult, RunError> {
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
        module,
        args,
        steps: 0,
        depth: 0,
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
    Ok(RunResult { value })
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
                StmtKind::Let {
                    mutable,
                    name,
                    annotation,
                    value,
                } => {
                    let value = self.eval_expr(value, scope)?;
                    if let Some(ty) = annotation {
                        check_type(ty, &value)?;
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
                        check_type(ty, &assigned)?;
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
        match &expr.kind {
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
                if let Some(record) = self
                    .module
                    .records
                    .iter()
                    .find(|record| record.name == name)
                {
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
                        name: record.name.clone(),
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
        let bridge = PythonBridge::new().map_err(|err| error("CGR017", err.to_string(), span))?;
        let values: Vec<_> = args
            .into_iter()
            .map(|value| match value {
                Value::I64(value) => BridgeValue::I64(value),
                Value::Bool(value) => BridgeValue::Bool(value),
                Value::String(value) => BridgeValue::String(value),
                Value::Record { .. } => BridgeValue::None,
                Value::Unit => BridgeValue::None,
            })
            .collect();
        let returned = registry
            .call(&bridge, &function.name, &values, &[])
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
        match returned {
            BridgeValue::I64(value) => Ok(Value::I64(value)),
            BridgeValue::Bool(value) => Ok(Value::Bool(value)),
            BridgeValue::String(value) => Ok(Value::String(value)),
            _ => Err(error("CGR021", "unsupported Python return shape", span)),
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
        let mut scope = Scope::new();
        for (param, value) in function.parameters.iter().zip(args) {
            check_type(&param.ty, &value)?;
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
                check_type(ty, &value)?;
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
        if stmt_has_memory(stmt) {
            return Err(error(
                "CGR009",
                "memory execution is unsupported",
                stmt.span,
            ));
        }
        if let StmtKind::For { iter, body, .. } = &stmt.kind {
            let valid = matches!(&iter.kind, ExprKind::Call { callee, arguments }
                if matches!(&callee.kind, ExprKind::Name(path) if path.segments.as_slice() == ["range"])
                && arguments.len() == 2 && arguments.iter().all(|arg| arg.name.is_none()));
            if !valid {
                return Err(error(
                    "CGR009",
                    "only range(start, end) loops execute in the scalar preview",
                    iter.span,
                ));
            }
            reject_unsupported(body)?;
        }
    }
    Ok(())
}

pub(crate) fn stmt_has_memory(stmt: &Stmt) -> bool {
    match &stmt.kind {
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
        ExprKind::Name(path) => path.segments.first().is_some_and(|s| s == "memory"),
        ExprKind::Call { callee, arguments } => {
            expr_has_memory(callee) || arguments.iter().any(|a| expr_has_memory(&a.value))
        }
        ExprKind::Binary { left, right, .. } => expr_has_memory(left) || expr_has_memory(right),
        _ => false,
    }
}

fn validate_type(ty: &TypeSyntax) -> Result<(), RunError> {
    if ty.arguments.is_empty() && ty.path.segments.len() == 1 {
        Ok(())
    } else {
        Err(error(
            "CGR009",
            format!("type {ty} is unsupported in the execution preview"),
            ty.span,
        ))
    }
}

fn check_type(ty: &TypeSyntax, value: &Value) -> Result<(), RunError> {
    validate_type(ty)?;
    let matches = matches!(
        (ty.path.segments[0].as_str(), value),
        ("i64", Value::I64(_))
            | ("Bool", Value::Bool(_))
            | ("String", Value::String(_))
            | ("Unit", Value::Unit)
    ) || matches!(value, Value::Record { name, .. } if name == &ty.path.segments[0]);
    if matches {
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
