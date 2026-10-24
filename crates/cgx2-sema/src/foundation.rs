use crate::{Type, TypeDiagnostic, TypedModule};
use cgx2_syntax::{Module, Path, Span, TypeSyntax};
pub(crate) fn syntax_type(ty: &Type, span: Span) -> TypeSyntax {
    let (name, arguments) = match ty {
        Type::I64 => ("i64".into(), vec![]),
        Type::Bool => ("Bool".into(), vec![]),
        Type::String => ("String".into(), vec![]),
        Type::Unit => ("Unit".into(), vec![]),
        Type::Record(n) => (n.clone(), vec![]),
        Type::Parameter { name, .. } => (name.rsplit("::").next().unwrap_or(name).into(), vec![]),
        Type::List(t) => ("List".into(), vec![syntax_type(t, span)]),
        Type::Array(t) => ("Array".into(), vec![syntax_type(t, span)]),
        Type::Variant { name, arguments } | Type::AppliedRecord { name, arguments } => (
            name.strip_prefix("cgx2.core.").unwrap_or(name).into(),
            arguments.iter().map(|t| syntax_type(t, span)).collect(),
        ),
    };
    TypeSyntax {
        path: Path {
            segments: vec![name],
            span,
        },
        arguments,
        span,
    }
}
use cgx2_syntax::{Expr, ExprKind, Function, Stmt, StmtKind};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Merge only already checked package snapshots. Source admission happens before
/// this internal qualification; it never grants access to a private declaration.
pub fn merge_modules(modules: &[&Module], entry: &str) -> Module {
    let known: BTreeSet<_> = modules
        .iter()
        .flat_map(|m| {
            m.variants
                .iter()
                .map(|v| crate::nominal::identity(m, &v.name))
        })
        .collect();
    let mut result = modules
        .first()
        .map(|m| (*m).clone())
        .unwrap_or_else(|| Module {
            name: None,
            imports: vec![],
            records: vec![],
            variants: vec![],
            functions: vec![],
            statements: vec![],
            span: Span { start: 0, end: 0 },
        });
    result.name = None;
    result.imports.clear();
    result.records.clear();
    result.variants.clear();
    result.functions.clear();
    result.statements.clear();
    let mut names = BTreeMap::new();
    for m in modules {
        for f in &m.functions {
            let id = crate::nominal::identity(m, &f.name);
            names.insert(
                id.clone(),
                if m.name
                    .as_ref()
                    .is_some_and(|p| p.segments.join(".") == entry)
                    && f.name == "main"
                {
                    "main".into()
                } else {
                    id
                },
            );
        }
    }
    for m in modules {
        let mut canonical = cgx2_syntax::canonical_module(m, &known);
        fn qualify(expr: &mut Expr, m: &Module, names: &BTreeMap<String, String>) {
            match &mut expr.kind {
                ExprKind::Call { callee, arguments } => {
                    if let ExprKind::Name(p) = &mut callee.kind {
                        let n = p.segments.join(".");
                        let id = if p.segments.len() == 1 {
                            crate::nominal::identity(m, &n)
                        } else {
                            n.clone()
                        };
                        if let Some(target) = names.get(&id) {
                            p.segments = vec![target.clone()];
                        }
                    }
                    for a in arguments {
                        qualify(&mut a.value, m, names);
                    }
                }
                ExprKind::Constructor {
                    ty,
                    case,
                    arguments,
                } => {
                    if case.is_empty() {
                        let n = ty.path.segments.join(".");
                        let id = if ty.path.segments.len() == 1 {
                            crate::nominal::identity(m, &n)
                        } else {
                            n
                        };
                        if let Some(target) = names.get(&id) {
                            ty.path.segments = vec![target.clone()];
                        }
                    }
                    for a in arguments {
                        qualify(&mut a.value, m, names);
                    }
                }
                ExprKind::Binary { left, right, .. }
                | ExprKind::Index {
                    collection: left,
                    index: right,
                } => {
                    qualify(left, m, names);
                    qualify(right, m, names);
                }
                ExprKind::Collection(v) => {
                    for e in v {
                        qualify(e, m, names);
                    }
                }
                _ => {}
            }
        }
        fn block(body: &mut [Stmt], m: &Module, names: &BTreeMap<String, String>) {
            for s in body {
                match &mut s.kind {
                    StmtKind::Match { scrutinee, arms } => {
                        qualify(scrutinee, m, names);
                        for a in arms {
                            block(&mut a.body, m, names);
                        }
                    }
                    StmtKind::If {
                        condition,
                        then_body,
                        else_body,
                    } => {
                        qualify(condition, m, names);
                        block(then_body, m, names);
                        block(else_body, m, names);
                    }
                    StmtKind::For { iter, body, .. } | StmtKind::NoGc { budget: iter, body } => {
                        qualify(iter, m, names);
                        block(body, m, names);
                    }
                    StmtKind::Let { value, .. }
                    | StmtKind::Assign { value, .. }
                    | StmtKind::Expr(value)
                    | StmtKind::Return(Some(value))
                    | StmtKind::LoopContract {
                        expression: value, ..
                    } => qualify(value, m, names),
                    _ => {}
                }
            }
        }
        for r in &mut canonical.records {
            r.name = crate::nominal::identity(m, &r.name);
        }
        for v in &mut canonical.variants {
            v.name = crate::nominal::identity(m, &v.name);
        }
        for f in &mut canonical.functions {
            let id = crate::nominal::identity(m, &f.name);
            f.name = names[&id].clone();
            block(&mut f.body, m, &names);
            for e in f.requires.iter_mut().chain(&mut f.ensures) {
                qualify(e, m, &names);
            }
        }
        result.records.extend(canonical.records);
        result.variants.extend(canonical.variants);
        result.functions.extend(canonical.functions);
        if m.name
            .as_ref()
            .is_some_and(|p| p.segments.join(".") == entry)
        {
            result.statements = canonical.statements;
            block(&mut result.statements, m, &names);
        }
    }
    result
}

