use super::diagnostic;
use crate::{SpanTypes, Type, TypeDiagnostic, TypedModule};
use cgx2_runtime::default_managed::model::*;
use cgx2_syntax::{AssignOp, BinaryOp, Expr, ExprKind, Module, Pattern, Span, Stmt, StmtKind};
use std::collections::{BTreeMap, BTreeSet};
fn loc(s: Span) -> Loc {
    Loc {
        start: s.start,
        end: s.end,
    }
}
type Result<T> = std::result::Result<T, Vec<TypeDiagnostic>>;
struct Lower<'a> {
    module: &'a Module,
    p: Program,
    types: Vec<Type>,
    strings: BTreeMap<String, usize>,
    functions: BTreeMap<String, usize>,
    view: SpanTypes,
    locals: Vec<BTreeMap<String, Type>>,
}
impl Lower<'_> {
    fn string(&mut self, s: &str) -> usize {
        if let Some(i) = self.strings.get(s) {
            return *i;
        }
        let id = self.p.strings.len();
        self.p.strings.push(s.into());
        self.strings.insert(s.into(), id);
        id
    }
    fn ty(&mut self, t: &Type) -> Result<usize> {
        if let Some(i) = self.types.iter().position(|old| old == t) {
            return Ok(i);
        }
        if self.types.len() >= 4096 {
            return Err(vec![diagnostic(
                "managed concrete type limit 4096",
                self.module.span,
            )]);
        }
        let id = self.types.len();
        self.types.push(t.clone());
        let name = self.string(&crate::type_key(t));
        self.p.layouts.push(Layout {
            name,
            shape: Shape::KUnit,
        });
        let shape = match t {
            Type::I64 => Shape::KInt,
            Type::Bool => Shape::KBool,
            Type::Unit => Shape::KUnit,
            Type::String => Shape::KString,
            Type::List(t) => Shape::KList(self.ty(t)?),
            Type::Array(t) => Shape::KArray(self.ty(t)?),
            Type::AppliedRecord { name, arguments } if name == "cgx2.managed.Ref" => {
                Shape::KRef(self.ty(&arguments[0])?)
            }
            Type::Variant { name, .. } if name == "cgx2.core.AllocationError" => Shape::KError,
            Type::Record(_) | Type::AppliedRecord { .. } => {
                let fields =
                    crate::nominal::members(t, self.module, &Default::default(), &mut vec![])
                        .ok_or_else(|| {
                            vec![diagnostic(
                                "missing managed record layout",
                                self.module.span,
                            )]
                        })?;
                let mut out = vec![];
                for (n, t) in fields {
                    out.push((self.string(&n), self.ty(&t)?));
                }
                Shape::KRecord(out)
            }
            Type::Variant { .. } => {
                let cases =
                    crate::variant_cases(self.module, &Default::default(), t).ok_or_else(|| {
                        vec![diagnostic(
                            "missing managed variant layout",
                            self.module.span,
                        )]
                    })?;
                let mut out = vec![];
                for (n, ts) in cases {
                    let mut fields = vec![];
                    for t in ts {
                        fields.push(self.ty(&t)?);
                    }
                    out.push((self.string(&n), fields));
                }
                Shape::KVariant(out)
            }
            Type::Parameter { .. } => {
                return Err(vec![diagnostic(
                    "unresolved managed type parameter",
                    self.module.span,
                )])
            }
        };
        self.p.layouts[id].shape = shape;
        Ok(id)
    }
    fn emit(&mut self, ty: usize, span: Span, op: Op) -> usize {
        let i = self.p.expressions.len();
        self.p.expressions.push(Expression {
            ty,
            loc: loc(span),
            op,
        });
        i
    }
    fn type_at(&self, e: &Expr) -> Result<Type> {
        self.view.get(e.span).cloned().ok_or_else(|| {
            vec![diagnostic(
                "expression lacks authenticated concrete type",
                e.span,
            )]
        })
    }
    fn local(&self, n: &str, span: Span) -> Result<Type> {
        self.locals
            .iter()
            .rev()
            .find_map(|m| m.get(n).cloned())
            .ok_or_else(|| vec![diagnostic(format!("unresolved local {n}"), span)])
    }
    fn expr(&mut self, e: &Expr) -> Result<usize> {
        if self.p.expressions.len() > 1_000_000 {
            return Err(vec![diagnostic("managed expression limit", e.span)]);
        }
        let t = self.type_at(e)?;
        let ty = self.ty(&t)?;
        let op = match &e.kind {
            ExprKind::Integer(n) => Op::EInt(
                n.replace('_', "")
                    .parse()
                    .map_err(|_| vec![diagnostic("integer exceeds i64", e.span)])?,
            ),
            ExprKind::Boolean(v) => Op::EBool(*v),
            ExprKind::StringLiteral(raw) => {
                let text = decode(raw).ok_or_else(|| {
                    vec![TypeDiagnostic {
                        code: "CGRS003",
                        message: "string literal contains unsupported escape".into(),
                        span: e.span,
                    }]
                })?;
                if text.len() > 1_048_576 {
                    return Err(vec![diagnostic("string exceeds 1 MiB UTF-8", e.span)]);
                }
                Op::EString(self.string(&text))
            }
            ExprKind::Name(p) => {
                let n = &p.segments[0];
                let mut t = self.local(n, e.span)?;
                let tid = self.ty(&t)?;
                let name = self.string(n);
                let mut value = self.emit(tid, e.span, Op::ELocal(name));
                for field in &p.segments[1..] {
                    let fields =
                        crate::nominal::members(&t, self.module, &Default::default(), &mut vec![])
                            .ok_or_else(|| vec![diagnostic("field of nonrecord", e.span)])?;
                    let index = fields
                        .iter()
                        .position(|(n, _)| n == field)
                        .ok_or_else(|| vec![diagnostic("missing field", e.span)])?;
                    t = fields[index].1.clone();
                    let tid = self.ty(&t)?;
                    value = self.emit(tid, e.span, Op::EField(value, index));
                }
                return Ok(value);
            }
            ExprKind::Binary { left, op, right } => Op::EBinary(
                self.expr(left)?,
                match op {
                    BinaryOp::Add => 1,
                    BinaryOp::Subtract => 2,
                    BinaryOp::Multiply => 3,
                    BinaryOp::Divide => 4,
                },
                self.expr(right)?,
            ),
            ExprKind::Collection(xs) => {
                let args = xs
                    .iter()
                    .map(|e| self.expr(e))
                    .collect::<Result<Vec<_>>>()?;
                Op::EConstruct(0, args)
            }
            ExprKind::Index { collection, index } => {
                Op::EData(1, vec![self.expr(collection)?, self.expr(index)?])
            }
            ExprKind::Constructor {
                case, arguments, ..
            } if !case.is_empty() => {
                let cases = crate::variant_cases(self.module, &Default::default(), &t)
                    .ok_or_else(|| vec![diagnostic("variant layout", e.span)])?;
                let case = cases
                    .iter()
                    .position(|(n, _)| n == case)
                    .ok_or_else(|| vec![diagnostic("variant case", e.span)])?;
                let args = arguments
                    .iter()
                    .map(|a| self.expr(&a.value))
                    .collect::<Result<Vec<_>>>()?;
                Op::EConstruct(case, args)
            }
            ExprKind::Call { callee, arguments } => {
                let ExprKind::Name(p) = &callee.kind else {
                    return Err(vec![diagnostic(
                        "computed managed call unsupported",
                        e.span,
                    )]);
                };
                return self.call(e, ty, &p.segments.join("."), arguments);
            }
            ExprKind::Constructor {
                ty: syntax,
                arguments,
                ..
            } => return self.call(e, ty, &syntax.path.segments.join("."), arguments),
        };
        Ok(self.emit(ty, e.span, op))
    }
    fn call(
        &mut self,
        e: &Expr,
        ty: usize,
        name: &str,
        args: &[cgx2_syntax::CallArg],
    ) -> Result<usize> {
        let values = args
            .iter()
            .map(|a| self.expr(&a.value))
            .collect::<Result<Vec<_>>>()?;
        let op = if let Some(f) = self.functions.get(name) {
            Op::ECall(*f, values)
        } else if matches!(self.p.layouts[ty].shape, Shape::KRecord(_))
            && (name == crate::type_key(&self.types[ty])
                || self.module.records.iter().any(|r| r.name == name))
        {
            let Shape::KRecord(fields) = &self.p.layouts[ty].shape else {
                unreachable!()
            };
            let mut out = vec![];
            for (a, id) in args.iter().zip(values) {
                let n = a
                    .name
                    .as_ref()
                    .ok_or_else(|| vec![diagnostic("record named field required", a.span)])?;
                let index = fields
                    .iter()
                    .position(|(s, _)| self.p.strings[*s] == *n)
                    .ok_or_else(|| vec![diagnostic("record field layout", a.span)])?;
                out.push((index, id));
            }
            Op::ERecord(out)
        } else if let Some(op) = comparison(name) {
            Op::EBinary(values[0], op, values[1])
        } else {
            match name {
                "managed.ref" => Op::ERef(values[0]),
                "managed.get" => Op::EGet(values[0]),
                "managed.set" => Op::ESet(values[0], values[1]),
                "managed.collect" => Op::ECollect,
                "managed.try_alloc" => Op::ETry(values[0]),
                _ => {
                    let op = INTRINSICS.iter().position(|n| *n == name).ok_or_else(|| {
                        vec![diagnostic(
                            format!("unsupported managed call {name}"),
                            e.span,
                        )]
                    })?;
                    Op::EData(op, values)
                }
            }
        };
        Ok(self.emit(ty, e.span, op))
    }
    fn block(&mut self, body: &[Stmt], nested: bool) -> Result<Vec<Statement>> {
        if nested {
            self.locals.push(BTreeMap::new());
        }
        let mut out = vec![];
        for st in body {
            out.push(match &st.kind{
            StmtKind::Let{name,value,..}=>{let e=self.expr(value)?;self.locals.last_mut().unwrap().insert(name.clone(),self.types[self.p.expressions[e].ty].clone());Statement::SLet(self.string(name),e)},
            StmtKind::Assign{target,op,value}=>{if target.segments.len()!=1{return Err(vec![diagnostic("immutable managed field assignment",st.span)]);}Statement::SAssign(self.string(&target.segments[0]),if *op==AssignOp::Set{0}else{1},self.expr(value)?)},
            StmtKind::Expr(e)=>Statement::SExpr(self.expr(e)?),StmtKind::Return(e)=>Statement::SReturn(e.as_ref().map(|e|self.expr(e)).transpose()?),
            StmtKind::If{condition,then_body,else_body}=>Statement::SIf(self.expr(condition)?,self.block(then_body,true)?,self.block(else_body,true)?),
            StmtKind::NoGc{budget,body}=>{let ExprKind::Integer(n)=&budget.kind else{return Err(vec![diagnostic("nonliteral no_gc budget",budget.span)]);};let n=n.replace('_',"").parse().map_err(|_|vec![diagnostic("invalid no_gc budget",budget.span)])?;Statement::SGuard(n,loc(st.span),self.block(body,true)?)},
            StmtKind::Match{scrutinee,arms}=>{let value=self.expr(scrutinee)?;let t=self.type_at(scrutinee)?;let cases=crate::variant_cases(self.module,&Default::default(),&t).ok_or_else(||vec![diagnostic("match layout",st.span)])?;let mut branches=vec![];for arm in arms{self.locals.push(BTreeMap::new());let(case,bindings)=match &arm.pattern{Pattern::Wildcard(_)=>(None,vec![]),Pattern::Case{case,bindings,..}=>{let c=cases.iter().position(|(n,_)|n==case).ok_or_else(||vec![diagnostic("match case identity",arm.span)])?;let mut ns=vec![];for(n,t)in bindings.iter().zip(&cases[c].1){self.locals.last_mut().unwrap().insert(n.name.clone(),t.clone());ns.push(self.string(&n.name));}(Some(c),ns)}};let body=self.block(&arm.body,false)?;self.locals.pop();branches.push(Arm{case,bindings,body});}Statement::SMatch(value,branches)},
            StmtKind::For{binding,iter,body}=>{self.locals.push(BTreeMap::new());let range=matches!(&iter.kind,ExprKind::Call{callee,..} if matches!(&callee.kind,ExprKind::Name(p) if p.segments==["range"]));let ty=if range{Type::I64}else{match self.type_at(iter)?{Type::List(t)|Type::Array(t)=>*t,_=>return Err(vec![diagnostic("for iterable type",iter.span)])}};self.locals.last_mut().unwrap().insert(binding.clone(),ty);let n=self.string(binding);let stmt=if range{let ExprKind::Call{arguments,..}=&iter.kind else{unreachable!()};let start=self.expr(&arguments[0].value)?;let end=self.expr(&arguments[1].value)?;Statement::SRange(n,start,end,self.block(body,false)?)}else{Statement::SFor(n,self.expr(iter)?,self.block(body,false)?)};self.locals.pop();stmt},
            StmtKind::LoopContract{..}=>return Err(vec![diagnostic("loop contract outside managed executable subset",st.span)]),
        });
        }
        if nested {
            self.locals.pop();
        }
        Ok(out)
    }
}
const INTRINSICS: [&str; 12] = [
    "length",
    "get",
    "updated",
    "append",
    "array",
    "list",
    "concat",
    "slice",
    "parse_i64",
    "show",
    "arg_count",
    "arg",
];
fn comparison(n: &str) -> Option<usize> {
    [
        "$cgx2_cmp_eq",
        "$cgx2_cmp_ne",
        "$cgx2_cmp_lt",
        "$cgx2_cmp_le",
        "$cgx2_cmp_gt",
        "$cgx2_cmp_ge",
    ]
    .iter()
    .position(|s| *s == n)
    .map(|i| i + 5)
}
fn decode(raw: &str) -> Option<String> {
    let mut s = String::new();
    let mut cs = raw.get(1..raw.len().checked_sub(1)?)?.chars();
    while let Some(c) = cs.next() {
        s.push(if c == '\\' {
            match cs.next()? {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                '"' => '"',
                _ => return None,
            }
        } else {
            c
        });
    }
    Some(s)
}
pub(super) fn lower(module: &Module, typed: &TypedModule) -> Result<(Program, Vec<Type>)> {
    let mut l = Lower {
        module,
        p: Program {
            root_maps: vec![],
            strings: vec![],
            layouts: vec![],
            expressions: vec![],
            functions: vec![],
            entry: 0,
            schema_version: 1,
            output_loc: loc(module.span),
        },
        types: vec![],
        strings: BTreeMap::new(),
        functions: module
            .functions
            .iter()
            .enumerate()
            .map(|(i, f)| (f.name.clone(), i))
            .collect(),
        view: typed.expressions.clone(),
        locals: vec![BTreeMap::new()],
    };
    for t in [Type::I64, Type::Bool, Type::Unit, Type::String] {
        l.ty(&t)?;
    }
    for f in &module.functions {
        if f.foreign.is_some() {
            return Err(vec![diagnostic(
                "foreign execution outside managed checkpoint",
                f.span,
            )]);
        }
        let sig = &typed.functions[&f.name];
        l.view = typed.bodies.get(&f.name).cloned().unwrap_or_default();
        l.locals = vec![f
            .parameters
            .iter()
            .zip(&sig.parameters)
            .map(|(p, t)| (p.name.clone(), t.clone()))
            .collect()];
        let parameters = f.parameters.iter().map(|p| l.string(&p.name)).collect();
        let parameter_types = sig
            .parameters
            .iter()
            .map(|t| l.ty(t))
            .collect::<Result<Vec<_>>>()?;
        let result = l.ty(&sig.return_type)?;
        let requires = f
            .requires
            .iter()
            .map(|e| l.expr(e))
            .collect::<Result<Vec<_>>>()?;
        let body = l.block(&f.body, false)?;
        l.locals[0].insert("result".into(), sig.return_type.clone());
        let ensures = f
            .ensures
            .iter()
            .map(|e| l.expr(e))
            .collect::<Result<Vec<_>>>()?;
        let name = l.string(&f.name);
        let result_name = l.string("result");
        let behavioral_effects = typed.effects[&f.name]
            .effects
            .iter()
            .map(|s| l.string(s))
            .collect();
        let capabilities = typed.effects[&f.name]
            .capabilities
            .iter()
            .map(|s| l.string(s))
            .collect();
        l.p.functions.push(Function {
            name,
            parameters,
            parameter_types,
            behavioral_effects,
            capabilities,
            result,
            requires,
            ensures,
            result_name,
            body,
            effects: MemoryEffectsV1::default(),
            loc: loc(f.span),
        });
    }
    if module.functions.is_empty() {
        let body = l.block(&module.statements, false)?;
        let result = body
            .last()
            .and_then(|s| {
                if let Statement::SExpr(e) = s {
                    Some(l.p.expressions[*e].ty)
                } else {
                    None
                }
            })
            .unwrap_or(2);
        let name = l.string("<script>");
        let result_name = l.string("result");
        let mut behavior = BTreeSet::new();
        let mut caps = BTreeSet::new();
        for e in &l.p.expressions {
            match e.op {
                Op::ERef(_) | Op::ETry(_) => {
                    behavior.insert("ManagedAlloc");
                }
                Op::EGet(_) => {
                    behavior.insert("ReadManaged");
                }
                Op::ESet(..) => {
                    behavior.insert("Mutate");
                }
                Op::ECollect => {
                    behavior.insert("Collect");
                    caps.insert("MemoryCollect");
                }
                Op::EData(10 | 11, _) => {
                    behavior.insert("ReadArgs");
                    caps.insert("ArgsRead");
                }
                _ => {}
            }
        }
        fn assigned(body: &[Statement]) -> bool {
            body.iter().any(|s| match s {
                Statement::SAssign(..) => true,
                Statement::SIf(_, a, b) => assigned(a) || assigned(b),
                Statement::SMatch(_, arms) => arms.iter().any(|a| assigned(&a.body)),
                Statement::SRange(_, _, _, body)
                | Statement::SGuard(_, _, body)
                | Statement::SFor(_, _, body) => assigned(body),
                _ => false,
            })
        }
        if assigned(&body) {
            behavior.insert("Mutate");
        }
        let behavioral_effects = behavior.into_iter().map(|s| l.string(s)).collect();
        let capabilities = caps.into_iter().map(|s| l.string(s)).collect();
        l.p.functions.push(Function {
            name,
            parameters: vec![],
            parameter_types: vec![],
            behavioral_effects,
            capabilities,
            result,
            requires: vec![],
            ensures: vec![],
            result_name,
            body,
            effects: MemoryEffectsV1::default(),
            loc: loc(module.span),
        });
    } else {
        l.p.entry = l.functions.get("main").copied().unwrap_or(usize::MAX);
        if l.p.entry != usize::MAX && !module.functions[l.p.entry].parameters.is_empty() {
            return Err(vec![diagnostic(
                "main requires zero parameters",
                module.functions[l.p.entry].span,
            )]);
        }
    }
    infer_effects(&mut l.p)?;
    for (i, e) in l.p.expressions.iter().enumerate() {
        let operands = match &e.op {
            Op::ECall(_, xs) | Op::EConstruct(_, xs) | Op::EData(_, xs) => xs.clone(),
            Op::ERecord(xs) => xs.iter().map(|(_, e)| *e).collect(),
            Op::EField(e, _) | Op::ETry(e) | Op::ERef(e) | Op::EGet(e) => vec![*e],
            Op::EBinary(a, _, b) | Op::ESet(a, b) => vec![*a, *b],
            _ => vec![],
        };
        l.p.root_maps.push(RootMapV1 {
            expression: i,
            operands,
            may_allocate_managed: matches!(
                e.op,
                Op::EString(_)
                    | Op::ERecord(_)
                    | Op::EConstruct(..)
                    | Op::ERef(_)
                    | Op::EData(1..=9, _)
                    | Op::EData(11, _)
            ) || matches!(&e.op,Op::ECall(f,_) if l.p.functions[*f].effects.may_allocate_managed),
            root_result: !matches!(
                l.p.layouts[e.ty].shape,
                Shape::KInt | Shape::KBool | Shape::KUnit | Shape::KError
            ),
        });
    }
    if l.p
        .functions
        .get(l.p.entry)
        .is_some_and(|f| contains_ref(&l.p, f.result, &mut BTreeSet::new()))
    {
        return Err(vec![diagnostic(
            "reference-bearing public result snapshots are unsupported",
            module.span,
        )]);
    }
    Ok((l.p, l.types))
}
fn contains_ref(p: &Program, t: usize, seen: &mut BTreeSet<usize>) -> bool {
    if !seen.insert(t) {
        return false;
    }
    match &p.layouts[t].shape {
        Shape::KRef(_) => true,
        Shape::KList(t) | Shape::KArray(t) => contains_ref(p, *t, seen),
        Shape::KRecord(fs) => fs.iter().any(|(_, t)| contains_ref(p, *t, seen)),
        Shape::KVariant(cs) => cs
            .iter()
            .any(|(_, fs)| fs.iter().any(|t| contains_ref(p, *t, seen))),
        _ => false,
    }
}
fn infer_effects(p: &mut Program) -> Result<()> {
    fn expr(p: &Program, e: usize) -> MemoryEffectsV1 {
        let ex = &p.expressions[e];
        let mut effect = MemoryEffectsV1::default();
        let children = match &ex.op {
            Op::ECall(f, xs) => {
                effect = p.functions[*f].effects;
                xs.clone()
            }
            Op::ERecord(xs) => xs.iter().map(|(_, e)| *e).collect(),
            Op::EConstruct(_, xs) | Op::EData(_, xs) => xs.clone(),
            Op::EField(e, _) | Op::ETry(e) | Op::ERef(e) | Op::EGet(e) => vec![*e],
            Op::EBinary(a, _, b) | Op::ESet(a, b) => vec![*a, *b],
            _ => vec![],
        };
        if matches!(
            ex.op,
            Op::EString(_)
                | Op::ERecord(_)
                | Op::EConstruct(..)
                | Op::ERef(_)
                | Op::EData(1..=9, _)
                | Op::EData(11, _)
        ) {
            effect.may_allocate_managed = true;
            effect.may_auto_collect_at_allocation = true;
        }
        if matches!(ex.op, Op::ECollect) {
            effect.may_explicitly_collect = true;
        }
        if !matches!(
            p.layouts[ex.ty].shape,
            Shape::KInt | Shape::KBool | Shape::KUnit | Shape::KError
        ) {
            effect.may_allocate_root_bookkeeping = true;
        }
        for c in children {
            effect = effect.union(expr(p, c));
        }
        effect
    }
    fn block(p: &Program, body: &[Statement], guard: bool) -> Result<MemoryEffectsV1> {
        let mut effect = MemoryEffectsV1::default();
        for st in body {
            if matches!(st, Statement::SGuard(..)) {
                effect.may_allocate_root_bookkeeping = true;
            }
            let (es, bs, g): (Vec<usize>, Vec<&[Statement]>, bool) = match st {
                Statement::SLet(_, e) | Statement::SAssign(_, _, e) | Statement::SExpr(e) => {
                    (vec![*e], vec![], guard)
                }
                Statement::SReturn(e) => (e.iter().copied().collect(), vec![], guard),
                Statement::SIf(e, a, b) => (vec![*e], vec![a, b], guard),
                Statement::SMatch(e, arms) => (
                    vec![*e],
                    arms.iter().map(|a| a.body.as_slice()).collect(),
                    guard,
                ),
                Statement::SFor(_, e, b) => (vec![*e], vec![b], guard),
                Statement::SRange(_, a, b, body) => (vec![*a, *b], vec![body], guard),
                Statement::SGuard(_, _, body) => (vec![], vec![body], true),
            };
            for e in es {
                let current = expr(p, e);
                if guard && current.may_explicitly_collect {
                    return Err(vec![diagnostic(
                        "explicit collection is forbidden transitively under no_gc",
                        Span {
                            start: p.expressions[e].loc.start,
                            end: p.expressions[e].loc.end,
                        },
                    )]);
                }
                effect = effect.union(current);
            }
            for b in bs {
                effect = effect.union(block(p, b, g)?);
            }
        }
        Ok(effect)
    }
    for _ in 0..=p.functions.len() {
        let old = p.functions.iter().map(|f| f.effects).collect::<Vec<_>>();
        let mut next = vec![];
        for f in &p.functions {
            let mut e = block(p, &f.body, false)?;
            e.may_allocate_root_bookkeeping |= f.parameter_types.iter().any(|t| {
                !matches!(
                    p.layouts[*t].shape,
                    Shape::KInt | Shape::KBool | Shape::KUnit | Shape::KError
                )
            });
            for c in f.requires.iter().chain(&f.ensures) {
                e = e.union(expr(p, *c));
            }
            next.push(e);
        }
        for (f, e) in p.functions.iter_mut().zip(next) {
            f.effects = e;
        }
        if p.functions.iter().map(|f| f.effects).eq(old) {
            return Ok(());
        }
    }
    Err(vec![diagnostic(
        "managed effect fixed point failed",
        Span { start: 0, end: 0 },
    )])
}
