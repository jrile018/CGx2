use super::{diagnostic, model_v2::*, source_span, span};
use crate::TypeDiagnostic;
use cgx2_ownership::v2 as o;
use cgx2_syntax::{self as s, ExprKind as E, StmtKind as S};
use std::collections::{BTreeMap, BTreeSet};
type Check<T> = Result<T, TypeDiagnostic>;
type Lowered = (Model, Vec<o::Function>, Vec<o::FunctionSummary>);
fn err(message: impl Into<String>, span: s::Span) -> TypeDiagnostic {
    diagnostic("CGT081", message, span)
}
fn reserved(n: &str) -> bool {
    matches!(
        n,
        "owned"
            | "shared"
            | "exclusive"
            | "try_owned"
            | "AllocationError"
            | "Result"
            | "borrow"
            | "borrow_mut"
    )
}
fn name(n: &str, at: s::Span) -> Check<()> {
    if reserved(n) {
        Err(diagnostic(
            "CGT082",
            format!(
                "reserved ownership symbol '{n}' cannot be declared or shadowed in this profile"
            ),
            at,
        ))
    } else {
        Ok(())
    }
}
fn ty(t: &s::TypeSyntax, layouts: &[Layout]) -> Check<Ty> {
    let n = t.path.segments.join(".");
    if n == "Result" {
        if t.arguments.len() != 2 || ty(&t.arguments[1], layouts)? != Ty::AllocationError {
            return Err(err(
                "owned Result requires Result<owned<R>, AllocationError>",
                t.span,
            ));
        }
        if let Ty::Owned(i) = ty(&t.arguments[0], layouts)? {
            return Ok(Ty::OwnedResult(i));
        }
        return Err(err(
            "only owned scalar-record Result payloads are supported",
            t.span,
        ));
    }
    if n == "AllocationError" && t.arguments.is_empty() {
        return Ok(Ty::AllocationError);
    }
    if matches!(n.as_str(), "owned" | "shared" | "exclusive") {
        if t.arguments.len() != 1 {
            return Err(err(
                "ownership wrapper requires exactly one local scalar record type",
                t.span,
            ));
        }
        let a = &t.arguments[0];
        if !a.arguments.is_empty() {
            return Err(err("generic ownership layouts are unsupported", a.span));
        }
        let Some(i) = layouts
            .iter()
            .position(|l| l.name == a.path.segments.join("."))
        else {
            return Err(err(
                "ownership layout must name a local nongeneric record",
                a.span,
            ));
        };
        return Ok(match n.as_str() {
            "owned" => Ty::Owned(i),
            "shared" => Ty::Shared(i),
            _ => Ty::Exclusive(i),
        });
    }
    if !t.arguments.is_empty() {
        return Err(diagnostic(
            "CGT002",
            "applied copied types are unsupported in ordinary-owned v1",
            t.span,
        ));
    }
    match n.as_str() {
        "i64" => Ok(Ty::I64),
        "Bool" => Ok(Ty::Bool),
        "Unit" => Ok(Ty::Unit),
        _ => layouts
            .iter()
            .position(|l| l.name == n)
            .map(Ty::Record)
            .ok_or_else(|| err("unknown ordinary-owned type", t.span)),
    }
}
#[derive(Clone)]
struct Signature {
    parameters: Vec<Ty>,
    result: Ty,
    mapping: Vec<Option<usize>>,
}
#[derive(Clone)]
struct Loan {
    place: o::Place,
    token: Option<String>,
    kind: o::LoanKind,
}
#[derive(Clone)]
struct Binding {
    id: usize,
    ty: Ty,
    mutable: bool,
    reference: Reference,
}
#[derive(Clone)]
enum Reference {
    Result {
        place: o::Place,
        origin: Option<usize>,
    },
    Copy,
    Owner {
        place: o::Place,
        origin: Option<usize>,
    },
    Loan(Loan),
}
struct Value {
    expr: Expression,
    reference: Reference,
}
impl Value {
    fn ty(&self) -> Ty {
        self.expr.ty
    }
}

