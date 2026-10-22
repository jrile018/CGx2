//! Conservative lowering of checked CGx2 source contracts to finite VCs.
use cgx2_sema::check_module;
use cgx2_syntax::{parse_module, BinaryOp, Expr as SourceExpr, ExprKind, Function, Span, StmtKind};
use cgx2_verify_core::{Contract, Domain, Expr, Parameter, Sort};
use serde_json::{json, Value as Json};

const SOURCE_NODE_LIMIT: usize = 1024;

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
    let Some(body) = (match function.body.as_slice() {
        [statement] => match &statement.kind {
            StmtKind::Expr(expression) | StmtKind::Return(Some(expression)) => Some(expression),
            _ => None,
        },
        _ => None,
    }) else {
        return Err(Diagnostic::unsupported(
            "body must be one scalar expression or return expression",
            function.span,
        ));
    };
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
    let body_span = body.span;
    let mut nodes = 0;
    let body = lower(body, None, &mut nodes)?;
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
    })
}