/// Bounded deterministic specialization of a checked finite value module.
/// Templates are checked symbolically first; this pass does not infer legality.
/// Every emitted declaration is internal; public source/API evidence belongs to
/// the unchanged input, rather than the concrete implementation module.
pub fn lower_module(module: &Module, typed: &TypedModule) -> Result<Module, Vec<TypeDiagnostic>> {
    let checked = crate::check_module_with_imports(module, &Default::default())?;
    if &checked != typed {
        return Err(vec![TypeDiagnostic::new(
            "CGT010",
            "specialization requires an unchanged body-checked module".into(),
            module.span,
        )]);
    }

    let known = module
        .variants
        .iter()
        .map(|v| crate::nominal::identity(module, &v.name))
        .collect();
    let canonical = cgx2_syntax::canonical_module(module, &known);
    let mut lower = Lower {
        module,
        typed,
        pending: VecDeque::new(),
        functions: BTreeMap::new(),
        aggregates: BTreeMap::new(),
        errors: vec![],
    };
    for f in &canonical.functions {
        if f.type_parameters.is_empty() {
            lower.pending.push_back((f.name.clone(), vec![], 0));
        }
    }
    let mut statements = canonical.statements.clone();
    lower.block(&mut statements, "", &BTreeMap::new(), 0);
    while let Some((name, args, depth)) = lower.pending.pop_front() {
        let key = specialization(&name, &args);
        if lower.functions.contains_key(&key) {
            continue;
        }
        if lower.functions.len() + lower.aggregates.len() >= 256 || depth >= 32 {
            lower.errors.push(TypeDiagnostic::new(
                "CGT030",
                "specialization count/depth exceeds 256/32".into(),
                module.span,
            ));
            break;
        }
        let Some(original) = canonical.functions.iter().find(|f| f.name == name) else {
            lower.errors.push(TypeDiagnostic::new(
                "CGT030",
                format!("missing specialization source `{name}`"),
                module.span,
            ));
            break;
        };
        let signature = &typed.functions[&name];
        let map = signature
            .type_parameters
            .iter()
            .zip(&args)
            .filter_map(|(p, t)| {
                if let Type::Parameter { name, .. } = p {
                    Some((name.clone(), t.clone()))
                } else {
                    None
                }
            })
            .collect();
        let mut f = original.clone();
        f.name = key.clone();
        // A concrete instance is internal implementation evidence, not a new
        // source export. The unchanged template was admitted above, including
        // its public signature, effects and constraints. Concrete bodies still
        // undergo ordinary checking, including caller-private substitutions.
        // This also applies to monomorphic callers: their concrete interfaces
        // can refer to an internal aggregate instance of a public template.
        f.public = false;
        f.type_parameters.clear();
        // Publish before traversing calls so ordinary recursion has one instance.
        lower.functions.insert(key.clone(), f.clone());
        for p in &mut f.parameters {
            lower.annotation(&mut p.ty, &name, &map);
        }
        if let Some(t) = &mut f.return_type {
            lower.annotation(t, &name, &map);
        }
        lower.block(&mut f.body, &name, &map, depth);
        for e in f.requires.iter_mut().chain(&mut f.ensures) {
            lower.expr(e, &name, &map, depth);
        }
        lower.functions.insert(key, f);
    }
    // Aggregate discovery reaches a fixed point through concrete fields/payloads.
    let mut done = BTreeSet::new();
    let mut records = vec![];
    let mut variants = vec![];
    loop {
        let next = lower
            .aggregates
            .iter()
            .find(|(n, _)| !done.contains(*n))
            .map(|(n, t)| (n.clone(), t.clone()));
        let Some((key, ty)) = next else { break };
        done.insert(key.clone());
        if done.len() + lower.functions.len() > 256 {
            lower.errors.push(TypeDiagnostic::new(
                "CGT030",
                "specialization count exceeds 256".into(),
                module.span,
            ));
            break;
        }
        if let Some(fields) =
            crate::nominal::members(&ty, module, &Default::default(), &mut lower.errors)
        {
            let name = match &ty {
                Type::Record(n) | Type::AppliedRecord { name: n, .. } => n,
                _ => unreachable!(),
            };
            if let Some(original) = module
                .records
                .iter()
                .find(|r| crate::nominal::identity(module, &r.name) == *name)
            {
                let mut r = original.clone();
                r.name = key;
                r.public = false;
                r.type_parameters.clear();
                for (field, (_, t)) in r.fields.iter_mut().zip(fields) {
                    let t = lower.concrete(&t);
                    field.ty = syntax_type(&t, field.ty.span);
                }
                records.push(r);
            }
        } else if let Some(cases) =
            crate::nominal::cases(&ty, module, &Default::default(), &mut lower.errors)
        {
            let Type::Variant { name, .. } = &ty else {
                continue;
            };
            if let Some(original) = module
                .variants
                .iter()
                .find(|v| crate::nominal::identity(module, &v.name) == *name)
            {
                let mut v = original.clone();
                v.name = key;
                v.public = false;
                v.type_parameters.clear();
                for (c, (_, ts)) in v.cases.iter_mut().zip(cases) {
                    for (p, t) in c.payloads.iter_mut().zip(ts) {
                        let t = lower.concrete(&t);
                        *p = syntax_type(&t, p.span);
                    }
                }
                variants.push(v);
            }
        }
    }
    if !lower.errors.is_empty() {
        return Err(lower.errors);
    }
    let mut output = canonical;
    output.records = records;
    output.variants = variants;
    output.functions = lower.functions.into_values().collect();
    output.statements = statements;
    Ok(output)
}
fn specialization(name: &str, args: &[Type]) -> String {
    if args.is_empty() {
        name.into()
    } else {
        format!(
            "{name}<{}>",
            args.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}
struct Lower<'a> {
    module: &'a Module,
    typed: &'a TypedModule,
    pending: VecDeque<(String, Vec<Type>, usize)>,
    functions: BTreeMap<String, Function>,
    aggregates: BTreeMap<String, Type>,
    errors: Vec<TypeDiagnostic>,
}
impl Lower<'_> {
    fn substitute(&mut self, t: &Type, map: &BTreeMap<String, Type>) -> Type {
        crate::nominal::try_substitute(t, map).unwrap_or_else(|| {
            self.errors.push(TypeDiagnostic::new(
                "CGT030",
                "type expansion exceeds 4096 nodes or depth 64".into(),
                self.module.span,
            ));
            Type::Unit
        })
    }

    fn concrete(&mut self, ty: &Type) -> Type {
        match ty {
            Type::Parameter { .. } => {
                self.errors.push(TypeDiagnostic::new(
                    "CGT040",
                    "unresolved specialization parameter".into(),
                    self.module.span,
                ));
                Type::Unit
            }
            Type::List(t) => Type::List(Box::new(self.concrete(t))),
            Type::Array(t) => Type::Array(Box::new(self.concrete(t))),
            Type::Record(name) => {
                self.aggregates.insert(name.clone(), ty.clone());
                ty.clone()
            }
            Type::AppliedRecord { name, arguments } if name == "cgx2.managed.Ref" => {
                Type::AppliedRecord {
                    name: name.clone(),
                    arguments: arguments.iter().map(|t| self.concrete(t)).collect(),
                }
            }
            Type::AppliedRecord { name, arguments } => {
                let key = specialization(name, arguments);
                self.aggregates.insert(key.clone(), ty.clone());
                Type::Record(key)
            }
            Type::Variant { name, arguments } if !name.starts_with("cgx2.core.") => {
                let key = specialization(name, arguments);
                self.aggregates.insert(key.clone(), ty.clone());
                Type::Variant {
                    name: key,
                    arguments: vec![],
                }
            }
            Type::Variant { name, arguments } => Type::Variant {
                name: name.clone(),
                arguments: arguments.iter().map(|t| self.concrete(t)).collect(),
            },
            _ => ty.clone(),
        }
    }
    fn annotation(&mut self, syntax: &mut TypeSyntax, owner: &str, map: &BTreeMap<String, Type>) {
        let parameters = self
            .module
            .functions
            .iter()
            .find(|f| f.name == owner)
            .map(|f| f.type_parameters.as_slice())
            .unwrap_or(&[]);
        let scope = crate::nominal::binders(self.module, owner, parameters, &mut self.errors);
        if let Some(t) = crate::nominal::scoped(
            syntax,
            self.module,
            &Default::default(),
            &scope,
            &mut self.errors,
        ) {
            let t = self.substitute(&t, map);
            let t = self.concrete(&t);
            *syntax = syntax_type(&t, syntax.span);
        }
    }
    fn expression_type(
        &mut self,
        owner: &str,
        span: Span,
        map: &BTreeMap<String, Type>,
    ) -> Option<Type> {
        let ty = self
            .typed
            .bodies
            .get(owner)
            .and_then(|b| b.get(span))
            .or_else(|| self.typed.expressions.get(span))?
            .clone();
        Some(self.substitute(&ty, map))
    }
    fn expr(&mut self, e: &mut Expr, owner: &str, map: &BTreeMap<String, Type>, depth: usize) {
        let resolved = self.expression_type(owner, e.span, map);
        if let Some(t) = &resolved {
            self.concrete(t);
        }
        match &mut e.kind {
            ExprKind::Call { callee, arguments } => {
                for a in &mut *arguments {
                    self.expr(&mut a.value, owner, map, depth);
                }
                if let Some(call) = self
                    .typed
                    .calls
                    .get(&(owner.into(), e.span.start, e.span.end))
                {
                    let arguments = call.arguments.clone();
                    let args: Vec<_> = arguments.iter().map(|t| self.substitute(t, map)).collect();
                    let key = specialization(&call.callee, &args);
                    self.pending
                        .push_back((call.callee.clone(), args, depth + 1));
                    callee.kind = ExprKind::Name(Path {
                        segments: vec![key],
                        span: callee.span,
                    });
                } else if let Some(t @ (Type::Record(_) | Type::AppliedRecord { .. })) = resolved {
                    let name = match &t {
                        Type::Record(name) | Type::AppliedRecord { name, .. } => name,
                        _ => unreachable!(),
                    };
                    // Use the checked nominal identity rather than payload naming.
                    // A function returning a record is still a call.
                    if matches!(&callee.kind, ExprKind::Name(path)
                        if crate::nominal::identity(self.module, &path.segments.join(".")) == *name)
                    {
                        let t = self.concrete(&t);
                        callee.kind = ExprKind::Name(Path {
                            segments: vec![t.to_string()],
                            span: callee.span,
                        });
                    }
                }
            }
            ExprKind::Constructor {
                ty,
                case,
                arguments,
            } => {
                for a in &mut *arguments {
                    self.expr(&mut a.value, owner, map, depth);
                }
                if case.is_empty() {
                    let target = if let Some(call) =
                        self.typed
                            .calls
                            .get(&(owner.into(), e.span.start, e.span.end))
                    {
                        let arguments = call.arguments.clone();
                        let args: Vec<_> =
                            arguments.iter().map(|t| self.substitute(t, map)).collect();
                        let key = specialization(&call.callee, &args);
                        self.pending
                            .push_back((call.callee.clone(), args, depth + 1));
                        key
                    } else {
                        resolved
                            .as_ref()
                            .map(|t| self.concrete(t).to_string())
                            .unwrap_or_else(|| ty.path.segments.join("."))
                    };
                    e.kind = ExprKind::Call {
                        callee: Box::new(Expr {
                            kind: ExprKind::Name(Path {
                                segments: vec![target],
                                span: ty.span,
                            }),
                            span: ty.span,
                        }),
                        arguments: arguments.clone(),
                    };
                } else if let Some(t) = resolved {
                    let t = self.concrete(&t);
                    *ty = syntax_type(&t, ty.span);
                }
            }
            ExprKind::Collection(values) => {
                for v in values {
                    self.expr(v, owner, map, depth);
                }
            }
            ExprKind::Binary { left, right, .. }
            | ExprKind::Index {
                collection: left,
                index: right,
            } => {
                self.expr(left, owner, map, depth);
                self.expr(right, owner, map, depth);
            }
            _ => {}
        }
    }
    fn block(
        &mut self,
        body: &mut [Stmt],
        owner: &str,
        map: &BTreeMap<String, Type>,
        depth: usize,
    ) {
        for s in body {
            match &mut s.kind {
                StmtKind::Match { scrutinee, arms } => {
                    let actual = self.expression_type(owner, scrutinee.span, map);
                    self.expr(scrutinee, owner, map, depth);
                    for a in arms {
                        if let cgx2_syntax::Pattern::Case { ty, .. } = &mut a.pattern {
                            if let Some(t) = &actual {
                                let t = self.concrete(t);
                                *ty = syntax_type(&t, ty.span);
                            }
                        }
                        self.block(&mut a.body, owner, map, depth);
                    }
                }
                StmtKind::Let {
                    annotation, value, ..
                } => {
                    if let Some(t) = annotation {
                        self.annotation(t, owner, map);
                    }
                    self.expr(value, owner, map, depth);
                }
                StmtKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expr(condition, owner, map, depth);
                    self.block(then_body, owner, map, depth);
                    self.block(else_body, owner, map, depth);
                }
                StmtKind::For { iter, body, .. } | StmtKind::NoGc { budget: iter, body } => {
                    self.expr(iter, owner, map, depth);
                    self.block(body, owner, map, depth);
                }
                StmtKind::Assign { value, .. }
                | StmtKind::Expr(value)
                | StmtKind::Return(Some(value))
                | StmtKind::LoopContract {
                    expression: value, ..
                } => self.expr(value, owner, map, depth),
                _ => {}
            }
        }
    }
}