pub(super) fn lower(module: &s::Module, source: &str) -> Check<Lowered> {
    if cgx2_syntax::ordinary_owned_span(module).is_none() {
        return Err(err("source does not select ordinary-owned v1", module.span));
    }
    if !module.imports.is_empty() || !module.variants.is_empty() || !module.statements.is_empty() {
        return Err(err(
            "ordinary-owned v1 requires local functions and scalar records; imports, variants and script statements are unsupported",
            module.span,
        ));
    }
    // New source expansion bounds apply only to the additive profile. Schema-1
    // copied-only bodies retain their prior source/runtime bounds.
    fn conditional_type(t: &s::TypeSyntax) -> bool {
        (t.path.segments == ["Result"] || t.path.segments == ["AllocationError"])
            || t.arguments.iter().any(conditional_type)
    }
    let new_profile = std::cell::Cell::new(module.functions.iter().any(|f| {
        f.parameters.iter().any(|p| conditional_type(&p.ty))
            || f.return_type.as_ref().is_some_and(conditional_type)
    }));
    let mut nodes = 0usize;
    let mut expressions = 0usize;
    for f in &module.functions {
        s::visit_stmts(
            &f.body,
            &mut |stmt| {
                nodes = nodes.saturating_add(1);
                if matches!(stmt.kind, S::NoGc { .. }) {
                    new_profile.set(true);
                }
            },
            &mut |expr| {
                expressions = expressions.saturating_add(1);
                if matches!(&expr.kind, E::Call {callee, ..} if matches!(&callee.kind, E::Name(p) if p.segments == ["try_owned"]))
                {
                    new_profile.set(true);
                }
            },
        );
    }
    if new_profile.get() && (nodes > 4096 || expressions > 4096) {
        return Err(err(
            "ordinary Result statement/expression limit is 4096",
            module.span,
        ));
    }
    fn bounded_expression(e: &s::Expr, depth: usize) -> Check<()> {
        if depth > 32 {
            return Err(err(
                "ordinary Result expression nesting limit is 32",
                e.span,
            ));
        }
        match &e.kind {
            E::Call { callee, arguments } => {
                bounded_expression(callee, depth + 1)?;
                for a in arguments {
                    bounded_expression(&a.value, depth + 1)?;
                }
            }
            E::Constructor { arguments, .. } => {
                for a in arguments {
                    bounded_expression(&a.value, depth + 1)?;
                }
            }
            E::Binary { left, right, .. } => {
                bounded_expression(left, depth + 1)?;
                bounded_expression(right, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn bounded_body(body: &[s::Stmt], depth: usize) -> Check<()> {
        for stmt in body {
            if depth > 32 {
                return Err(err("ordinary Result block nesting limit is 32", stmt.span));
            }
            match &stmt.kind {
                S::NoGc { budget, body } => {
                    bounded_expression(budget, 1)?;
                    bounded_body(body, depth + 1)?;
                }
                S::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    bounded_expression(condition, 1)?;
                    bounded_body(then_body, depth + 1)?;
                    bounded_body(else_body, depth + 1)?;
                }
                S::Match { scrutinee, arms } => {
                    bounded_expression(scrutinee, 1)?;
                    for arm in arms {
                        bounded_body(&arm.body, depth + 1)?;
                    }
                }
                S::Let { value, .. }
                | S::Assign { value, .. }
                | S::Expr(value)
                | S::Return(Some(value)) => bounded_expression(value, 1)?,
                _ => {}
            }
        }
        Ok(())
    }
    if new_profile.get() {
        for f in &module.functions {
            bounded_body(&f.body, 0)?;
        }
    }
    if module.functions.len() > 256 {
        return Err(err("ordinary-owned function limit is 256", module.span));
    }
    let mut names = BTreeSet::new();
    let mut layouts = Vec::new();
    for record in &module.records {
        name(&record.name, record.span)?;
        if !names.insert(record.name.clone()) || !record.type_parameters.is_empty() {
            return Err(err("duplicate or generic record layout", record.span));
        }
        if record.fields.len() > 256 {
            return Err(err("record field limit is 256", record.span));
        }
        let mut fields = Vec::new();
        let mut size = 0usize;
        let mut seen = BTreeSet::new();
        for f in &record.fields {
            name(&f.name, f.span)?;
            if !seen.insert(f.name.clone()) {
                return Err(err("duplicate record field", f.span));
            }
            let t = ty(&f.ty, &[])?;
            let width = match t {
                Ty::I64 => 8,
                Ty::Bool => 1,
                _ => {
                    return Err(err(
                        "owned layouts contain only i64 and Bool fields",
                        f.ty.span,
                    ));
                }
            };
            fields.push(Field {
                name: f.name.clone(),
                ty: t,
                offset: size,
                span: source_span(f.span),
            });
            size = size
                .checked_add(width)
                .ok_or_else(|| err("record layout overflow", f.span))?;
        }
        layouts.push(Layout {
            name: record.name.clone(),
            fields,
            size,
            span: source_span(record.span),
        });
    }
    let mut signatures = Vec::new();
    let mut function_names = BTreeMap::new();
    for (id, f) in module.functions.iter().enumerate() {
        name(&f.name, f.span)?;
        if !names.insert(f.name.clone()) {
            return Err(err("duplicate declaration", f.span));
        }
        if !f.type_parameters.is_empty()
            || f.foreign.is_some()
            || !f.requires.is_empty()
            || !f.ensures.is_empty()
            || f.effects.is_some()
            || f.capabilities.is_some()
        {
            return Err(err(
                "generics, foreign calls, contracts and explicit effects require a later ordinary-memory extension",
                f.span,
            ));
        }
        let mut parameters = Vec::new();
        let mut mapping = Vec::new();
        let mut oi = 0;
        for p in &f.parameters {
            name(&p.name, p.span)?;
            let t = ty(&p.ty, &layouts)?;
            mapping.push(if t.ownership() {
                let i = oi;
                oi += 1;
                Some(i)
            } else {
                None
            });
            parameters.push(t);
        }
        let result = f
            .return_type
            .as_ref()
            .map_or(Ok(Ty::Unit), |t| ty(t, &layouts))?;
        function_names.insert(f.name.clone(), id);
        signatures.push(Signature {
            parameters,
            result,
            mapping,
        });
    }
    let entry = *function_names
        .get("main")
        .ok_or_else(|| err("ordinary-owned source requires main", module.span))?;
    if !signatures[entry].parameters.is_empty()
        || signatures[entry].result.ownership()
        || signatures[entry].result == Ty::AllocationError
    {
        return Err(err(
            "main requires no parameters and a copied scalar or Unit result",
            module.functions[entry].span,
        ));
    }
    // Source call graph first: no summary becomes visible before its body passes v1.
    let mut dependencies = vec![BTreeSet::new(); signatures.len()];
    for (i, f) in module.functions.iter().enumerate() {
        s::visit_stmts(&f.body, &mut |_| {}, &mut |e| {
            if let E::Call { callee, .. } = &e.kind {
                if let E::Name(p) = &callee.kind {
                    if let Some(id) = function_names.get(&p.segments.join(".")) {
                        dependencies[i].insert(*id);
                    }
                }
            }
        });
    }
    fn order(
        i: usize,
        deps: &[BTreeSet<usize>],
        state: &mut [u8],
        out: &mut Vec<usize>,
        module: &s::Module,
    ) -> Check<()> {
        if state[i] == 1 {
            return Err(err(
                "recursive ownership SCCs are unsupported; no unchecked recursive summary is exposed",
                module.functions[i].span,
            ));
        }
        if state[i] == 2 {
            return Ok(());
        }
        state[i] = 1;
        for d in &deps[i] {
            order(*d, deps, state, out, module)?;
        }
        state[i] = 2;
        out.push(i);
        Ok(())
    }
    let mut sorted = Vec::new();
    let mut state = vec![0; signatures.len()];
    for i in 0..signatures.len() {
        order(i, &dependencies, &mut state, &mut sorted, module)?;
    }
    let mut models: Vec<Option<Function>> = vec![None; signatures.len()];
    let mut ownership = Vec::new();
    let mut summaries = Vec::new();
    let mut checked = BTreeMap::new();
    for i in sorted {
        let lower = Lower::new(module, i, &signatures, &layouts, &function_names, &checked)?;
        let (model, cfg) = lower.finish()?;
        let report = o::analyze_function(&cfg, &summaries, source, o::AnalysisLimits::default());
        if !report.is_accepted() {
            let d = &report.diagnostics[0];
            return Err(diagnostic(d.code, d.message.clone(), d.span));
        }
        checked.insert(i, cfg.summary.clone());
        summaries.push(cfg.summary.clone());
        ownership.push(cfg);
        models[i] = Some(model);
    }
    Ok((
        Model {
            layouts,
            functions: models.into_iter().map(Option::unwrap).collect(),
            entry,
        },
        ownership,
        summaries,
    ))
}

struct Lower<'a> {
    module: &'a s::Module,
    id: usize,
    signatures: &'a [Signature],
    layouts: &'a [Layout],
    functions: &'a BTreeMap<String, usize>,
    checked: &'a BTreeMap<usize, o::FunctionSummary>,
    locals: Vec<Local>,
    owner_locals: Vec<o::Local>,
    scopes: Vec<BTreeMap<String, Binding>>,
    blocks: Vec<o::Block>,
    current: Option<usize>,
    tokens: Vec<String>,
    serial: usize,
    operation_count: usize,
    returns: Vec<Option<usize>>,
}
impl<'a> Lower<'a> {
    fn new(
        module: &'a s::Module,
        id: usize,
        signatures: &'a [Signature],
        layouts: &'a [Layout],
        functions: &'a BTreeMap<String, usize>,
        checked: &'a BTreeMap<usize, o::FunctionSummary>,
    ) -> Check<Self> {
        let f = &module.functions[id];
        let mut this = Self {
            module,
            id,
            signatures,
            layouts,
            functions,
            checked,
            locals: vec![],
            owner_locals: vec![],
            scopes: vec![BTreeMap::new()],
            blocks: vec![],
            current: None,
            tokens: vec![],
            serial: 0,
            operation_count: 0,
            returns: vec![],
        };
        this.current = Some(this.block(f.span)?);
        for (i, p) in f.parameters.iter().enumerate() {
            let t = signatures[id].parameters[i];
            let local = this.local(&p.name, t, p.span);
            let reference = if t.ownership() {
                let v = if matches!(t, Ty::OwnedResult(_)) {
                    this.sum_local(p.span)?
                } else {
                    this.owner_local(p.span)?
                };
                match t {
                    Ty::OwnedResult(_) => Reference::Result {
                        place: o::Place::local(v),
                        origin: signatures[id].mapping[i],
                    },
                    Ty::Owned(_) => Reference::Owner {
                        place: o::Place::local(v),
                        origin: signatures[id].mapping[i],
                    },
                    _ => Reference::Loan(Loan {
                        place: o::Place::local(v),
                        token: None,
                        kind: if matches!(t, Ty::Shared(_)) {
                            o::LoanKind::Shared
                        } else {
                            o::LoanKind::Exclusive
                        },
                    }),
                }
            } else {
                Reference::Copy
            };
            this.insert(
                &p.name,
                Binding {
                    id: local,
                    ty: t,
                    mutable: false,
                    reference,
                },
                p.span,
            )?;
        }
        Ok(this)
    }
    fn local(&mut self, name: &str, ty: Ty, at: s::Span) -> usize {
        let id = self.locals.len();
        self.locals.push(Local {
            name: name.into(),
            ty,
            span: source_span(at),
        });
        id
    }
    fn fresh(&mut self) -> String {
        let n = format!("v{}", self.serial);
        self.serial += 1;
        n
    }
    fn owner_local(&mut self, at: s::Span) -> Check<String> {
        if self.owner_locals.len() >= o::AnalysisLimits::default().max_locals {
            return Err(diagnostic(
                "CGO106",
                "ownership local limit before expansion",
                at,
            ));
        }
        let name = self.fresh();
        self.owner_locals.push(o::Local {
            name: name.clone(),
            storage: o::Storage::Owned,
            span: at,
        });
        Ok(name)
    }
    fn sum_local(&mut self, at: s::Span) -> Check<String> {
        let name = self.owner_local(at)?;
        self.owner_locals.last_mut().unwrap().storage = o::Storage::ConditionalOwned;
        Ok(name)
    }
    fn block(&mut self, at: s::Span) -> Check<usize> {
        if self.blocks.len() >= o::AnalysisLimits::default().max_blocks {
            return Err(diagnostic(
                "CGO106",
                "ownership block limit before expansion",
                at,
            ));
        }
        let id = self.blocks.len();
        self.blocks.push(o::Block {
            operations: vec![],
            terminator: o::Terminator::Return(o::ReturnValue::Unit),
            span: at,
        });
        Ok(id)
    }
    fn op(&mut self, operation: o::Operation, at: s::Span) -> Check<()> {
        if self.operation_count >= o::AnalysisLimits::default().max_operations {
            return Err(diagnostic(
                "CGO106",
                "ownership operation limit before expansion",
                at,
            ));
        }
        if let Some(i) = self.current {
            self.operation_count += 1;
            self.blocks[i].operations.push(o::SpannedOperation {
                operation,
                span: at,
            });
        }
        Ok(())
    }
    fn terminate(&mut self, terminator: o::Terminator, at: s::Span) {
        if let Some(i) = self.current.take() {
            self.blocks[i].terminator = terminator;
            self.blocks[i].span = at;
        }
    }
    fn insert(&mut self, n: &str, b: Binding, at: s::Span) -> Check<()> {
        name(n, at)?;
        let scope = self.scopes.last_mut().unwrap();
        if scope.insert(n.into(), b).is_some() {
            return Err(err("duplicate binding in lexical scope", at));
        }
        Ok(())
    }
    fn binding(&self, p: &s::Path) -> Check<Binding> {
        self.scopes
            .iter()
            .rev()
            .find_map(|s| s.get(&p.segments[0]))
            .cloned()
            .ok_or_else(|| err(format!("unresolved local '{}'", p.segments[0]), p.span))
    }
    fn field(&self, b: &Binding, p: &s::Path) -> Check<usize> {
        if p.segments.len() != 2 {
            return Err(err(
                "only named-base scalar field projection is supported",
                p.span,
            ));
        }
        let layout =
            b.ty.layout()
                .ok_or_else(|| err("field projection requires a record layout", p.span))?;
        self.layouts[layout]
            .fields
            .iter()
            .position(|f| f.name == p.segments[1])
            .ok_or_else(|| {
                err(
                    format!(
                        "unknown field '{}' in '{}'",
                        p.segments[1], self.layouts[layout].name
                    ),
                    p.span,
                )
            })
    }
    fn access(&mut self, b: &Binding, field: usize, write: bool, at: s::Span) -> Check<()> {
        let field_name = self.layouts[b.ty.layout().unwrap()].fields[field]
            .name
            .clone();
        match &b.reference {
            Reference::Owner { place, .. } => {
                if write && !b.mutable {
                    return Err(err(
                        "owner field write requires a mutable owner or exclusive loan",
                        at,
                    ));
                }
                let p = place.clone().project(o::Projection::Field(field_name));
                self.op(
                    if write {
                        o::Operation::Write(p)
                    } else {
                        o::Operation::Read(p)
                    },
                    at,
                )?;
            }
            Reference::Loan(l) => {
                if write && l.kind != o::LoanKind::Exclusive {
                    return Err(err("field write requires exclusive access", at));
                }
                if let Some(token) = &l.token {
                    self.op(
                        o::Operation::UseLoan {
                            token: token.clone(),
                            write,
                        },
                        at,
                    )?;
                } else {
                    let p = l.place.clone().project(o::Projection::Field(field_name));
                    self.op(
                        if write {
                            o::Operation::Write(p)
                        } else {
                            o::Operation::Read(p)
                        },
                        at,
                    )?;
                }
            }
            Reference::Copy if !write => {}
            _ => return Err(err("copied records are immutable", at)),
        }
        Ok(())
    }
    fn expression(&mut self, e: &s::Expr, call_argument: bool) -> Check<Value> {
        let at = e.span;
        let make = |kind, ty, reference| Value {
            expr: Expression {
                kind,
                ty,
                span: source_span(at),
            },
            reference,
        };
        match &e.kind {
            E::Integer(n) => Ok(make(
                ExpressionKind::I64(
                    n.replace('_', "")
                        .parse()
                        .map_err(|_| err("integer outside i64", at))?,
                ),
                Ty::I64,
                Reference::Copy,
            )),
            E::Boolean(b) => Ok(make(ExpressionKind::Bool(*b), Ty::Bool, Reference::Copy)),
            E::Name(p) => {
                let b = self.binding(p)?;
                if p.segments.len() == 1 {
                    return Ok(make(ExpressionKind::Local(b.id), b.ty, b.reference));
                }
                let field = self.field(&b, p)?;
                self.access(&b, field, false, at)?;
                Ok(make(
                    ExpressionKind::Field { local: b.id, field },
                    self.layouts[b.ty.layout().unwrap()].fields[field].ty,
                    Reference::Copy,
                ))
            }
            E::Binary { left, op, right } => {
                let a = self.expression(left, false)?;
                let b = self.expression(right, false)?;
                if a.ty() != Ty::I64 || b.ty() != Ty::I64 {
                    return Err(err("arithmetic requires copied i64 operands", at));
                }
                let op = match op {
                    s::BinaryOp::Add => 0,
                    s::BinaryOp::Subtract => 1,
                    s::BinaryOp::Multiply => 2,
                    s::BinaryOp::Divide => 3,
                };
                Ok(make(
                    ExpressionKind::Binary {
                        op,
                        left: Box::new(a.expr),
                        right: Box::new(b.expr),
                    },
                    Ty::I64,
                    Reference::Copy,
                ))
            }
            E::Call { callee, arguments } => {
                let E::Name(p) = &callee.kind else {
                    return Err(err(
                        "call target must be a resolved local function",
                        callee.span,
                    ));
                };
                self.call(&p.segments.join("."), arguments, at, call_argument)
            }
            E::Constructor { case, .. } if !case.is_empty() => Err(err("direct Result/AllocationError constructors are excluded; propagate try_owned results", at)),
            E::Constructor {
                ty,
                case,
                arguments,
            } if case.is_empty() && ty.arguments.is_empty() => {
                self.call(&ty.path.segments.join("."), arguments, at, call_argument)
            }
            _ => Err(err("expression is outside ordinary-owned v1", at)),
        }
    }
    fn call(
        &mut self,
        n: &str,
        args: &[s::CallArg],
        at: s::Span,
        call_argument: bool,
    ) -> Check<Value> {
        let make = |kind, ty, reference| Value {
            expr: Expression {
                kind,
                ty,
                span: source_span(at),
            },
            reference,
        };
        if n.starts_with("Result.") || n.starts_with("AllocationError.") {
            return Err(err("direct Result/AllocationError constructors are excluded; propagate try_owned results", at));
        }
        if matches!(n, "owned" | "try_owned" | "borrow" | "borrow_mut") {
            if args.len() != 1 || args[0].name.is_some() {
                return Err(err("ownership intrinsic requires one positional value", at));
            }
            if n == "owned" || n == "try_owned" {
                let value = self.expression(&args[0].value, false)?;
                let Ty::Record(layout) = value.ty() else {
                    return Err(err(
                        "owned(value) requires a scalar record value",
                        args[0].span,
                    ));
                };
                if n == "try_owned" {
                    let temp = self.sum_local(at)?;
                    self.op(o::Operation::TryCreate(temp.clone()), at)?;
                    return Ok(make(
                        ExpressionKind::TryOwned(Box::new(value.expr)),
                        Ty::OwnedResult(layout),
                        Reference::Result {
                            place: o::Place::local(temp),
                            origin: None,
                        },
                    ));
                }
                let temp = self.owner_local(at)?;
                self.op(o::Operation::Create(temp.clone()), at)?;
                return Ok(make(
                    ExpressionKind::Owned(Box::new(value.expr)),
                    Ty::Owned(layout),
                    Reference::Owner {
                        place: o::Place::local(temp),
                        origin: None,
                    },
                ));
            }
            let E::Name(p) = &args[0].value.kind else {
                return Err(err("borrow requires a named whole owner", args[0].span));
            };
            if p.segments.len() != 1 {
                return Err(err(
                    "borrowed field results require a later layout extension",
                    p.span,
                ));
            }
            let b = self.binding(p)?;
            let Reference::Owner { place, .. } = b.reference else {
                return Err(diagnostic(
                    "CGT083",
                    "reborrowing an existing loan is unsupported",
                    args[0].span,
                ));
            };
            let layout = b.ty.layout().unwrap();
            let kind = if n == "borrow" {
                o::LoanKind::Shared
            } else {
                o::LoanKind::Exclusive
            };
            // Direct call borrows are represented by Call's place access, with call-only lifetime.
            let token = if call_argument {
                None
            } else {
                let token = self.fresh();
                self.op(
                    o::Operation::Borrow {
                        place: place.clone(),
                        token: token.clone(),
                        kind,
                    },
                    at,
                )?;
                self.tokens.push(token.clone());
                Some(token)
            };
            return Ok(make(
                ExpressionKind::Borrow { local: b.id },
                if kind == o::LoanKind::Shared {
                    Ty::Shared(layout)
                } else {
                    Ty::Exclusive(layout)
                },
                Reference::Loan(Loan { place, token, kind }),
            ));
        }
        if let Some(layout) = self.layouts.iter().position(|l| l.name == n) {
            if args.len() != self.layouts[layout].fields.len() {
                return Err(err("record constructor field count mismatch", at));
            }
            let mut seen = BTreeSet::new();
            let mut fields = Vec::new();
            let mut order = Vec::new();
            for arg in args {
                let field = arg
                    .name
                    .as_ref()
                    .and_then(|n| {
                        self.layouts[layout]
                            .fields
                            .iter()
                            .position(|f| &f.name == n)
                    })
                    .ok_or_else(|| err("record arguments must name declared fields", arg.span))?;
                if !seen.insert(field) {
                    return Err(err("duplicate record constructor field", arg.span));
                }
                let value = self.expression(&arg.value, false)?;
                if value.ty() != self.layouts[layout].fields[field].ty {
                    return Err(err("record field type mismatch", arg.value.span));
                }
                fields.push(value.expr);
                order.push(field);
            }
            return Ok(make(
                ExpressionKind::Record {
                    layout,
                    fields,
                    order,
                },
                Ty::Record(layout),
                Reference::Copy,
            ));
        }
        if let Some(op) = [
            "$cgx2_cmp_eq",
            "$cgx2_cmp_ne",
            "$cgx2_cmp_lt",
            "$cgx2_cmp_le",
            "$cgx2_cmp_gt",
            "$cgx2_cmp_ge",
        ]
        .iter()
        .position(|s| *s == n)
        {
            if args.len() != 2 {
                return Err(err("comparison arity", at));
            }
            let a = self.expression(&args[0].value, false)?;
            let b = self.expression(&args[1].value, false)?;
            if a.ty() != b.ty() || !(a.ty() == Ty::I64 || a.ty() == Ty::Bool && op <= 1) {
                return Err(err("invalid comparison operands", at));
            }
            return Ok(make(
                ExpressionKind::Binary {
                    op: op as u8 + 4,
                    left: Box::new(a.expr),
                    right: Box::new(b.expr),
                },
                Ty::Bool,
                Reference::Copy,
            ));
        }
        let id = *self
            .functions
            .get(n)
            .ok_or_else(|| err(format!("unresolved ordinary-owned function '{n}'"), at))?;
        let sig = self.signatures[id].clone();
        let summary = self
            .checked
            .get(&id)
            .ok_or_else(|| err("callee has no body-checked summary", at))?
            .clone();
        if args.len() != sig.parameters.len() {
            return Err(err("function parameter count mismatch", at));
        }
        let mut expressions = Vec::new();
        let mut places = Vec::new();
        let mut origins = Vec::new();
        for (arg, expected) in args.iter().zip(&sig.parameters) {
            if arg.name.is_some() {
                return Err(err(
                    "named function arguments are unsupported in ordinary-owned v1",
                    arg.span,
                ));
            }
            let value = self.expression(&arg.value, true)?;
            if value.ty() != *expected {
                return Err(err(
                    "function parameter type/provenance mismatch",
                    arg.value.span,
                ));
            }
            match value.reference {
                Reference::Result { place, origin } => {
                    let temp = self.sum_local(arg.value.span)?;
                    self.op(
                        o::Operation::MoveResult {
                            from: place,
                            to: temp.clone(),
                        },
                        arg.value.span,
                    )?;
                    places.push(o::Place::local(temp));
                    origins.push(origin);
                }
                Reference::Owner { place, origin } => {
                    // Source arguments evaluate left-to-right: transfer now, before later arguments.
                    let temp = self.owner_local(arg.value.span)?;
                    self.op(
                        o::Operation::Move {
                            from: place,
                            to: temp.clone(),
                        },
                        arg.value.span,
                    )?;
                    places.push(o::Place::local(temp));
                    origins.push(origin);
                }
                Reference::Loan(l) => {
                    // Inline borrow_mut(owner) is call-only. Named exclusive loans cannot be silently erased.
                    if l.kind == o::LoanKind::Exclusive
                        && !matches!(arg.value.kind, E::Call { ref callee, .. } if matches!(&callee.kind, E::Name(p) if p.segments == ["borrow_mut"]))
                    {
                        return Err(diagnostic(
                            "CGT083",
                            "forwarding an existing exclusive loan requires unsupported reborrow semantics",
                            arg.value.span,
                        ));
                    }
                    places.push(l.place);
                    origins.push(None);
                }
                Reference::Copy => {}
            }
            expressions.push(value.expr);
        }
        let (result, reference) = match (&summary.result, sig.result) {
            (o::ResultMode::OwnedResult { from_parameter }, Ty::OwnedResult(_)) => {
                let temp = self.sum_local(at)?;
                (
                    o::CallResult::OwnedResult(temp.clone()),
                    Reference::Result {
                        place: o::Place::local(temp),
                        origin: from_parameter.and_then(|i| origins[i]),
                    },
                )
            }
            (o::ResultMode::Owned { from_parameter }, Ty::Owned(_)) => {
                let temp = self.owner_local(at)?;
                (
                    o::CallResult::Owned(temp.clone()),
                    Reference::Owner {
                        place: o::Place::local(temp),
                        origin: from_parameter.and_then(|i| origins[i]),
                    },
                )
            }
            (
                o::ResultMode::Loan {
                    parameter,
                    projection,
                    kind,
                },
                Ty::Shared(_) | Ty::Exclusive(_),
            ) => {
                let token = self.fresh();
                self.tokens.push(token.clone());
                let mut place = places[*parameter].clone();
                place.projection.extend(projection.clone());
                (
                    o::CallResult::Loan(token.clone()),
                    Reference::Loan(Loan {
                        place,
                        token: Some(token),
                        kind: *kind,
                    }),
                )
            }
            (o::ResultMode::Unit, _) => (o::CallResult::Unit, Reference::Copy),
            _ => return Err(err("invalid callee result summary", at)),
        };
        self.op(
            o::Operation::Call {
                function: n.into(),
                arguments: places,
                result,
                retention_owner: None,
            },
            at,
        )?;
        Ok(make(
            ExpressionKind::Call {
                function: id,
                arguments: expressions,
            },
            sig.result,
            reference,
        ))
    }
    fn return_value(&mut self, v: Value, at: s::Span) -> Check<Statement> {
        if v.ty() != self.signatures[self.id].result {
            return Err(err("function result type/provenance mismatch", at));
        }
        let mut returned_token = None;
        let result = match v.reference {
            Reference::Result { place, origin } => {
                self.returns.push(origin);
                o::ReturnValue::OwnedResult(place)
            }
            Reference::Copy => o::ReturnValue::Unit,
            Reference::Owner { place, origin } => {
                self.returns.push(origin);
                o::ReturnValue::Owned(place)
            }
            Reference::Loan(l) => {
                let token = if let Some(t) = l.token {
                    t
                } else {
                    let t = self.fresh();
                    self.op(
                        o::Operation::Borrow {
                            place: l.place,
                            token: t.clone(),
                            kind: l.kind,
                        },
                        at,
                    )?;
                    t
                };
                returned_token = Some(token.clone());
                o::ReturnValue::Loan(token)
            }
        };
        for token in self.tokens.clone().iter().rev() {
            if returned_token.as_ref() != Some(token) {
                self.op(o::Operation::EndLoan(token.clone()), at)?;
            }
        }
        self.terminate(o::Terminator::Return(result), at);
        Ok(Statement {
            kind: StatementKind::Return(v.expr),
            span: source_span(at),
        })
    }
    fn statements(&mut self, body: &[s::Stmt], tail: bool) -> Check<Vec<Statement>> {
        self.scopes.push(BTreeMap::new());
        let token_start = self.tokens.len();
        let mut out = Vec::new();
        for (i, s) in body.iter().enumerate() {
            if self.current.is_none() {
                return Err(err("unreachable statements are unsupported", s.span));
            }
            let kind = match &s.kind {
                S::Let {
                    mutable,
                    name: n,
                    annotation,
                    value,
                } => {
                    name(n, s.span)?;
                    let value = self.expression(value, false)?;
                    let t = value.ty();
                    if let Some(annotation) = annotation {
                        if ty(annotation, self.layouts)? != t {
                            return Err(err("binding annotation mismatch", annotation.span));
                        }
                    }
                    let local = self.local(n, t, s.span);
                    if matches!(t, Ty::Exclusive(_))
                        && matches!(value.expr.kind, ExpressionKind::Local(_))
                    {
                        return Err(diagnostic(
                            "CGT083",
                            "aliasing an existing exclusive loan requires unsupported reborrow semantics",
                            span(value.expr.span),
                        ));
                    }
                    let reference = match value.reference {
                        Reference::Result { place, origin } => {
                            let dest = self.sum_local(s.span)?;
                            self.op(
                                o::Operation::MoveResult {
                                    from: place,
                                    to: dest.clone(),
                                },
                                span(value.expr.span),
                            )?;
                            Reference::Result {
                                place: o::Place::local(dest),
                                origin,
                            }
                        }
                        Reference::Owner { place, origin } => {
                            let dest = self.owner_local(s.span)?;
                            self.op(
                                o::Operation::Move {
                                    from: place,
                                    to: dest.clone(),
                                },
                                span(value.expr.span),
                            )?;
                            Reference::Owner {
                                place: o::Place::local(dest),
                                origin,
                            }
                        }
                        Reference::Loan(l) if l.token.is_none() => {
                            if l.kind == o::LoanKind::Exclusive {
                                return Err(diagnostic(
                                    "CGT083",
                                    "aliasing an exclusive parameter requires unsupported reborrow semantics",
                                    span(value.expr.span),
                                ));
                            }
                            let token = self.fresh();
                            self.op(
                                o::Operation::Borrow {
                                    place: l.place.clone(),
                                    token: token.clone(),
                                    kind: l.kind,
                                },
                                span(value.expr.span),
                            )?;
                            self.tokens.push(token.clone());
                            Reference::Loan(Loan {
                                token: Some(token),
                                ..l
                            })
                        }
                        reference => reference,
                    };
                    self.insert(
                        n,
                        Binding {
                            id: local,
                            ty: t,
                            mutable: *mutable,
                            reference,
                        },
                        s.span,
                    )?;
                    StatementKind::Let {
                        local,
                        value: value.expr,
                    }
                }
                S::Assign { target, op, value } => {
                    let b = self.binding(target)?;
                    let value = self.expression(value, false)?;
                    let field = if target.segments.len() == 1 {
                        if !b.mutable || b.ty.ownership() || matches!(b.ty, Ty::Record(_)) {
                            return Err(err(
                                "assignment requires a mutable copied scalar",
                                target.span,
                            ));
                        }
                        None
                    } else {
                        let f = self.field(&b, target)?;
                        self.access(&b, f, true, target.span)?;
                        Some(f)
                    };
                    let expected =
                        field.map_or(b.ty, |f| self.layouts[b.ty.layout().unwrap()].fields[f].ty);
                    if value.ty() != expected
                        || matches!(op, s::AssignOp::Add) && expected != Ty::I64
                    {
                        return Err(err("assignment type mismatch", s.span));
                    }
                    StatementKind::Set {
                        local: b.id,
                        field,
                        value: value.expr,
                        add: matches!(op, s::AssignOp::Add),
                    }
                }
                S::Return(value) => {
                    let v = value.as_ref().map_or_else(
                        || {
                            Ok(Value {
                                expr: Expression {
                                    kind: ExpressionKind::Unit,
                                    ty: Ty::Unit,
                                    span: source_span(s.span),
                                },
                                reference: Reference::Copy,
                            })
                        },
                        |e| self.expression(e, false),
                    )?;
                    out.push(self.return_value(v, value.as_ref().map_or(s.span, |v| v.span))?);
                    continue;
                }
                S::Expr(e) => {
                    let v = self.expression(e, false)?;
                    if tail && i + 1 == body.len() {
                        out.push(self.return_value(v, e.span)?);
                        continue;
                    }
                    if let Reference::Result { place, .. } = &v.reference {
                        self.op(o::Operation::DropResult(place.clone()), e.span)?;
                    }
                    if let Reference::Owner { place, .. } = &v.reference {
                        self.op(o::Operation::Drop(place.clone()), e.span)?;
                    }
                    StatementKind::Evaluate(v.expr)
                }
                S::NoGc { budget, body } => {
                    let E::Integer(n) = &budget.kind else {
                        return Err(err(
                            "no_gc budget requires a nonnegative integer literal",
                            budget.span,
                        ));
                    };
                    let budget = n
                        .replace('_', "")
                        .parse::<usize>()
                        .map_err(|_| err("no_gc budget is outside usize", budget.span))?;
                    StatementKind::NoGc {
                        budget,
                        body: self.statements(body, false)?,
                    }
                }
                S::Match { scrutinee, arms } => {
                    self.match_statement(scrutinee, arms, s.span, tail && i + 1 == body.len())?
                }
                S::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    let c = self.expression(condition, false)?;
                    if c.ty() != Ty::Bool {
                        return Err(err("if condition requires Bool", condition.span));
                    }
                    let then_block = self.block(s.span)?;
                    let else_block = self.block(s.span)?;
                    self.terminate(
                        o::Terminator::Branch {
                            then_block,
                            else_block,
                        },
                        condition.span,
                    );
                    self.current = Some(then_block);
                    let then_body = self.statements(then_body, false)?;
                    let then_exit = self.current.take();
                    self.current = Some(else_block);
                    let else_body = self.statements(else_body, false)?;
                    let else_exit = self.current.take();
                    if then_exit.is_some() || else_exit.is_some() {
                        let join = self.block(s.span)?;
                        for exit in [then_exit, else_exit].into_iter().flatten() {
                            self.blocks[exit].terminator = o::Terminator::Goto(join);
                        }
                        self.current = Some(join);
                    }
                    StatementKind::If {
                        condition: c.expr,
                        then_body,
                        else_body,
                    }
                }
                _ => {
                    return Err(err(
                        "loops, matching, regions and no_gc remain mandatory later extensions",
                        s.span,
                    ));
                }
            };
            out.push(Statement {
                kind,
                span: source_span(s.span),
            });
        }
        if self.current.is_some() {
            for token in self.tokens[token_start..].to_vec().iter().rev() {
                self.op(
                    o::Operation::EndLoan(token.clone()),
                    body.last()
                        .map_or(self.module.functions[self.id].span, |s| s.span),
                )?;
            }
        }
        self.tokens.truncate(token_start);
        self.scopes.pop();
        Ok(out)
    }
    fn match_statement(
        &mut self,
        scrutinee: &s::Expr,
        arms: &[s::MatchArm],
        at: s::Span,
        tail: bool,
    ) -> Check<StatementKind> {
        let value = self.expression(scrutinee, false)?;
        let cases: &[&str] = match value.ty() {
            Ty::OwnedResult(_) => &["Ok", "Err"],
            Ty::AllocationError => &[
                "BudgetExceeded",
                "RegionLimit",
                "HostAllocation",
                "InvalidRegion",
                "ShapeLimit",
            ],
            _ => {
                return Err(err(
                    "match requires an owned Result or AllocationError",
                    scrutinee.span,
                ))
            }
        };
        if arms.len() != cases.len() {
            return Err(err("match must exhaust exactly the declared cases", at));
        }
        let type_name = if cases.len() == 2 {
            "Result"
        } else {
            "AllocationError"
        };
        let mut ordered = vec![None; cases.len()];
        for arm in arms {
            let s::Pattern::Case {
                ty: pattern_ty,
                case,
                bindings,
                ..
            } = &arm.pattern
            else {
                return Err(err(
                    "only exhaustive qualified case patterns are supported",
                    arm.span,
                ));
            };
            if pattern_ty.path.segments.join(".") != type_name
                || (!pattern_ty.arguments.is_empty() && ty(pattern_ty, self.layouts)? != value.ty())
            {
                return Err(err(
                    "pattern type/layout differs from scrutinee",
                    pattern_ty.span,
                ));
            }
            let index = cases
                .iter()
                .position(|c| *c == case)
                .ok_or_else(|| err("unknown match case", arm.span))?;
            if ordered[index].replace(arm).is_some()
                || bindings.len() != usize::from(cases.len() == 2)
            {
                return Err(err("duplicate case or wrong payload arity", arm.span));
            }
        }
        let blocks: Vec<_> = cases.iter().map(|_| self.block(at)).collect::<Check<_>>()?;
        let mut binding_locals = Vec::new();
        let mut references = Vec::new();
        if let (Ty::OwnedResult(layout), Reference::Result { place, origin }) =
            (value.ty(), &value.reference)
        {
            let payload = self.owner_local(at)?;
            self.terminate(
                o::Terminator::MatchResult {
                    from: place.clone(),
                    payload: payload.clone(),
                    ok: blocks[0],
                    err: blocks[1],
                },
                scrutinee.span,
            );
            references.push(Reference::Owner {
                place: o::Place::local(payload),
                origin: *origin,
            });
            references.push(Reference::Copy);
            binding_locals.push(self.local("$ok", Ty::Owned(layout), at));
            binding_locals.push(self.local("$err", Ty::AllocationError, at));
        } else {
            // Five copied error arms: a linear binary decision CFG checks every arm.
            for (i, block) in blocks.iter().enumerate().take(blocks.len() - 1) {
                let next = if i + 2 == blocks.len() {
                    blocks[i + 1]
                } else {
                    self.block(at)?
                };
                self.terminate(
                    o::Terminator::Branch {
                        then_block: *block,
                        else_block: next,
                    },
                    scrutinee.span,
                );
                self.current = Some(next);
            }
            self.current = None;
        }
        let mut bodies = Vec::new();
        let mut exits = Vec::new();
        for (i, arm) in ordered.into_iter().enumerate() {
            let arm = arm.unwrap();
            self.current = Some(blocks[i]);
            self.scopes.push(BTreeMap::new());
            if cases.len() == 2 {
                let s::Pattern::Case { bindings, .. } = &arm.pattern else {
                    unreachable!()
                };
                let local = binding_locals[i];
                self.insert(
                    &bindings[0].name,
                    Binding {
                        id: local,
                        ty: self.locals[local].ty,
                        mutable: false,
                        reference: references[i].clone(),
                    },
                    bindings[0].span,
                )?;
            }
            bodies.push(self.statements(&arm.body, tail)?);
            self.scopes.pop();
            exits.push(self.current.take());
        }
        if exits.iter().any(Option::is_some) {
            let join = self.block(at)?;
            for exit in exits.into_iter().flatten() {
                self.blocks[exit].terminator = o::Terminator::Goto(join);
            }
            self.current = Some(join);
        }
        if cases.len() == 2 {
            let err_body = bodies.pop().unwrap();
            let ok_body = bodies.pop().unwrap();
            Ok(StatementKind::MatchResult {
                scrutinee: value.expr,
                ok_local: binding_locals[0],
                err_local: binding_locals[1],
                ok_body,
                err_body,
            })
        } else {
            Ok(StatementKind::MatchError {
                scrutinee: value.expr,
                arms: bodies,
            })
        }
    }
    fn finish(mut self) -> Check<(Function, o::Function)> {
        let f = &self.module.functions[self.id];
        let sig = self.signatures[self.id].clone();
        let mut body = self.statements(&f.body, true)?;
        if self.current.is_some() {
            if sig.result != Ty::Unit {
                return Err(err("not every path returns the declared result", f.span));
            }
            body.push(self.return_value(
                Value {
                    expr: Expression {
                        kind: ExpressionKind::Unit,
                        ty: Ty::Unit,
                        span: source_span(f.span),
                    },
                    reference: Reference::Copy,
                },
                f.span,
            )?);
        }
        let result = match sig.result {
            Ty::Owned(_) | Ty::OwnedResult(_) => {
                let origin = self.returns.first().copied().flatten();
                if self.returns.iter().any(|o| *o != origin) {
                    return Err(err(
                        "owned result paths have different parameter provenance",
                        f.span,
                    ));
                }
                if matches!(sig.result, Ty::OwnedResult(_)) {
                    o::ResultMode::OwnedResult {
                        from_parameter: origin,
                    }
                } else {
                    o::ResultMode::Owned {
                        from_parameter: origin,
                    }
                }
            }
            Ty::Shared(layout) | Ty::Exclusive(layout) => {
                let eligible: Vec<_> = sig.parameters.iter().enumerate().filter(|(_, t)| matches!((sig.result, t), (Ty::Shared(_), Ty::Shared(i) | Ty::Exclusive(i)) | (Ty::Exclusive(_), Ty::Exclusive(i)) if *i == layout)).map(|(i, _)| i).collect();
                if eligible.len() != 1 {
                    return Err(err(
                        "borrowed result requires exactly one eligible borrowed input",
                        f.return_type.as_ref().unwrap().span,
                    ));
                }
                o::ResultMode::Loan {
                    parameter: sig.mapping[eligible[0]].unwrap(),
                    projection: vec![],
                    kind: if matches!(sig.result, Ty::Shared(_)) {
                        o::LoanKind::Shared
                    } else {
                        o::LoanKind::Exclusive
                    },
                }
            }
            _ => o::ResultMode::Unit,
        };
        let summary = o::FunctionSummary {
            name: f.name.clone(),
            parameters: sig
                .parameters
                .iter()
                .filter_map(|t| match t {
                    Ty::OwnedResult(_) => Some(o::ParameterMode::OwnedResult),
                    Ty::Owned(_) => Some(o::ParameterMode::Owned),
                    Ty::Shared(_) => Some(o::ParameterMode::Shared),
                    Ty::Exclusive(_) => Some(o::ParameterMode::Exclusive),
                    _ => None,
                })
                .collect(),
            result,
            retention: o::Retention::Local,
        };
        Ok((
            Function {
                name: f.name.clone(),
                parameters: f.parameters.len(),
                result: sig.result,
                locals: self.locals,
                body,
                span: source_span(f.span),
                ownership_parameters: sig.mapping,
            },
            o::Function {
                summary,
                locals: self.owner_locals,
                blocks: self.blocks,
                entry: 0,
                span: f.span,
            },
        ))
    }
}
