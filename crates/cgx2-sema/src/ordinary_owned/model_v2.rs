//! Additive schema-2 data shared by the checked interpreter and standalone Rust output.
//! A Model is data, never an acceptance certificate. Only OwnedProgram executes.
pub use super::model::SourceSpan;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    OwnedResult(usize),
    AllocationError,
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
        matches!(
            self,
            Self::Owned(_) | Self::Shared(_) | Self::Exclusive(_) | Self::OwnedResult(_)
        )
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
    TryOwned(Box<Expression>),
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
    NoGc {
        budget: usize,
        body: Vec<Statement>,
    },
    MatchResult {
        scrutinee: Expression,
        ok_local: usize,
        err_local: usize,
        ok_body: Vec<Statement>,
        err_body: Vec<Statement>,
    },
    MatchError {
        scrutinee: Expression,
        arms: Vec<Vec<Statement>>,
    },
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
            ExpressionKind::TryOwned(e) => format!("TryOwned(Box::new({}))", e.rust()),
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
            StatementKind::NoGc { budget, body } => format!("NoGc{{budget:{budget},body:{}}}", list(body, Self::rust)),
            StatementKind::MatchResult { scrutinee, ok_local, err_local, ok_body, err_body } => format!("MatchResult{{scrutinee:{},ok_local:{ok_local},err_local:{err_local},ok_body:{},err_body:{}}}", scrutinee.rust(), list(ok_body, Self::rust), list(err_body, Self::rust)),
            StatementKind::MatchError { scrutinee, arms } => format!("MatchError{{scrutinee:{},arms:{}}}", scrutinee.rust(), list(arms, |a| list(a, Self::rust))),
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

// A lossless schema-1 projection is available only when no new shape is present.
impl Ty {
    fn as_v1(self) -> Option<super::model::Ty> {
        use super::model::Ty as Old;
        Some(match self {
            Self::I64 => Old::I64,
            Self::Bool => Old::Bool,
            Self::Unit => Old::Unit,
            Self::Record(i) => Old::Record(i),
            Self::Owned(i) => Old::Owned(i),
            Self::Shared(i) => Old::Shared(i),
            Self::Exclusive(i) => Old::Exclusive(i),
            Self::OwnedResult(_) | Self::AllocationError => return None,
        })
    }
}
impl Expression {
    fn as_v1(&self) -> Option<super::model::Expression> {
        use super::model::ExpressionKind as Old;
        Some(super::model::Expression {
            ty: self.ty.as_v1()?,
            span: self.span,
            kind: match &self.kind {
                ExpressionKind::I64(i) => Old::I64(*i),
                ExpressionKind::Bool(b) => Old::Bool(*b),
                ExpressionKind::Unit => Old::Unit,
                ExpressionKind::Local(i) => Old::Local(*i),
                ExpressionKind::Field { local, field } => Old::Field {
                    local: *local,
                    field: *field,
                },
                ExpressionKind::Record {
                    layout,
                    fields,
                    order,
                } => Old::Record {
                    layout: *layout,
                    fields: fields
                        .iter()
                        .map(Expression::as_v1)
                        .collect::<Option<_>>()?,
                    order: order.clone(),
                },
                ExpressionKind::Owned(e) => Old::Owned(Box::new(e.as_v1()?)),
                ExpressionKind::Borrow { local } => Old::Borrow { local: *local },
                ExpressionKind::Call {
                    function,
                    arguments,
                } => Old::Call {
                    function: *function,
                    arguments: arguments
                        .iter()
                        .map(Expression::as_v1)
                        .collect::<Option<_>>()?,
                },
                ExpressionKind::Binary { op, left, right } => Old::Binary {
                    op: *op,
                    left: Box::new(left.as_v1()?),
                    right: Box::new(right.as_v1()?),
                },
                ExpressionKind::TryOwned(_) => return None,
            },
        })
    }
}
impl Statement {
    fn as_v1(&self) -> Option<super::model::Statement> {
        use super::model::StatementKind as Old;
        Some(super::model::Statement {
            span: self.span,
            kind: match &self.kind {
                StatementKind::Let { local, value } => Old::Let {
                    local: *local,
                    value: value.as_v1()?,
                },
                StatementKind::Set {
                    local,
                    field,
                    value,
                    add,
                } => Old::Set {
                    local: *local,
                    field: *field,
                    value: value.as_v1()?,
                    add: *add,
                },
                StatementKind::Evaluate(e) => Old::Evaluate(e.as_v1()?),
                StatementKind::Return(e) => Old::Return(e.as_v1()?),
                StatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => Old::If {
                    condition: condition.as_v1()?,
                    then_body: then_body
                        .iter()
                        .map(Statement::as_v1)
                        .collect::<Option<_>>()?,
                    else_body: else_body
                        .iter()
                        .map(Statement::as_v1)
                        .collect::<Option<_>>()?,
                },
                StatementKind::NoGc { .. }
                | StatementKind::MatchResult { .. }
                | StatementKind::MatchError { .. } => return None,
            },
        })
    }
}
impl Model {
    pub(super) fn as_v1(&self) -> Option<super::model::Model> {
        use super::model as old;
        Some(old::Model {
            layouts: self
                .layouts
                .iter()
                .map(|l| {
                    Some(old::Layout {
                        name: l.name.clone(),
                        size: l.size,
                        span: l.span,
                        fields: l
                            .fields
                            .iter()
                            .map(|f| {
                                Some(old::Field {
                                    name: f.name.clone(),
                                    ty: f.ty.as_v1()?,
                                    offset: f.offset,
                                    span: f.span,
                                })
                            })
                            .collect::<Option<_>>()?,
                    })
                })
                .collect::<Option<_>>()?,
            functions: self
                .functions
                .iter()
                .map(|f| {
                    Some(old::Function {
                        name: f.name.clone(),
                        parameters: f.parameters,
                        result: f.result.as_v1()?,
                        span: f.span,
                        locals: f
                            .locals
                            .iter()
                            .map(|l| {
                                Some(old::Local {
                                    name: l.name.clone(),
                                    ty: l.ty.as_v1()?,
                                    span: l.span,
                                })
                            })
                            .collect::<Option<_>>()?,
                        body: f.body.iter().map(Statement::as_v1).collect::<Option<_>>()?,
                        ownership_parameters: f.ownership_parameters.clone(),
                    })
                })
                .collect::<Option<_>>()?,
            entry: self.entry,
        })
    }
}
