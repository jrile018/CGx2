//! Conservative source lowering for the initial, script-only memory profile.
use std::collections::BTreeMap;

use cgx2_ownership::{check_ops, Op};
use cgx2_syntax::{CallArg, Expr, ExprKind, Module, Path, Span, Stmt, StmtKind};

use crate::{SpanTypes, TypeDiagnostic, TypedModule};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Region,
    Owned,
    Managed,
    Borrow,
}

#[derive(Default)]
struct Lowerer {
    bindings: BTreeMap<String, Kind>,
    ops: Vec<Op>,
    diagnostics: Vec<TypeDiagnostic>,
}

/// Return `None` for the unchanged scalar checker, or a checked memory script.
/// Only direct reserved `memory.*` intrinsics are lowered. All other operations
/// in a memory script are rejected before `check_ops` is called.
pub(crate) fn check_memory_module(
    module: &Module,
) -> Option<Result<TypedModule, Vec<TypeDiagnostic>>> {
    if !module.statements.iter().any(has_memory)
        && !module
            .functions
            .iter()
            .any(|f| f.body.iter().any(has_memory))
    {
        return None;
    }
    if !module.functions.is_empty() {
        let span = module
            .functions
            .iter()
            .flat_map(|f| &f.body)
            .find(|stmt| has_memory(stmt))
            .map_or(module.span, |stmt| stmt.span);
        return Some(Err(vec![TypeDiagnostic::new(
            "CGM001",
            "memory operations in functions are not modeled by the source checker yet".into(),
            span,
        )]));
    }
    let mut lowerer = Lowerer::default();
    lowerer.block(&module.statements);
    if lowerer.diagnostics.is_empty() {
        if let Err(errors) = check_ops(&lowerer.ops) {
            lowerer.diagnostics.extend(
                errors
                    .into_iter()
                    .map(|error| TypeDiagnostic::new(error.code, error.message, error.span)),
            );
        }
    }
    Some(if lowerer.diagnostics.is_empty() {
        Ok(TypedModule {
            functions: BTreeMap::new(),
            expressions: SpanTypes::default(),
            effects: BTreeMap::new(),
        })
    } else {
        Err(lowerer.diagnostics)
    })
}

fn has_memory(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::NoGc { .. } => true,
        StmtKind::Let { value, .. } | StmtKind::Assign { value, .. } | StmtKind::Expr(value) => {
            expr_has_memory(value)
        }
        StmtKind::Return(Some(value)) => expr_has_memory(value),
        StmtKind::For { iter, body, .. } => expr_has_memory(iter) || body.iter().any(has_memory),
        StmtKind::Return(None) => false,
    }
}

fn expr_has_memory(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Name(path) => path.segments.first().is_some_and(|s| s == "memory"),
        ExprKind::Call { callee, arguments } => {
            expr_has_memory(callee) || arguments.iter().any(|a| expr_has_memory(&a.value))
        }
        ExprKind::Binary { left, right, .. } => expr_has_memory(left) || expr_has_memory(right),
        _ => false,
    }
}

impl Lowerer {
    fn report(&mut self, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(TypeDiagnostic::new("CGM001", message.into(), span));
    }

    fn argument_error(&mut self, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(TypeDiagnostic::new("CGM002", message.into(), span));
    }

