//! Version 1 data shared by the checked interpreter and standalone Rust output.
//! A Model is data, never an acceptance certificate. Only OwnedProgram executes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    I64,
    Bool,
    Unit,
    Record(usize),
    Owned(usize),
    Shared(usize),
    Exclusive(usize),
}
impl Ty {
    pub fn ownership(self) -> bool {
        matches!(self, Self::Owned(_) | Self::Shared(_) | Self::Exclusive(_))
    }
    pub fn layout(self) -> Option<usize> {
        match self {
            Self::Record(i) | Self::Owned(i) | Self::Shared(i) | Self::Exclusive(i) => Some(i),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub ty: Ty,
    pub offset: usize,
    pub span: SourceSpan,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub name: String,
    pub fields: Vec<Field>,
    pub size: usize,
    pub span: SourceSpan,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Local {
    pub name: String,
    pub ty: Ty,
    pub span: SourceSpan,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub parameters: usize,
    pub result: Ty,
    pub locals: Vec<Local>,
    pub body: Vec<Statement>,
    pub span: SourceSpan,
    /// Source parameter index -> ownership-v1 index (copied slots are None).
    pub ownership_parameters: Vec<Option<usize>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    pub layouts: Vec<Layout>,
    pub functions: Vec<Function>,
    pub entry: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub ty: Ty,
    pub span: SourceSpan,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExpressionKind {
    I64(i64),
    Bool(bool),
    Unit,
    Local(usize),
    Field {
        local: usize,
        field: usize,
    },
    Record {
        layout: usize,
        fields: Vec<Expression>,
        order: Vec<usize>,
    },
    Owned(Box<Expression>),
    Borrow {
        local: usize,
    },
    Call {
        function: usize,
        arguments: Vec<Expression>,
    },
    Binary {
        op: u8,
        left: Box<Expression>,
        right: Box<Expression>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: SourceSpan,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StatementKind {
    Let {
        local: usize,
        value: Expression,
    },
    Set {
        local: usize,
        field: Option<usize>,
        value: Expression,
        add: bool,
    },
    Evaluate(Expression),
    Return(Expression),
    If {
        condition: Expression,
        then_body: Vec<Statement>,
        else_body: Vec<Statement>,
    },
}

// Fully qualified literals: emitted source carries precisely the already-checked IR.
fn list<T>(xs: &[T], f: impl Fn(&T) -> String) -> String {
    format!("vec![{}]", xs.iter().map(f).collect::<Vec<_>>().join(","))
}
impl SourceSpan {
    fn rust(self) -> String {
        format!("SourceSpan{{start:{},end:{}}}", self.start, self.end)
    }
}
impl Ty {
    fn rust(self) -> String {
        format!("Ty::{self:?}")
    }
}
impl Expression {
    fn rust(&self) -> String {
        let k = match &self.kind {
            ExpressionKind::I64(v) => format!("I64({v})"),
            ExpressionKind::Bool(v) => format!("Bool({v})"),
            ExpressionKind::Unit => "Unit".into(),
            ExpressionKind::Local(i) => format!("Local({i})"),
            ExpressionKind::Field { local, field } => {
                format!("Field{{local:{local},field:{field}}}")
            }
            ExpressionKind::Record {
                layout,
                fields,
                order,
            } => format!(
                "Record{{layout:{layout},fields:{},order:vec!{:?}}}",
                list(fields, Self::rust),
                order
            ),
            ExpressionKind::Owned(e) => format!("Owned(Box::new({}))", e.rust()),
            ExpressionKind::Borrow { local } => format!("Borrow{{local:{local}}}"),
            ExpressionKind::Call {
                function,
                arguments,
            } => format!(
                "Call{{function:{function},arguments:{}}}",
                list(arguments, Self::rust)
            ),
            ExpressionKind::Binary { op, left, right } => format!(
                "Binary{{op:{op},left:Box::new({}),right:Box::new({})}}",
                left.rust(),
                right.rust()
            ),
        };
        format!(
            "Expression{{kind:ExpressionKind::{k},ty:{},span:{}}}",
            self.ty.rust(),
            self.span.rust()
        )
    }
}
impl Statement {
    fn rust(&self) -> String {
        let k = match &self.kind {
            StatementKind::Let { local, value } => {
                format!("Let{{local:{local},value:{}}}", value.rust())
            }
            StatementKind::Set {
                local,
                field,
                value,
                add,
            } => format!(
                "Set{{local:{local},field:{field:?},value:{},add:{add}}}",
                value.rust()
            ),
            StatementKind::Evaluate(e) => format!("Evaluate({})", e.rust()),
            StatementKind::Return(e) => format!("Return({})", e.rust()),
            StatementKind::If {
                condition,
                then_body,
                else_body,
            } => format!(
                "If{{condition:{},then_body:{},else_body:{}}}",
                condition.rust(),
                list(then_body, Self::rust),
                list(else_body, Self::rust)
            ),
        };
        format!(
            "Statement{{kind:StatementKind::{k},span:{}}}",
            self.span.rust()
        )
    }
}
impl Model {
    pub(super) fn rust(&self) -> String {
        let layouts = list(&self.layouts, |l| {
            format!(
                "Layout{{name:{:?}.into(),fields:{},size:{},span:{}}}",
                l.name,
                list(&l.fields, |f| format!(
                    "Field{{name:{:?}.into(),ty:{},offset:{},span:{}}}",
                    f.name,
                    f.ty.rust(),
                    f.offset,
                    f.span.rust()
                )),
                l.size,
                l.span.rust()
            )
        });
        let functions = list(&self.functions, |f| {
            format!(
                "Function{{name:{:?}.into(),parameters:{},result:{},locals:{},body:{},span:{},ownership_parameters:{}}}",
                f.name,
                f.parameters,
                f.result.rust(),
                list(&f.locals, |l| format!(
                    "Local{{name:{:?}.into(),ty:{},span:{}}}",
                    l.name,
                    l.ty.rust(),
                    l.span.rust()
                )),
                list(&f.body, Statement::rust),
                f.span.rust(),
                list(&f.ownership_parameters, |p| format!("{p:?}"))
            )
        });
        format!(
            "Model{{layouts:{layouts},functions:{functions},entry:{}}}",
            self.entry
        )
    }
}
