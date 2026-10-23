use crate::Span;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Path {
    pub segments: Vec<String>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeSyntax {
    pub path: Path,
    pub arguments: Vec<TypeSyntax>,
    pub span: Span,
}

impl fmt::Display for TypeSyntax {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path.segments.join("."))?;
        if !self.arguments.is_empty() {
            write!(f, "<")?;
            for (i, arg) in self.arguments.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{arg}")?;
            }
            write!(f, ">")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub name: Option<Path>,
    pub imports: Vec<Path>,
    pub records: Vec<Record>,
    pub variants: Vec<Variant>,
    pub functions: Vec<Function>,
    pub statements: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeParameter {
    pub name: String,
    pub constraints: Vec<Path>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub public: bool,
    pub name: String,
    pub type_parameters: Vec<TypeParameter>,
    pub cases: Vec<VariantCase>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantCase {
    pub name: String,
    pub payloads: Vec<TypeSyntax>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pattern {
    Wildcard(Span),
    Case {
        ty: TypeSyntax,
        case: String,
        bindings: Vec<DeclaredName>,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub public: bool,
    pub name: String,
    pub type_parameters: Vec<TypeParameter>,
    pub fields: Vec<RecordField>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordField {
    pub name: String,
    pub ty: TypeSyntax,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub public: bool,
    pub name: String,
    pub type_parameters: Vec<TypeParameter>,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeSyntax>,
    /// `None` retains legacy inference; `Some([])` is an explicit pure declaration.
    pub effects: Option<Declaration>,
    pub capabilities: Option<Declaration>,
    pub requires: Vec<Expr>,
    pub ensures: Vec<Expr>,
    pub body: Vec<Stmt>,
    /// Distinct marker: source emitters must reject this until they implement the route.
    pub foreign: Option<ForeignBinding>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignBinding {
    /// Required explicit escape hatch for native declarations.
    pub unsafe_call: bool,
    pub route: String,
    pub target: Path,
    pub runtime: String,
    pub ownership: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    pub names: Vec<DeclaredName>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclaredName {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub ty: TypeSyntax,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StmtKind {
    Match {
        scrutinee: Expr,
        arms: Vec<MatchArm>,
    },
    /// Syntax-only next-stage loop contracts; semantics explicitly deferred.
    LoopContract {
        decreases: bool,
        expression: Expr,
    },
    /// A statement with two lexical scopes and no tail-expression value.
    If {
        condition: Expr,
        then_body: Vec<Stmt>,
        else_body: Vec<Stmt>,
    },
    NoGc {
        budget: Expr,
        body: Vec<Stmt>,
    },
    Let {
        mutable: bool,
        name: String,
        annotation: Option<TypeSyntax>,
        value: Expr,
    },
    Assign {
        target: Path,
        op: AssignOp,
        value: Expr,
    },
    Return(Option<Expr>),
    Expr(Expr),
    For {
        binding: String,
        iter: Expr,
        body: Vec<Stmt>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignOp {
    Set,
    Add,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallArg {
    pub name: Option<String>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprKind {
    /// Explicit type application on a constructor (e.g. Option<i64>.Some(1)).
    Constructor {
        ty: TypeSyntax,
        case: String,
        arguments: Vec<CallArg>,
    },
    Collection(Vec<Expr>),
    Index {
        collection: Box<Expr>,
        index: Box<Expr>,
    },
    Name(Path),
    Integer(String),
    StringLiteral(String),
    Boolean(bool),
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        arguments: Vec<CallArg>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
}
