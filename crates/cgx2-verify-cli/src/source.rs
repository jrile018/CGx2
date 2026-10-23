//! Conservative lowering of checked CGx2 source contracts to finite VCs.
use cgx2_sema::check_module;
use cgx2_syntax::{parse_module, BinaryOp, Expr as SourceExpr, ExprKind, Function, Span, StmtKind};
use cgx2_verify_core::{AuditError, Contract, Domain, EvalError, Expr, Parameter, Sort, Value};
use serde_json::{json, Value as Json};
use std::collections::BTreeMap;

const SOURCE_NODE_LIMIT: usize = 1024;
const SOURCE_CONDITIONAL_DEPTH_LIMIT: usize = 32;

#[derive(Debug)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    fn unsupported(message: impl Into<String>, span: Span) -> Self {
        Self {
            code: "CGV001".into(),
            message: message.into(),
            span,
        }
    }

    pub fn json(&self) -> Json {
        json!({"code": self.code, "message": self.message,
            "span": {"start": self.span.start, "end": self.span.end}})
    }
}

pub struct Selected {
    pub contract: Contract,
    pub domains: Vec<Domain>,
    pub name: String,
    pub span: Span,
    pub obligations: Vec<Json>,
    source_function: Function,
}

fn source_i64(value: Value) -> Result<i64, EvalError> {
    match value {
        Value::I64(value) => Ok(value),
        Value::Bool(_) => Err(EvalError::SortMismatch),
    }
}

/// Evaluates the parsed CGx2 expression directly, without using the lowered
/// verifier Expr or its evaluator. The parser represents comparisons as
/// `$cgx2_cmp_*` calls; ordinary calls are outside this source subset.
fn eval_source(
    expression: &SourceExpr,
    values: &BTreeMap<String, Value>,
) -> Result<Value, EvalError> {
    match &expression.kind {
        ExprKind::Boolean(value) => Ok(Value::Bool(*value)),
        ExprKind::Integer(value) => value
            .parse::<i64>()
            .map(Value::I64)
            .map_err(|_| EvalError::Overflow),
        ExprKind::Name(path) if path.segments.len() == 1 => values
            .get(&path.segments[0])
            .cloned()
            .ok_or_else(|| EvalError::UnknownVariable(path.segments[0].clone())),
        ExprKind::Binary { left, op, right } => {
            let left = source_i64(eval_source(left, values)?)?;
            let right = source_i64(eval_source(right, values)?)?;
            let result = match op {
                BinaryOp::Add => left.checked_add(right),
                BinaryOp::Subtract => left.checked_sub(right),
                _ => return Err(EvalError::Unsupported("source binary operation".into())),
            };
            result.map(Value::I64).ok_or(EvalError::Overflow)
        }
        ExprKind::Call { callee, arguments } => {
            let ExprKind::Name(path) = &callee.kind else {
                return Err(EvalError::Unsupported("call".into()));
            };
            let [name] = path.segments.as_slice() else {
                return Err(EvalError::Unsupported("call".into()));
            };
            if arguments.len() != 2 || arguments.iter().any(|arg| arg.name.is_some()) {
                return Err(EvalError::Unsupported("call arguments".into()));
            }
            let left = eval_source(&arguments[0].value, values)?;
            let right = eval_source(&arguments[1].value, values)?;
            let answer = match name.as_str() {
                "$cgx2_cmp_eq" => left == right,
                "$cgx2_cmp_ne" => left != right,
                "$cgx2_cmp_lt" => source_i64(left)? < source_i64(right)?,
                "$cgx2_cmp_le" => source_i64(left)? <= source_i64(right)?,
                "$cgx2_cmp_gt" => source_i64(left)? > source_i64(right)?,
                "$cgx2_cmp_ge" => source_i64(left)? >= source_i64(right)?,
                _ => return Err(EvalError::Unsupported(name.clone())),
            };
            Ok(Value::Bool(answer))
        }
        _ => Err(EvalError::Unsupported("source expression".into())),
    }
}

// This oracle executes the original statements, independently of lowering and core evaluation.
fn eval_source_body(
    body: &[cgx2_syntax::Stmt],
    values: &BTreeMap<String, Value>,
) -> Result<Value, EvalError> {
    let [statement] = body else {
        return Err(EvalError::Unsupported("source body".into()));
    };
    match &statement.kind {
        StmtKind::Expr(expression) | StmtKind::Return(Some(expression)) => {
            eval_source(expression, values)
        }
        StmtKind::If {
            condition,
            then_body,
            else_body,
        } => match eval_source(condition, values)? {
            Value::Bool(true) => eval_source_body(then_body, values),
            Value::Bool(false) => eval_source_body(else_body, values),
            _ => Err(EvalError::SortMismatch),
        },
        _ => Err(EvalError::Unsupported("source body".into())),
    }
}

