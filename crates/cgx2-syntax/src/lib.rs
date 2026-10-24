//! Candidate CGx2 indentation syntax.

mod ordinary_memory;
pub use ordinary_memory::{ordinary_memory_builtin, ordinary_memory_span};
mod ast;
mod format;
mod lexer;
mod ordinary_owned;
mod parser;
pub use ordinary_owned::ordinary_owned_span;

pub use ast::{
    AssignOp, BinaryOp, CallArg, Declaration, DeclaredName, Expr, ExprKind, ForeignBinding,
    Function, MatchArm, Module, Parameter, Path, Pattern, Record, RecordField, Stmt, StmtKind,
    TypeParameter, TypeSyntax, Variant, VariantCase,
};
pub use format::{format_expression, format_module};
pub use lexer::{lex, Span, SyntaxDiagnostic, Token, TokenKind};
pub use parser::parse_module;

mod visit;
pub use visit::{s2a_feature_span, visit_expr, visit_stmts};

mod canonical;
pub use canonical::{canonical_module, qualify_type};

mod default_managed;
pub use default_managed::{managed_feature_span, managed_guard_span};
