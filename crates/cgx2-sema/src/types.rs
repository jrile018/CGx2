#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    I64,
    Bool,
    String,
    Unit,
    List(Box<Type>),
    Record(String),
    Variant { name: String, arguments: Vec<Type> },
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::I64 => write!(f, "i64"),
            Self::Bool => write!(f, "Bool"),
            Self::String => write!(f, "String"),
            Self::Unit => write!(f, "Unit"),
            Self::List(inner) => write!(f, "List<{inner}>"),
            Self::Variant { name, arguments } => {
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