impl Selected {
    pub fn source_obligations(
        &self,
        values: &BTreeMap<String, Value>,
    ) -> Result<[bool; 4], AuditError> {
        let pre = eval_source(&self.source_function.requires[0], values);
        let undefined = pre == Err(EvalError::Overflow);
        let admitted = pre == Ok(Value::Bool(true));
        if let Err(error) = &pre {
            if *error != EvalError::Overflow {
                return Err(AuditError::Unsupported(format!(
                    "source requires: {error:?}"
                )));
            }
        }
        let mut false_post = false;
        let mut overflow = false;
        if admitted {
            let body = eval_source_body(&self.source_function.body, values);
            match body {
                Ok(result) => {
                    let mut post_values = values.clone();
                    post_values.insert("result".into(), result);
                    match eval_source(&self.source_function.ensures[0], &post_values) {
                        Ok(Value::Bool(false)) => false_post = true,
                        Ok(Value::Bool(true)) => (),
                        Err(EvalError::Overflow) => overflow = true,
                        other => {
                            return Err(AuditError::Unsupported(format!(
                                "source ensures: {other:?}"
                            )))
                        }
                    }
                }
                Err(EvalError::Overflow) => overflow = true,
                Err(error) => {
                    return Err(AuditError::Unsupported(format!("source body: {error:?}")))
                }
            }
        }
        Ok([undefined, admitted, false_post, overflow])
    }
}

fn lower(
    expression: &SourceExpr,
    result: Option<&Expr>,
    nodes: &mut usize,
) -> Result<Expr, Diagnostic> {
    *nodes += 1;
    if *nodes > SOURCE_NODE_LIMIT {
        return Err(Diagnostic::unsupported(
            "source expression exceeds verification node limit",
            expression.span,
        ));
    }
    let pair = |left: &SourceExpr, right: &SourceExpr, nodes: &mut usize| {
        Ok::<_, Diagnostic>((
            Box::new(lower(left, result, nodes)?),
            Box::new(lower(right, result, nodes)?),
        ))
    };
    match &expression.kind {
        ExprKind::Boolean(value) => Ok(Expr::Bool(*value)),
        ExprKind::Integer(value) => value
            .parse::<i64>()
            .map(Expr::I64)
            .map_err(|_| Diagnostic::unsupported("integer literal outside i64", expression.span)),
        ExprKind::Name(path) if path.segments.len() == 1 => {
            let name = &path.segments[0];
            if name == "result" {
                result.cloned().ok_or_else(|| {
                    Diagnostic::unsupported(
                        "`result` is only available in ensures",
                        expression.span,
                    )
                })
            } else {
                Ok(Expr::Var(name.clone()))
            }
        }
        ExprKind::Binary { left, op, right } => {
            let (left, right) = pair(left, right, nodes)?;
            match op {
                BinaryOp::Add => Ok(Expr::Add(left, right)),
                BinaryOp::Subtract => Ok(Expr::Sub(left, right)),
                _ => Err(Diagnostic::unsupported(
                    "only checked i64 addition and subtraction are modeled",
                    expression.span,
                )),
            }
        }
        ExprKind::Call { callee, arguments } => {
            let ExprKind::Name(path) = &callee.kind else {
                return Err(Diagnostic::unsupported(
                    "calls are outside the pure contract subset",
                    expression.span,
                ));
            };
            let [operator] = path.segments.as_slice() else {
                return Err(Diagnostic::unsupported(
                    "calls are outside the pure contract subset",
                    expression.span,
                ));
            };
            if arguments.len() != 2 || arguments.iter().any(|arg| arg.name.is_some()) {
                return Err(Diagnostic::unsupported(
                    "comparison requires two positional operands",
                    expression.span,
                ));
            }
            if !operator.starts_with("$cgx2_cmp_") {
                return Err(Diagnostic::unsupported(
                    "calls are outside the pure contract subset",
                    expression.span,
                ));
            }
            let (left, right) = pair(&arguments[0].value, &arguments[1].value, nodes)?;
            match operator.as_str() {
                "$cgx2_cmp_eq" => Ok(Expr::Eq(left, right)),
                "$cgx2_cmp_ne" => Ok(Expr::Not(Box::new(Expr::Eq(left, right)))),
                "$cgx2_cmp_lt" => Ok(Expr::Lt(left, right)),
                "$cgx2_cmp_le" => Ok(Expr::Le(left, right)),
                "$cgx2_cmp_gt" => Ok(Expr::Gt(left, right)),
                "$cgx2_cmp_ge" => Ok(Expr::Ge(left, right)),
                _ => Err(Diagnostic::unsupported(
                    "unknown comparison",
                    expression.span,
                )),
            }
        }
        _ => Err(Diagnostic::unsupported(
            "expression is outside the checked scalar subset",
            expression.span,
        )),
    }
}