    fn block(&mut self, statements: &[Stmt]) {
        for stmt in statements {
            match &stmt.kind {
                StmtKind::NoGc { budget, body } => {
                    self.bytes(budget, "budget");
                    self.ops.push(Op::EnterNoGc { span: stmt.span });
                    self.block(body);
                    self.ops.push(Op::ExitNoGc { span: stmt.span });
                }
                StmtKind::Let {
                    mutable,
                    name,
                    annotation,
                    value,
                } => {
                    if *mutable || annotation.is_some() {
                        self.report(
                            "memory bindings cannot be mutable or annotated in this slice",
                            stmt.span,
                        );
                        continue;
                    }
                    if self.bindings.contains_key(name) {
                        self.report(format!("duplicate memory binding `{name}`"), stmt.span);
                        continue;
                    }
                    if let Some(kind) = self.call(value, Some(name)) {
                        self.bindings.insert(name.clone(), kind);
                    }
                }
                StmtKind::Expr(expr) => {
                    if let ExprKind::Name(path) = &expr.kind {
                        if let Some(name) = self.name(path, &[Kind::Owned, Kind::Managed]) {
                            self.ops.push(Op::Use {
                                value: name,
                                span: expr.span,
                            });
                        }
                    } else {
                        self.call(expr, None);
                    }
                }
                StmtKind::Assign { .. } | StmtKind::For { .. } | StmtKind::Return(_) => {
                    self.report(
                        "assignment, control flow, and returns are unsupported in memory scripts",
                        stmt.span,
                    );
                }
            }
        }
    }

    fn call(&mut self, expr: &Expr, destination: Option<&str>) -> Option<Kind> {
        let ExprKind::Call { callee, arguments } = &expr.kind else {
            self.report(
                "memory binding requires a direct memory intrinsic call",
                expr.span,
            );
            return None;
        };
        let ExprKind::Name(path) = &callee.kind else {
            self.report(
                "computed calls are unsupported in memory scripts",
                callee.span,
            );
            return None;
        };
        let [namespace, operation] = path.segments.as_slice() else {
            self.report(
                "only direct memory.* calls are supported in memory scripts",
                path.span,
            );
            return None;
        };
        if namespace != "memory" {
            self.report(
                "calls outside reserved memory namespace are unsupported in memory scripts",
                path.span,
            );
            return None;
        }
        match operation.as_str() {
            "region" if destination.is_some() => {
                if arguments.len() != 2 {
                    self.argument_error("memory.region requires mode and limit", expr.span);
                    return None;
                }
                let mode = self.named(arguments, "mode")?;
                let limit = self.named(arguments, "limit")?;
                if !matches!(&mode.kind, ExprKind::Name(p) if p.segments.as_slice() == ["auto"] || p.segments.as_slice() == ["manual"])
                {
                    self.argument_error("mode must be auto or manual", mode.span);
                    return None;
                }
                self.bytes(limit, "limit")?;
                self.ops.push(Op::OpenRegion {
                    region: destination.unwrap_or_default().into(),
                    span: expr.span,
                });
                Some(Kind::Region)
            }
            "owned_bytes" if destination.is_some() => {
                let args = self.positional(arguments, 1, expr.span)?;
                self.bytes(args[0], "size")?;
                self.ops.push(Op::CreateOwned {
                    value: destination.unwrap_or_default().into(),
                    span: expr.span,
                });
                Some(Kind::Owned)
            }
            "move" if destination.is_some() => {
                let args = self.positional(arguments, 1, expr.span)?;
                let from = self.reference(args[0], &[Kind::Owned])?;
                self.ops.push(Op::Move {
                    from,
                    to: destination.unwrap_or_default().into(),
                    span: expr.span,
                });
                Some(Kind::Owned)
            }
            "promote_owned" if destination.is_some() => {
                let args = self.positional(arguments, 2, expr.span)?;
                let region = self.reference(args[0], &[Kind::Region])?;
                let from = self.reference(args[1], &[Kind::Owned])?;
                self.ops.push(Op::Promote {
                    from,
                    to: destination.unwrap_or_default().into(),
                    region,
                    span: expr.span,
                });
                Some(Kind::Managed)
            }
            "copy_to_owned" if destination.is_some() => {
                let args = self.positional(arguments, 1, expr.span)?;
                let from = self.reference(args[0], &[Kind::Managed])?;
                self.ops.push(Op::CopyToOwned {
                    from,
                    to: destination.unwrap_or_default().into(),
                    span: expr.span,
                });
                Some(Kind::Owned)
            }
            "alloc_bytes" if destination.is_some() => {
                let args = self.positional(arguments, 2, expr.span)?;
                let region = self.reference(args[0], &[Kind::Region])?;
                self.bytes(args[1], "size")?;
                self.ops.push(Op::CreateManaged {
                    value: destination.unwrap_or_default().into(),
                    region,
                    span: expr.span,
                });
                Some(Kind::Managed)
            }
            "borrow" | "borrow_mut" if destination.is_some() => {
                let args = self.positional(arguments, 1, expr.span)?;
                let value = self.reference(args[0], &[Kind::Owned, Kind::Managed])?;
                let token = destination.unwrap_or_default().to_owned();
                self.ops.push(if operation == "borrow" {
                    Op::BorrowShared {
                        value,
                        token,
                        span: expr.span,
                    }
                } else {
                    Op::BorrowMut {
                        value,
                        token,
                        span: expr.span,
                    }
                });
                Some(Kind::Borrow)
            }
            "collect" | "close" | "drop" | "use" | "end_borrow" if destination.is_none() => {
                let args = self.positional(arguments, 1, expr.span)?;
                let expected = match operation.as_str() {
                    "collect" | "close" => &[Kind::Region][..],
                    "end_borrow" => &[Kind::Borrow][..],
                    _ => &[Kind::Owned, Kind::Managed][..],
                };
                let name = self.reference(args[0], expected)?;
                self.ops.push(match operation.as_str() {
                    "collect" => Op::Collect {
                        region: name,
                        span: expr.span,
                    },
                    "close" => Op::CloseRegion {
                        region: name,
                        span: expr.span,
                    },
                    "drop" => Op::DropValue {
                        value: name,
                        span: expr.span,
                    },
                    "use" => Op::Use {
                        value: name,
                        span: expr.span,
                    },
                    "end_borrow" => Op::EndBorrow {
                        token: name,
                        span: expr.span,
                    },
                    _ => unreachable!(),
                });
                None
            }
            _ => {
                self.report(
                    format!(
                        "unsupported memory operation `memory.{operation}` or use in this position"
                    ),
                    expr.span,
                );
                None
            }
        }
    }

