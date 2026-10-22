//! Candidate CGx2 indentation syntax.

mod ast;
mod format;
mod lexer;
mod parser;

pub use ast::{
    AssignOp, BinaryOp, CallArg, Declaration, DeclaredName, Expr, ExprKind, Function, Module,
    Parameter, Path, Stmt, StmtKind, TypeSyntax,
};
pub use format::{format_expression, format_module};
pub use lexer::{lex, Span, SyntaxDiagnostic, Token, TokenKind};
pub use parser::parse_module;