fn lower_body(
    body: &[cgx2_syntax::Stmt],
    depth: usize,
    nodes: &mut usize,
    span: Span,
) -> Result<Expr, Diagnostic> {
    let [statement] = body else {
        return Err(Diagnostic::unsupported(
            "body must contain exactly one return or conditional",
            span,
        ));
    };
    // Keep the legacy single-expression/return node accounting unchanged.
    if depth != 0 || matches!(statement.kind, StmtKind::If { .. }) {
        *nodes += 1;
    }
    if *nodes > SOURCE_NODE_LIMIT {
        return Err(Diagnostic::unsupported(
            "source body exceeds verification node limit",
            statement.span,
        ));
    }
    match &statement.kind {
        StmtKind::Expr(expression) if depth == 0 => lower(expression, None, nodes),
        StmtKind::Return(Some(expression)) => lower(expression, None, nodes),
        StmtKind::If {
            condition,
            then_body,
            else_body,
        } => {
            if depth >= SOURCE_CONDITIONAL_DEPTH_LIMIT {
                return Err(Diagnostic::unsupported(
                    "conditional nesting exceeds verification depth limit (32)",
                    statement.span,
                ));
            }
            Ok(Expr::If(
                Box::new(lower(condition, None, nodes)?),
                Box::new(lower_body(then_body, depth + 1, nodes, statement.span)?),
                Box::new(lower_body(else_body, depth + 1, nodes, statement.span)?),
            ))
        }
        _ => Err(Diagnostic::unsupported(
            "only explicit scalar returns or complete conditional trees are supported",
            statement.span,
        )),
    }
}

fn select<'a>(
    functions: &'a [Function],
    requested: Option<&str>,
    span: Span,
) -> Result<&'a Function, Diagnostic> {
    match requested {
        Some(name) => functions
            .iter()
            .find(|function| function.name == name)
            .ok_or_else(|| Diagnostic::unsupported(format!("function `{name}` not found"), span)),
        None if functions.len() == 1 => Ok(&functions[0]),
        None => Err(Diagnostic::unsupported(
            "select exactly one function with --function NAME",
            span,
        )),
    }
}