    fn named<'a>(&mut self, args: &'a [CallArg], label: &str) -> Option<&'a Expr> {
        if let Some(arg) = args.iter().find(|arg| arg.name.as_deref() == Some(label)) {
            Some(&arg.value)
        } else {
            self.argument_error(
                format!("missing named argument `{label}`"),
                args.first().map_or(Span { start: 0, end: 0 }, |a| a.span),
            );
            None
        }
    }

    fn positional<'a>(
        &mut self,
        args: &'a [CallArg],
        count: usize,
        span: Span,
    ) -> Option<Vec<&'a Expr>> {
        if args.len() != count || args.iter().any(|a| a.name.is_some()) {
            self.argument_error(format!("expected {count} positional argument(s)"), span);
            None
        } else {
            Some(args.iter().map(|a| &a.value).collect())
        }
    }

    fn bytes(&mut self, expr: &Expr, label: &str) -> Option<usize> {
        if let ExprKind::Integer(raw) = &expr.kind {
            if let Ok(value) = raw.replace('_', "").parse::<usize>() {
                return Some(value);
            }
        }
        self.argument_error(
            format!("{label} must be a nonnegative integer literal within byte range"),
            expr.span,
        );
        None
    }

    fn reference(&mut self, expr: &Expr, allowed: &[Kind]) -> Option<String> {
        if let ExprKind::Name(path) = &expr.kind {
            self.name(path, allowed)
        } else {
            self.argument_error("expected a memory handle name", expr.span);
            None
        }
    }

    fn name(&mut self, path: &Path, allowed: &[Kind]) -> Option<String> {
        if let [name] = path.segments.as_slice() {
            if self
                .bindings
                .get(name)
                .is_some_and(|kind| allowed.contains(kind))
            {
                return Some(name.clone());
            }
        }
        self.argument_error(
            format!(
                "unknown memory handle or wrong handle type `{}`",
                path.segments.join(".")
            ),
            path.span,
        );
        None
    }
}
