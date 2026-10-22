//! Deliberately lexical source profile: no tensor handle is representable in CGx2 values.
use cgx2_syntax::{Expr, ExprKind, Module, Span, StmtKind};

use crate::TypeDiagnostic;

pub const MAX_F64_VECTOR: usize = 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct TensorProfile {
    pub binding_id: String,
    pub version: String,
    pub left: Vec<f64>,
    pub right: Vec<f64>,
    pub call_span: Span,
    pub declaration_span: Span,
}

fn error(message: impl Into<String>, span: Span) -> Vec<TypeDiagnostic> {
    vec![TypeDiagnostic::new("CGT018", message.into(), span)]
}

fn vector(expr: &Expr) -> Result<Vec<f64>, Vec<TypeDiagnostic>> {
    let ExprKind::StringLiteral(raw) = &expr.kind else {
        return Err(error(
            "tensor input must be a literal float64 vector",
            expr.span,
        ));
    };
    let Some(inner) = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return Err(error(
            "tensor input must be a quoted float64 vector",
            expr.span,
        ));
    };
    let Some(inner) = inner.strip_prefix('[').and_then(|s| s.strip_suffix(']')) else {
        return Err(error(
            "tensor literal must have one dimension: \"[1.0, 2.0]\"",
            expr.span,
        ));
    };
    let parts: Vec<_> = inner.split(',').collect();
    if parts.is_empty() || parts.len() > MAX_F64_VECTOR || inner.trim().is_empty() {
        return Err(error(
            format!("tensor vector length must be 1..={MAX_F64_VECTOR}"),
            expr.span,
        ));
    }
    parts
        .into_iter()
        .map(|part| {
            let part = part.trim();
            if !part.contains(['.', 'e', 'E'])
                || !part
                    .bytes()
                    .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'+' | b'-' | b'e' | b'E'))
            {
                return Err(error(
                    "tensor elements must be finite float64 literals",
                    expr.span,
                ));
            }
            let value = part
                .parse::<f64>()
                .map_err(|_| error("invalid float64 literal", expr.span))?;
            if !value.is_finite() {
                return Err(error(
                    "NaN and Infinity are unsupported tensor elements",
                    expr.span,
                ));
            }
            Ok(value)
        })
        .collect()
}

/// Recognize and validate the entire bounded source profile before a native adapter is loaded.
pub fn tensor_profile(module: &Module) -> Result<Option<TensorProfile>, Vec<TypeDiagnostic>> {
    let tensor: Vec<_> = module
        .functions
        .iter()
        .filter(|function| {
            function.foreign.as_ref().is_some_and(|binding| {
                binding.route == "native"
                    && binding
                        .target
                        .segments
                        .get(1)
                        .is_some_and(|symbol| symbol == "cgx2_torch_add")
            })
        })
        .collect();
    if tensor.is_empty() {
        return Ok(None);
    }
    let foreign = tensor[0];
    let binding = foreign
        .foreign
        .as_ref()
        .expect("selected foreign declaration");
    if tensor.len() != 1
        || module.functions.len() != 2
        || !module.statements.is_empty()
        || !module.imports.is_empty()
    {
        return Err(error("tensor profile requires one native declaration and one local main; handles cannot escape", binding.span));
    }
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .ok_or_else(|| error("tensor profile requires local main", binding.span))?;
    if !binding.unsafe_call
        || binding.ownership != "copied"
        || binding.target.segments.len() != 2
        || !binding.runtime.starts_with("cgx2.libtorch.cpu@")
        || binding.runtime.ends_with('@')
        || foreign.parameters.len() != 2
        || foreign
            .parameters
            .iter()
            .any(|p| p.ty.to_string() != "String")
        || foreign
            .return_type
            .as_ref()
            .is_none_or(|ty| ty.to_string() != "String")
    {
        return Err(error("tensor declaration requires unsafe foreign native, (String, String) -> String, copied ownership, and cgx2.libtorch.cpu@VERSION", binding.span));
    }
    let declares = |function: &cgx2_syntax::Function| {
        function
            .effects
            .as_ref()
            .is_some_and(|d| d.names.iter().any(|n| n.name == "Foreign"))
            && function
                .capabilities
                .as_ref()
                .is_some_and(|d| d.names.iter().any(|n| n.name == "native"))
    };
    if !declares(foreign) || !declares(main) {
        return Err(error(
            "tensor declaration and main require effects(Foreign) caps(native)",
            main.span,
        ));
    }
    if !main.parameters.is_empty()
        || !main.requires.is_empty()
        || !main.ensures.is_empty()
        || main
            .return_type
            .as_ref()
            .is_none_or(|ty| ty.to_string() != "String")
        || main.body.len() != 1
    {
        return Err(error(
            "tensor main must have no parameters and one direct String result call",
            main.span,
        ));
    }
    let StmtKind::Expr(expr) = &main.body[0].kind else {
        return Err(error(
            "tensor result must be the only expression in main",
            main.body[0].span,
        ));
    };
    let ExprKind::Call { callee, arguments } = &expr.kind else {
        return Err(error(
            "tensor result must call the declared adapter",
            expr.span,
        ));
    };
    let ExprKind::Name(path) = &callee.kind else {
        return Err(error(
            "tensor result must call the declared adapter",
            callee.span,
        ));
    };
    if path.segments.as_slice() != [foreign.name.as_str()]
        || arguments.len() != 2
        || arguments.iter().any(|argument| argument.name.is_some())
    {
        return Err(error(
            "tensor call requires exactly two positional literal vectors",
            expr.span,
        ));
    }
    let left = vector(&arguments[0].value)?;
    let right = vector(&arguments[1].value)?;
    if left.len() != right.len() {
        return Err(error(
            "tensor addition requires equal one-dimensional shapes",
            expr.span,
        ));
    }
    Ok(Some(TensorProfile {
        binding_id: binding.target.segments[0].clone(),
        version: binding.runtime["cgx2.libtorch.cpu@".len()..].into(),
        left,
        right,
        call_span: expr.span,
        declaration_span: binding.span,
    }))
}
