//! Bounded scalar entry evaluator for the native C process. This module has no CPython dependency.
use std::collections::{BTreeMap, BTreeSet};

use cgx2_ffi_native::Function as NativeFunction;
use cgx2_syntax::{BinaryOp, Expr, ExprKind, Module, Span, StmtKind};

#[derive(Debug)]
pub struct NativeEvalError {
    pub code: &'static str,
    pub message: String,
    pub span: Option<Span>,
}

fn error(code: &'static str, message: impl Into<String>, span: Span) -> NativeEvalError {
    NativeEvalError {
        code,
        message: message.into(),
        span: Some(span),
    }
}

fn entry_expression(module: &Module) -> Result<&Expr, NativeEvalError> {
    if !module.statements.is_empty() {
        return Err(error(
            "CGN009",
            "native C package entry cannot contain script statements",
            module.statements[0].span,
        ));
    }
    let Some(main) = module
        .functions
        .iter()
        .find(|function| function.name == "main")
    else {
        return Err(error(
            "CGN009",
            "native C package entry requires main",
            module.span,
        ));
    };
    if main.foreign.is_some()
        || !main.parameters.is_empty()
        || main
            .return_type
            .as_ref()
            .is_none_or(|ty| ty.to_string() != "i64")
        || !main.requires.is_empty()
        || !main.ensures.is_empty()
        || main.body.len() != 1
    {
        return Err(error("CGN009", "native C main must be a zero-argument i64 function with one scalar expression and no contracts", main.span));
    }
    match &main.body[0].kind {
        StmtKind::If { .. } => Err(error(
            "CGN009",
            "conditional native foreign entry bodies are unsupported",
            main.body[0].span,
        )),
        StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) => Ok(expr),
        _ => Err(error(
            "CGN009",
            "native C main supports one scalar expression or return",
            main.body[0].span,
        )),
    }
}

fn validate_expr(expr: &Expr, names: &BTreeSet<&str>, depth: usize) -> Result<(), NativeEvalError> {
    if depth > 128 {
        return Err(error(
            "CGN009",
            "native expression nesting limit exceeded",
            expr.span,
        ));
    }
    match &expr.kind {
        ExprKind::Integer(_) => Ok(()),
        ExprKind::Binary { left, right, .. } => {
            validate_expr(left, names, depth + 1)?;
            validate_expr(right, names, depth + 1)
        }
        ExprKind::Call { callee, arguments } => {
            let ExprKind::Name(path) = &callee.kind else {
                return Err(error(
                    "CGN009",
                    "native call must name a declared C adapter",
                    callee.span,
                ));
            };
            if path.segments.len() != 1
                || !names.contains(path.segments[0].as_str())
                || arguments.len() != 1
                || arguments[0].name.is_some()
            {
                return Err(error(
                    "CGN009",
                    "native call requires one positional i64 argument to a declared adapter",
                    expr.span,
                ));
            }
            validate_expr(&arguments[0].value, names, depth + 1)
        }
        _ => Err(error(
            "CGN009",
            "native C entry supports only integer arithmetic and declared adapter calls",
            expr.span,
        )),
    }
}

/// Validate the bounded entry profile at both check and run.
pub fn validate(module: &Module) -> Result<(), NativeEvalError> {
    let names: BTreeSet<_> = module
        .functions
        .iter()
        .filter_map(|function| {
            function
                .foreign
                .as_ref()
                .filter(|binding| binding.route == "native")
                .map(|_| function.name.as_str())
        })
        .collect();
    for function in &module.functions {
        if function.name != "main" && !names.contains(function.name.as_str()) {
            return Err(error(
                "CGN009",
                "native C package contains an unsupported local or Python function",
                function.span,
            ));
        }
    }
    validate_expr(entry_expression(module)?, &names, 0)
}

fn eval_expr(
    expr: &Expr,
    adapters: &BTreeMap<String, NativeFunction>,
    depth: usize,
) -> Result<i64, NativeEvalError> {
    if depth > 128 {
        return Err(error(
            "CGN009",
            "native expression nesting limit exceeded",
            expr.span,
        ));
    }
    match &expr.kind {
        ExprKind::Integer(raw) => raw
            .replace('_', "")
            .parse::<i64>()
            .map_err(|_| error("CGR005", "integer literal exceeds i64 range", expr.span)),
        ExprKind::Binary { left, op, right } => {
            let left = eval_expr(left, adapters, depth + 1)?;
            let right = eval_expr(right, adapters, depth + 1)?;
            match op {
                BinaryOp::Add => left.checked_add(right),
                BinaryOp::Subtract => left.checked_sub(right),
                BinaryOp::Multiply => left.checked_mul(right),
                BinaryOp::Divide => left.checked_div(right),
            }
            .ok_or_else(|| {
                error(
                    "CGR006",
                    "integer arithmetic overflow or division by zero",
                    expr.span,
                )
            })
        }
        ExprKind::Call { callee, arguments } => {
            let ExprKind::Name(path) = &callee.kind else {
                unreachable!("validated native call")
            };
            let adapter = &adapters[&path.segments[0]];
            let input = eval_expr(&arguments[0].value, adapters, depth + 1)?;
            adapter.call_i64(input).map_err(|cause| {
                error(
                    "CGR023",
                    format!("native C call {} failed: {cause:?}", path.segments[0]),
                    expr.span,
                )
            })
        }
        _ => unreachable!("validated native expression"),
    }
}

pub fn run(
    module: &Module,
    args: &[String],
    adapters: &BTreeMap<String, NativeFunction>,
) -> Result<i64, NativeEvalError> {
    if !args.is_empty() {
        return Err(error(
            "CGN009",
            "native C package entry does not accept program arguments",
            module.span,
        ));
    }
    validate(module)?;
    eval_expr(entry_expression(module)?, adapters, 0)
}
