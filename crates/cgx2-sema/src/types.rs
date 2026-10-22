use cgx2_syntax::TypeSyntax;

use crate::TypeDiagnostic;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    I64,
    Bool,
    String,
    Unit,
    List(Box<Type>),
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::I64 => write!(f, "i64"),
            Self::Bool => write!(f, "Bool"),
            Self::String => write!(f, "String"),
            Self::Unit => write!(f, "Unit"),
            Self::List(inner) => write!(f, "List<{inner}>"),
        }
    }
}

pub(crate) fn resolve(ty: &TypeSyntax, diagnostics: &mut Vec<TypeDiagnostic>) -> Option<Type> {
    let name = ty.path.segments.as_slice();
    let resolved = match (name, ty.arguments.as_slice()) {
        ([name], []) if name == "i64" => Some(Type::I64),
        ([name], []) if name == "Bool" => Some(Type::Bool),
        ([name], []) if name == "String" => Some(Type::String),
        ([name], []) if name == "Unit" => Some(Type::Unit),
        ([name], [inner]) if name == "List" => {
            resolve(inner, diagnostics).map(|t| Type::List(Box::new(t)))
        }
        _ => None,
    };
    if resolved.is_none() && !(name == ["List"] && ty.arguments.len() == 1) {
        diagnostics.push(TypeDiagnostic::new(
            "CGT002",
            format!("unknown or unsupported type `{ty}`"),
            ty.span,
        ));
    }
    resolved
}
