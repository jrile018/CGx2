//! Lexical source profile: foreign tensor handles cannot become CGx2 values.
use cgx2_syntax::{Expr, ExprKind, Module, Span, StmtKind};

use crate::TypeDiagnostic;

pub const MAX_F64_VECTOR: usize = 1024;
pub const MAX_MATRIX_DIMENSION: usize = 32;
pub const MAX_MATRIX_ELEMENTS: usize = 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct MatrixLiteral {
    pub rows: usize,
    pub cols: usize,
    pub values: Vec<f64>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct MatrixLiteralF32 {
    pub rows: usize,
    pub cols: usize,
    pub values: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TensorOperation {
    VectorAdd {
        left: Vec<f64>,
        right: Vec<f64>,
    },
    MatrixMultiply {
        left: MatrixLiteral,
        right: MatrixLiteral,
    },
    MatrixMultiplyF32 {
        left: MatrixLiteralF32,
        right: MatrixLiteralF32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct TensorProfile {
    pub binding_id: String,
    pub version: String,
    pub operation: TensorOperation,
    pub call_span: Span,
    pub declaration_span: Span,
}

fn error(message: impl Into<String>, span: Span) -> Vec<TypeDiagnostic> {
    vec![TypeDiagnostic::new("CGT018", message.into(), span)]
}

fn quoted(expr: &Expr) -> Result<&str, Vec<TypeDiagnostic>> {
    let ExprKind::StringLiteral(raw) = &expr.kind else {
        return Err(error("tensor input must be a quoted literal", expr.span));
    };
    raw.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or_else(|| error("tensor input must be a quoted literal", expr.span))
}

fn float64(part: &str, span: Span) -> Result<f64, Vec<TypeDiagnostic>> {
    let part = part.trim();
    if !part.contains(['.', 'e', 'E'])
        || !part
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'+' | b'-' | b'e' | b'E'))
    {
        return Err(error(
            "tensor elements must be finite float64 literals",
            span,
        ));
    }
    let value = part
        .parse::<f64>()
        .map_err(|_| error("invalid float64 literal", span))?;
    if !value.is_finite() {
        return Err(error(
            "NaN and Infinity are unsupported tensor elements",
            span,
        ));
    }
    Ok(value)
}

fn vector(expr: &Expr) -> Result<Vec<f64>, Vec<TypeDiagnostic>> {
    let raw = quoted(expr)?;
    let inner = raw
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| {
            error(
                "tensor literal must be one dimension: \"[1.0, 2.0]\"",
                expr.span,
            )
        })?;
    let parts: Vec<_> = inner.split(',').collect();
    if parts.is_empty() || parts.len() > MAX_F64_VECTOR || inner.trim().is_empty() {
        return Err(error(
            format!("tensor vector length must be 1..={MAX_F64_VECTOR}"),
            expr.span,
        ));
    }
    parts
        .into_iter()
        .map(|part| float64(part, expr.span))
        .collect()
}

fn matrix(expr: &Expr) -> Result<MatrixLiteral, Vec<TypeDiagnostic>> {
    let raw = quoted(expr)?;
    let mut remaining = raw
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| error("matrix literal must be nested row-major arrays", expr.span))?;
    let mut rows = 0usize;
    let mut cols = None;
    let mut values = Vec::new();
    loop {
        remaining = remaining.trim_start();
        let row = remaining
            .strip_prefix('[')
            .and_then(|s| s.split_once(']'))
            .ok_or_else(|| error("matrix literal must contain rows in brackets", expr.span))?;
        if row.0.contains('[') {
            return Err(error("matrix literal must have rank two", expr.span));
        }
        let elements: Vec<_> = row.0.split(',').collect();
        if elements.is_empty() || elements.len() > MAX_MATRIX_DIMENSION || row.0.trim().is_empty() {
            return Err(error("matrix columns must be 1..=32", expr.span));
        }
        if cols.is_some_and(|width| width != elements.len()) {
            return Err(error("matrix rows must have equal lengths", expr.span));
        }
        cols = Some(elements.len());
        rows += 1;
        if rows > MAX_MATRIX_DIMENSION || rows * elements.len() > MAX_MATRIX_ELEMENTS {
            return Err(error(
                "matrix size exceeds 32 rows or 1,024 elements",
                expr.span,
            ));
        }
        for element in elements {
            values.push(float64(element, expr.span)?);
        }
        remaining = row.1.trim_start();
        if remaining.is_empty() {
            break;
        }
        remaining = remaining
            .strip_prefix(',')
            .ok_or_else(|| error("matrix rows must be comma-separated", expr.span))?;
        if remaining.trim().is_empty() {
            return Err(error("matrix literal has a trailing comma", expr.span));
        }
    }
    Ok(MatrixLiteral {
        rows,
        cols: cols.expect("a parsed matrix has at least one row"),
        values,
    })
}

