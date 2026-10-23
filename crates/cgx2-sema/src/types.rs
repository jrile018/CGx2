use cgx2_syntax::{Record, TypeSyntax};

use crate::TypeDiagnostic;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    I64,
    Bool,
    String,
    Unit,
    List(Box<Type>),
    Record(String),
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::I64 => write!(f, "i64"),
            Self::Bool => write!(f, "Bool"),
            Self::String => write!(f, "String"),
            Self::Unit => write!(f, "Unit"),
            Self::List(inner) => write!(f, "List<{inner}>"),
            Self::Record(name) => write!(f, "{name}"),
        }
    }
}

pub(crate) fn resolve(
    ty: &TypeSyntax,
    records: &[Record],
    diagnostics: &mut Vec<TypeDiagnostic>,
) -> Option<Type> {
    let name = ty.path.segments.as_slice();
    let resolved = match (name, ty.arguments.as_slice()) {
        ([name], []) if name == "i64" => Some(Type::I64),
        ([name], []) if name == "Bool" => Some(Type::Bool),
        ([name], []) if name == "String" => Some(Type::String),
        ([name], []) if name == "Unit" => Some(Type::Unit),
        ([name], [inner]) if name == "List" => {
            resolve(inner, records, diagnostics).and_then(|inner_ty| {
                if matches!(inner_ty, Type::Record(_)) {
                    diagnostics.push(TypeDiagnostic::new(
                        "CGT023",
                        "record values in generic containers are unsupported".into(),
                        ty.span,
                    ));
                    None
                } else {
                    Some(Type::List(Box::new(inner_ty)))
                }
            })
        }
        ([name], []) if records.iter().any(|record| &record.name == name) => {
            Some(Type::Record(name.clone()))
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
