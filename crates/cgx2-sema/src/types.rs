#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    I64,
    Bool,
    String,
    Unit,
    List(Box<Type>),
    Record(String),
    AppliedRecord {
        name: String,
        arguments: Vec<Type>,
    },
    Parameter {
        name: String,
        constraints: Vec<String>,
    },
    Array(Box<Type>),
    Variant {
        name: String,
        arguments: Vec<Type>,
    },
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::I64 => write!(f, "i64"),
            Self::Bool => write!(f, "Bool"),
            Self::String => write!(f, "String"),
            Self::Unit => write!(f, "Unit"),
            Self::List(inner) => write!(f, "List<{inner}>"),
            Self::Array(inner) => write!(f, "Array<{inner}>"),
            Self::Parameter { name, .. } => f.write_str(name.rsplit("::").next().unwrap_or(name)),
            Self::Variant { name, arguments } | Self::AppliedRecord { name, arguments } => {
                write!(f, "{name}")?;
                if !arguments.is_empty() {
                    write!(f, "<")?;
                    for (i, ty) in arguments.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{ty}")?;
                    }
                    write!(f, ">")?;
                }
                Ok(())
            }
            Self::Record(name) => write!(f, "{name}"),
        }
    }
}