fn matrix_f32(expr: &Expr) -> Result<MatrixLiteralF32, Vec<TypeDiagnostic>> {
    let matrix = matrix(expr)?;
    let values = matrix
        .values
        .into_iter()
        .map(|value| {
            // Rust's f64-to-f32 cast uses IEEE round-to-nearest, ties-to-even.
            let narrowed = value as f32;
            if !narrowed.is_finite() {
                return Err(error(
                    "float32 matrix element overflows to Infinity",
                    expr.span,
                ));
            }
            if value != 0.0 && narrowed == 0.0 {
                return Err(error(
                    "float32 matrix element underflows to zero",
                    expr.span,
                ));
            }
            Ok(narrowed)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(MatrixLiteralF32 {
        rows: matrix.rows,
        cols: matrix.cols,
        values,
    })
}

/// Recognize and check the entire bounded source profile before loading native code.
pub fn tensor_profile(module: &Module) -> Result<Option<TensorProfile>, Vec<TypeDiagnostic>> {
    let tensor: Vec<_> = module
        .functions
        .iter()
        .filter(|function| {
            function.foreign.as_ref().is_some_and(|binding| {
                binding.route == "native"
                    && binding.target.segments.get(1).is_some_and(|symbol| {
                        symbol == "cgx2_torch_add"
                            || symbol == "cgx2_torch_matmul"
                            || symbol == "cgx2_torch_matmul_f32"
                    })
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
        return Err(error(
            "tensor profile requires one native declaration and one local main; handles cannot escape",
            binding.span,
        ));
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
        return Err(error(
            "tensor declaration requires unsafe foreign native, (String, String) -> String, copied ownership, and cgx2.libtorch.cpu@VERSION",
            binding.span,
        ));
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
        return Err(error("tensor result must call declared adapter", expr.span));
    };
    let ExprKind::Name(path) = &callee.kind else {
        return Err(error(
            "tensor result must call declared adapter",
            callee.span,
        ));
    };
    if path.segments.as_slice() != [foreign.name.as_str()]
        || arguments.len() != 2
        || arguments.iter().any(|argument| argument.name.is_some())
    {
        return Err(error(
            "tensor call requires exactly two positional literal inputs",
            expr.span,
        ));
    }
    let operation = if binding.target.segments[1] == "cgx2_torch_matmul_f32" {
        let left = matrix_f32(&arguments[0].value)?;
        let right = matrix_f32(&arguments[1].value)?;
        if left.cols != right.rows {
            return Err(error(
                "matrix multiplication requires left columns equal right rows",
                expr.span,
            ));
        }
        if left.rows * right.cols > MAX_MATRIX_ELEMENTS {
            return Err(error(
                "matrix output capacity exceeds 1,024 elements",
                expr.span,
            ));
        }
        TensorOperation::MatrixMultiplyF32 { left, right }
    } else if binding.target.segments[1] == "cgx2_torch_matmul" {
        let left = matrix(&arguments[0].value)?;
        let right = matrix(&arguments[1].value)?;
        if left.cols != right.rows {
            return Err(error(
                "matrix multiplication requires left columns equal right rows",
                expr.span,
            ));
        }
        if left.rows * right.cols > MAX_MATRIX_ELEMENTS {
            return Err(error(
                "matrix output capacity exceeds 1,024 elements",
                expr.span,
            ));
        }
        TensorOperation::MatrixMultiply { left, right }
    } else {
        let left = vector(&arguments[0].value)?;
        let right = vector(&arguments[1].value)?;
        if left.len() != right.len() {
            return Err(error(
                "tensor addition requires equal one-dimensional shapes",
                expr.span,
            ));
        }
        TensorOperation::VectorAdd { left, right }
    };
    Ok(Some(TensorProfile {
        binding_id: binding.target.segments[0].clone(),
        version: binding.runtime["cgx2.libtorch.cpu@".len()..].into(),
        operation,
        call_span: expr.span,
        declaration_span: binding.span,
    }))
}