pub fn parse(
    source: &str,
    requested: Option<&str>,
    domains: Vec<Domain>,
) -> Result<Selected, Diagnostic> {
    let module = parse_module(source).map_err(|errors| {
        let error = &errors[0];
        Diagnostic {
            code: error.code.into(),
            message: error.message.clone(),
            span: error.span,
        }
    })?;
    let function = select(&module.functions, requested, module.span)?;
    if function.foreign.is_some() {
        return Err(Diagnostic::unsupported(
            "foreign declarations cannot be verified",
            function.span,
        ));
    }
    if !function.public {
        return Err(Diagnostic::unsupported(
            "verified function must be public",
            function.span,
        ));
    }
    if function
        .effects
        .as_ref()
        .is_none_or(|effects| !effects.names.is_empty())
    {
        return Err(Diagnostic::unsupported(
            "verified function requires explicit effects()",
            function.span,
        ));
    }
    if function.requires.len() != 1 || function.ensures.len() != 1 {
        return Err(Diagnostic::unsupported(
            "exactly one explicit requires and one ensures are required",
            function.span,
        ));
    }
    if let Some(statement) = function.body.iter().find(|statement| {
        matches!(&statement.kind,
            StmtKind::Expr(SourceExpr { kind: ExprKind::Call { callee, .. }, .. })
                if matches!(&callee.kind, ExprKind::Name(path)
                    if path.segments.as_slice() == ["assert"]))
    }) {
        return Err(Diagnostic::unsupported(
            "assert call is not a checked CGx2 source statement in this verifier fragment",
            statement.span,
        ));
    }
    let mut nodes = 0;
    let body = lower_body(&function.body, 0, &mut nodes, function.span)?;
    // Validate both branches before source checking, including statically inactive branches.
    let sorts = function
        .parameters
        .iter()
        .filter_map(|parameter| {
            let sort = match parameter.ty.to_string().as_str() {
                "i64" => Sort::I64,
                "Bool" => Sort::Bool,
                _ => return None,
            };
            Some((parameter.name.as_str(), sort))
        })
        .collect();
    if matches!(body, Expr::If(..)) {
        body.sort(&sorts).map_err(|error| {
            Diagnostic::unsupported(
                format!("conditional body invalid: {error:?}"),
                function.body[0].span,
            )
        })?;
    }
    check_module(&module).map_err(|errors| {
        let error = &errors[0];
        Diagnostic {
            code: error.code.into(),
            message: error.message.clone(),
            span: error.span,
        }
    })?;
    let mut parameters = Vec::new();
    for parameter in &function.parameters {
        let sort = match parameter.ty.to_string().as_str() {
            "i64" => Sort::I64,
            "Bool" => Sort::Bool,
            _ => {
                return Err(Diagnostic::unsupported(
                    "only i64 and Bool parameters are modeled",
                    parameter.span,
                ))
            }
        };
        parameters.push(Parameter::new(&parameter.name, sort));
    }
    if !matches!(
        function
            .return_type
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("i64" | "Bool")
    ) {
        return Err(Diagnostic::unsupported(
            "return type must be i64 or Bool",
            function.span,
        ));
    }
    let body_span = function.body[0].span;
    let requires = lower(&function.requires[0], None, &mut nodes)?;
    let postconditions = lower(&function.ensures[0], Some(&body), &mut nodes)?;
    // Evaluating the body is required even when the postcondition never reads `result`.
    // The equality is true for every defined pure scalar body and propagates overflow.
    let body_defined = Expr::Eq(Box::new(body.clone()), Box::new(body));
    let ensures = Expr::And(Box::new(body_defined), Box::new(postconditions));
    let contract = Contract {
        parameters,
        requires,
        ensures,
    };
    contract.validate().map_err(|error| {
        Diagnostic::unsupported(
            format!("lowered contract invalid: {error:?}"),
            function.span,
        )
    })?;
    Ok(Selected {
        contract,
        domains,
        name: function.name.clone(),
        span: function.span,
        obligations: vec![
            json!({"kind": "requires_defined_and_satisfiable", "span": {"start": function.requires[0].span.start, "end": function.requires[0].span.end}}),
            json!({"kind": "body_defined", "span": {"start": body_span.start, "end": body_span.end}}),
            json!({"kind": "ensures", "span": {"start": function.ensures[0].span.start, "end": function.ensures[0].span.end}}),
        ],
        source_function: function.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgx2_verify_core::{audit_encoding, audit_encoding_with_source, emit_smtlib};

    #[test]
    fn original_source_oracle_rejects_corrupted_lowering_and_encoding() {
        let text = "pub fn choose(flag: Bool) -> i64 effects() caps():\n    requires true\n    ensures result == 1\n    if flag:\n        return 1\n    else:\n        return 2\n";
        let selected = parse(text, None, vec![]).unwrap();
        let script = emit_smtlib(&selected.contract).unwrap();
        let oracle = |values: &BTreeMap<String, Value>| selected.source_obligations(values);
        let audit =
            audit_encoding_with_source(&selected.contract, &[], 2, &script, &oracle).unwrap();
        assert_eq!(audit.obligations, [0, 2, 1, 0]);
        // A self-consistent but corrupted lowering still disagrees with original source.
        let mut corrupted = selected.contract.clone();
        corrupted.ensures = Expr::Bool(true);
        let corrupted_script = emit_smtlib(&corrupted).unwrap();
        assert!(audit_encoding(&corrupted, &[], 2, &corrupted_script).is_ok());
        assert!(matches!(
            audit_encoding_with_source(&corrupted, &[], 2, &corrupted_script, &oracle),
            Err(AuditError::SourceMismatch { .. })
        ));
        // Swapping branches in emitted terms must fail even though each term parses.
        let corrupted_script = script.replace("(ite v_666c6167 1 2)", "(ite v_666c6167 2 1)");
        assert_ne!(script, corrupted_script);
        assert!(matches!(
            audit_encoding_with_source(&selected.contract, &[], 2, &corrupted_script, &oracle),
            Err(AuditError::Mismatch { .. })
        ));
    }

    #[test]
    fn original_source_replay_and_oracle_reject_unconditional_branch_definedness() {
        let selected = parse(
            include_str!("../examples/conditional-overflow.cg"),
            None,
            vec![Domain::new("x", i64::MAX, i64::MAX)],
        )
        .unwrap();
        let values = BTreeMap::from([
            ("x".into(), Value::I64(i64::MAX)),
            ("flag".into(), Value::Bool(true)),
        ]);
        assert_eq!(
            selected.source_obligations(&values).unwrap(),
            [false, true, false, false]
        );
        let mut active = values.clone();
        active.insert("flag".into(), Value::Bool(false));
        assert_eq!(
            selected.source_obligations(&active).unwrap(),
            [false, true, false, true]
        );
        let script = emit_smtlib(&selected.contract).unwrap();
        let overflow_defined = "(and true true (>= (+ v_78 1) (- 9223372036854775808)) (<= (+ v_78 1) 9223372036854775807))";
        let selected_defined = format!("(ite v_666c6167 true {overflow_defined})");
        let corrupted = script.replace(&selected_defined, overflow_defined);
        assert_ne!(script, corrupted);
        assert!(matches!(
            audit_encoding_with_source(
                &selected.contract,
                &selected.domains,
                2,
                &corrupted,
                &|values| selected.source_obligations(values)
            ),
            Err(AuditError::Mismatch { .. })
        ));
    }
}
