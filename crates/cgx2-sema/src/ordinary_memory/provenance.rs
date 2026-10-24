use super::*;
use crate::Type;
use cgx2_runtime::default_managed::model::{Op, Program, Shape, Statement};
const DEFAULT: &str = "memory.Default";
#[derive(Clone, Debug, PartialEq, Eq)]
enum Value {
    Scalar,
    Heap(String),
    Capability {
        region: Option<String>,
        borrowed: bool,
    },
}
pub(super) fn local_name<'a>(module: &'a Module, name: &str) -> Option<&'a str> {
    module
        .functions
        .iter()
        .find(|f| {
            f.name == name
                || module
                    .name
                    .as_ref()
                    .is_some_and(|m| format!("{}.{}", m.segments.join("."), f.name) == name)
        })
        .map(|f| f.name.as_str())
}
fn is_heap(t: &Type) -> bool {
    match t {
        Type::I64 | Type::Bool | Type::Unit => false,
        Type::Variant { name, .. }
            if name == "cgx2.core.AllocationError"
                || name == "memory.SessionEntryError"
                || name == "memory.SessionReserveSite" =>
        {
            false
        }
        _ => true,
    }
}
pub(super) fn cap(t: &TypeSyntax) -> Option<String> {
    if t.path.segments.join(".") == "shared" && t.arguments.len() == 1 {
        let t = &t.arguments[0];
        if t.path.segments.join(".") == "memory.Region" && t.arguments.len() == 1 {
            return Some(t.arguments[0].path.segments.join("."));
        }
    }
    None
}
pub(super) fn contains_region_capability(t: &TypeSyntax) -> bool {
    t.path.segments.join(".") == "memory.Region"
        || t.arguments.iter().any(contains_region_capability)
}
pub(super) fn validate_capability_positions(module: &Module) -> Checked<()> {
    for f in &module.functions {
        for p in &f.parameters {
            if contains_region_capability(&p.ty) && cap(&p.ty).is_none() {
                return Err(diagnostic(
                    "region capabilities require a direct shared region parameter",
                    p.ty.span,
                ));
            }
        }
        if let Some(t) = &f.return_type {
            if contains_region_capability(t) {
                return Err(diagnostic(
                    "memory capabilities cannot be manufactured or returned by a function",
                    t.span,
                ));
            }
        }
    }
    for t in module
        .records
        .iter()
        .flat_map(|r| r.fields.iter().map(|f| &f.ty))
        .chain(
            module
                .variants
                .iter()
                .flat_map(|v| v.cases.iter().flat_map(|c| c.payloads.iter())),
        )
    {
        if contains_region_capability(t) {
            return Err(diagnostic(
                "managed records and variants cannot store memory capabilities",
                t.span,
            ));
        }
    }
    Ok(())
}
fn qualification(t: &TypeSyntax) -> Option<String> {
    if t.path.segments.join(".") == "memory.Value" && t.arguments.len() == 2 {
        Some(t.arguments[0].path.segments.join("."))
    } else {
        t.arguments.iter().find_map(qualification)
    }
}
fn validate_type(t: &TypeSyntax, outer: Option<&str>) -> Checked<()> {
    let name = t.path.segments.join(".");
    if name == "memory.Value" && t.arguments.len() == 2 {
        let r = t.arguments[0].path.segments.join(".");
        let value = &t.arguments[1];
        let scalar = matches!(
            value.path.segments.join(".").as_str(),
            "i64" | "Bool" | "Unit"
        );
        if !scalar && outer.is_some_and(|other| other != r) {
            return Err(diagnostic(
                "conflicting nested nontrivial memory.Value brands",
                t.span,
            ));
        }
        validate_type(value, if scalar { None } else { Some(&r) })
    } else {
        if outer.is_some()
            && matches!(
                name.as_str(),
                "owned" | "shared" | "exclusive" | "memory.Region"
            )
        {
            return Err(diagnostic(
                "managed graphs cannot contain ownership, loans, or region capabilities",
                t.span,
            ));
        }
        for a in &t.arguments {
            validate_type(a, outer)?;
        }
        Ok(())
    }
}
pub(super) fn entry_policy(module: &Module) -> Checked<EntryPolicyV1> {
    let mut entries = vec![];
    for f in &module.functions {
        visit_stmts(&f.body, &mut |_| {}, &mut |e| {
            if call(e).is_some_and(|(n, _)| builtin(module, &n) && n == "memory.try_session") {
                entries.push((f, e.span));
            }
        });
    }
    if entries.is_empty() {
        return Ok(EntryPolicyV1::Implicit);
    }
    let (f, span) = entries[0];
    if entries.len() != 1
        || f.name != "main"
        || f.body.len() != 1
        || !f.requires.is_empty()
        || !f.ensures.is_empty()
    {
        return Err(diagnostic(
            "recoverable Session entry requires one entire main match and no entry contracts",
            span,
        ));
    }
    if !matches!(&f.body[0].kind,StmtKind::Match{scrutinee,..} if scrutinee.span==span) {
        return Err(diagnostic(
            "memory.try_session must be the direct scrutinee of the entire main body",
            span,
        ));
    }
    if f.return_type.as_ref().is_some_and(|t| {
        !matches!(t.path.segments.join(".").as_str(), "i64" | "Bool" | "Unit")
            || !t.arguments.is_empty()
    }) {
        return Err(diagnostic(
            "recoverable Session main must return i64, Bool, or Unit",
            f.span,
        ));
    }
    Ok(EntryPolicyV1::Recoverable)
}
struct Analyzer<'a> {
    module: &'a Module,
    typed: &'a TypedModule,
    function: &'a Function,
    signatures: &'a [MemorySignatureV1],
    env: Vec<BTreeMap<String, Value>>,
    collecting: &'a BTreeSet<String>,
    guard: bool,
}
impl Analyzer<'_> {
    fn ty(&self, e: &Expr) -> Option<&Type> {
        self.typed
            .bodies
            .get(&self.function.name)
            .and_then(|t| t.get(e.span))
    }
    fn value(&self, e: &Expr, brand: &str) -> Value {
        if self.ty(e).is_some_and(is_heap) {
            Value::Heap(brand.into())
        } else {
            Value::Scalar
        }
    }
    fn lookup(&self, n: &str) -> Option<Value> {
        self.env.iter().rev().find_map(|s| s.get(n).cloned())
    }
    fn expected_return(&self, value: &Value, span: Span) -> Checked<()> {
        if matches!(value, Value::Capability { .. }) {
            return Err(diagnostic(
                "memory capability cannot escape through a return",
                span,
            ));
        }
        let expected = self
            .function
            .return_type
            .as_ref()
            .and_then(qualification)
            .unwrap_or_else(|| DEFAULT.into());
        if let Value::Heap(actual) = value {
            if *actual != expected {
                return Err(diagnostic(
                    "function result erases or changes region provenance",
                    span,
                ));
            }
        }
        Ok(())
    }
    fn region(&mut self, e: &Expr) -> Checked<String> {
        match self.expr(e, DEFAULT)? {
            Value::Capability {
                region: Some(r),
                borrowed: true,
            } => Ok(r),
            _ => Err(diagnostic(
                "allocation/collection requires a borrowed region capability",
                e.span,
            )),
        }
    }
    fn arguments(&mut self, args: &[CallArg], context: &str) -> Checked<Vec<Value>> {
        args.iter().map(|a| self.expr(&a.value, context)).collect()
    }
    fn fresh(&self, e: &Expr, values: &[Value], context: &str) -> Checked<Value> {
        for value in values {
            match value {
                Value::Capability { .. } => {
                    return Err(diagnostic(
                        "memory capability cannot be stored in an aggregate",
                        e.span,
                    ))
                }
                Value::Heap(r) if r != context => {
                    return Err(diagnostic(
                        "cross-region graph construction requires explicit memory.copy_to",
                        e.span,
                    ))
                }
                _ => {}
            }
        }
        Ok(self.value(e, context))
    }
    fn expr(&mut self, e: &Expr, context: &str) -> Checked<Value> {
        if let Some((name, args)) = call(e) {
            if builtin(self.module, &name) {
                return match name.as_str() {
                    "memory.default_region" => Ok(Value::Capability {
                        region: Some(DEFAULT.into()),
                        borrowed: true,
                    }),
                    "memory.try_in" | "memory.copy_to" => {
                        let r = self.region(&args[0].value)?;
                        let value = self.expr(
                            &args[1].value,
                            if name == "memory.try_in" { &r } else { context },
                        )?;
                        if matches!(value, Value::Capability { .. }) {
                            return Err(diagnostic(
                                "recovery carriers cannot hide memory capabilities",
                                e.span,
                            ));
                        }
                        if name == "memory.try_in"
                            && matches!(&value,Value::Heap(source) if *source!=r)
                        {
                            return Err(diagnostic(
                                "try_in result retains a different allocation region",
                                args[1].value.span,
                            ));
                        }
                        Ok(if matches!(value, Value::Heap(_)) {
                            Value::Heap(r)
                        } else {
                            Value::Scalar
                        })
                    }
                    "memory.collect" => {
                        self.region(&args[0].value)?;
                        if self.guard {
                            return Err(diagnostic(
                                "explicit collection forbidden transitively under a guard",
                                e.span,
                            ));
                        }
                        Ok(Value::Scalar)
                    }
                    _ => Err(diagnostic(
                        "memory entry requires its scoped direct match",
                        e.span,
                    )),
                };
            }
            if region_borrow(self.module, &name, args) {
                return match self.expr(&args[0].value, context)? {
                    Value::Capability {
                        region: Some(r), ..
                    } => Ok(Value::Capability {
                        region: Some(r),
                        borrowed: true,
                    }),
                    _ => Err(diagnostic("borrow requires a region capability", e.span)),
                };
            }
            let values = self.arguments(args, context)?;
            let resolved = local_name(self.module, &name).unwrap_or(&name);
            if self.guard && self.collecting.contains(resolved) {
                return Err(diagnostic(
                    "explicit or unknown collection forbidden transitively under a guard",
                    e.span,
                ));
            }
            if let Some(signature) = self.signatures.iter().find(|s| s.name == resolved) {
                let mut substitution = BTreeMap::new();
                let declaration = self
                    .module
                    .functions
                    .iter()
                    .find(|f| f.name == resolved)
                    .unwrap();
                for (index, value) in values.iter().enumerate() {
                    let parameter_index = if let Some(n) = &args[index].name {
                        declaration
                            .parameters
                            .iter()
                            .position(|p| p.name == *n)
                            .unwrap_or(index)
                    } else {
                        index
                    };
                    let parameter = &signature.parameters[parameter_index];
                    if let Some(r) = cap(parameter) {
                        let Value::Capability {
                            region: Some(actual),
                            borrowed: true,
                        } = value
                        else {
                            return Err(diagnostic(
                                "region parameter requires a borrowed capability",
                                e.span,
                            ));
                        };
                        if r == DEFAULT && actual != DEFAULT {
                            return Err(diagnostic(
                                "default region parameter received a named region",
                                e.span,
                            ));
                        }
                        if let Some(old) = substitution.insert(r, actual.clone()) {
                            if old != *actual {
                                return Err(diagnostic(
                                    "inconsistent generic region arguments",
                                    e.span,
                                ));
                            }
                        }
                    } else {
                        if matches!(value, Value::Capability { .. }) {
                            return Err(diagnostic(
                                "ordinary value parameter cannot receive a memory capability",
                                e.span,
                            ));
                        }
                        let expected = qualification(parameter).unwrap_or_else(|| DEFAULT.into());
                        if let Value::Heap(actual) = value {
                            if signature.region_parameters.contains(&expected) {
                                if let Some(old) =
                                    substitution.insert(expected.clone(), actual.clone())
                                {
                                    if old != *actual {
                                        return Err(diagnostic(
                                            "invariant generic region mismatch",
                                            e.span,
                                        ));
                                    }
                                }
                            } else if expected != *actual {
                                return Err(diagnostic(
                                    "call changes heap parameter region",
                                    e.span,
                                ));
                            }
                        }
                    }
                }
                let r = signature
                    .result
                    .as_ref()
                    .and_then(qualification)
                    .unwrap_or_else(|| DEFAULT.into());
                let r = substitution.get(&r).cloned().unwrap_or(r);
                return Ok(self.value(e, &r));
            }
            if values.iter().any(|v| matches!(v, Value::Capability { .. })) {
                return Err(diagnostic(
                    "untrusted call cannot receive a memory capability",
                    e.span,
                ));
            }
            if name.starts_with("$cgx2_cmp_") || name == "length" {
                return Ok(Value::Scalar);
            }
            if matches!(
                name.as_str(),
                "get"
                    | "updated"
                    | "append"
                    | "slice"
                    | "concat"
                    | "array"
                    | "list"
                    | "show"
                    | "parse_i64"
                    | "managed.get"
                    | "managed.set"
            ) {
                let brand = values
                    .first()
                    .and_then(|v| {
                        if let Value::Heap(r) = v {
                            Some(r.as_str())
                        } else {
                            None
                        }
                    })
                    .unwrap_or(context);
                if values
                    .iter()
                    .any(|v| matches!(v,Value::Heap(r) if r!=brand))
                {
                    return Err(diagnostic(
                        "heap operation changes invariant region provenance",
                        e.span,
                    ));
                }
                return Ok(self.value(e, brand));
            }
            self.fresh(e, &values, context)
        } else {
            match &e.kind {
                ExprKind::Name(p) => {
                    if let Some(value) = self.lookup(&p.segments[0]) {
                        if p.segments.len() > 1 && matches!(value, Value::Capability { .. }) {
                            return Err(diagnostic("memory capability has no user fields", e.span));
                        }
                        Ok(
                            if matches!(value, Value::Heap(_)) && !self.ty(e).is_some_and(is_heap) {
                                Value::Scalar
                            } else {
                                value
                            },
                        )
                    } else {
                        Ok(self.value(e, context))
                    }
                }
                ExprKind::Integer(_) | ExprKind::Boolean(_) => Ok(Value::Scalar),
                ExprKind::StringLiteral(_) => Ok(Value::Heap(context.into())),
                ExprKind::Binary { left, right, .. } => {
                    let a = self.expr(left, context)?;
                    let b = self.expr(right, context)?;
                    if matches!(a, Value::Capability { .. })
                        || matches!(b, Value::Capability { .. })
                    {
                        return Err(diagnostic(
                            "memory capability cannot be used as a copied scalar",
                            e.span,
                        ));
                    }
                    Ok(Value::Scalar)
                }
                ExprKind::Collection(xs) => {
                    let values = xs
                        .iter()
                        .map(|e| self.expr(e, context))
                        .collect::<Checked<Vec<_>>>()?;
                    self.fresh(e, &values, context)
                }
                ExprKind::Constructor {
                    ty,
                    case,
                    arguments,
                } => {
                    let name = ty.path.segments.join(".");
                    if case.is_empty() && local_name(self.module, &name).is_some() {
                        let call = Expr {
                            span: e.span,
                            kind: ExprKind::Call {
                                callee: Box::new(Expr {
                                    span: e.span,
                                    kind: ExprKind::Name(ty.path.clone()),
                                }),
                                arguments: arguments.clone(),
                            },
                        };
                        self.expr(&call, context)
                    } else {
                        let values = self.arguments(arguments, context)?;
                        self.fresh(e, &values, context)
                    }
                }
                ExprKind::Index { collection, index } => {
                    let v = self.expr(collection, context)?;
                    let i = self.expr(index, context)?;
                    if !matches!(i, Value::Scalar) {
                        return Err(diagnostic("index must be a copied scalar", index.span));
                    }
                    Ok(if self.ty(e).is_some_and(is_heap) {
                        v
                    } else {
                        Value::Scalar
                    })
                }
                ExprKind::Call { .. } => Err(diagnostic(
                    "computed memory call is not authenticated",
                    e.span,
                )),
            }
        }
    }
    fn block(&mut self, body: &[Stmt], nested: bool) -> Checked<Value> {
        if nested {
            self.env.push(BTreeMap::new());
        }
        let mut tail = Value::Scalar;
        for s in body {
            tail = match &s.kind {
                StmtKind::Let {
                    name,
                    annotation,
                    value,
                    ..
                } => {
                    if let Some(t) = annotation {
                        validate_type(t, None)?;
                    }
                    let v = self.expr(value, DEFAULT)?;
                    if let Some(t) = annotation {
                        match (cap(t), &v) {
                            (
                                Some(expected),
                                Value::Capability {
                                    region: Some(actual),
                                    borrowed: true,
                                },
                            ) if expected == *actual => {}
                            (Some(_), _) => return Err(diagnostic(
                                "region loan annotation requires the matching borrowed capability",
                                t.span,
                            )),
                            (None, Value::Capability { .. }) => {
                                return Err(diagnostic(
                                    "an ordinary annotation cannot erase a memory capability",
                                    t.span,
                                ))
                            }
                            _ => {}
                        }
                    }
                    if let (Some(t), Value::Heap(actual)) = (annotation, &v) {
                        if let Some(expected) = qualification(t) {
                            if expected != *actual {
                                return Err(diagnostic(
                                    "local annotation changes region provenance",
                                    t.span,
                                ));
                            }
                        }
                    }
                    if matches!(
                        v,
                        Value::Capability {
                            borrowed: false,
                            ..
                        }
                    ) {
                        return Err(diagnostic(
                            "affine capability cannot be copied into a binding",
                            value.span,
                        ));
                    }
                    self.env.last_mut().unwrap().insert(name.clone(), v);
                    Value::Scalar
                }
                StmtKind::Assign { target, value, .. } => {
                    let v = self.expr(value, DEFAULT)?;
                    let old = self.lookup(&target.segments[0]);
                    if matches!(v, Value::Capability { .. })
                        || old
                            .as_ref()
                            .is_some_and(|old| matches!(old, Value::Capability { .. }))
                        || matches!((&old,&v),(Some(Value::Heap(a)),Value::Heap(b)) if a!=b)
                    {
                        return Err(diagnostic(
                            "assignment stores an escaping capability or changes region provenance",
                            s.span,
                        ));
                    }
                    Value::Scalar
                }
                StmtKind::Expr(e) => {
                    let v = self.expr(e, DEFAULT)?;
                    if matches!(v, Value::Capability { .. }) {
                        return Err(diagnostic(
                            "capability cannot be copied as a tail value",
                            e.span,
                        ));
                    }
                    v
                }
                StmtKind::Return(e) => {
                    let v = if let Some(e) = e {
                        self.expr(e, DEFAULT)?
                    } else {
                        Value::Scalar
                    };
                    self.expected_return(&v, s.span)?;
                    Value::Scalar
                }
                StmtKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expr(condition, DEFAULT)?;
                    self.block(then_body, true)?;
                    self.block(else_body, true)?;
                    Value::Scalar
                }
                StmtKind::For {
                    binding,
                    iter,
                    body,
                } => {
                    let value = self.expr(iter, DEFAULT)?;
                    self.env.push(BTreeMap::from([(binding.clone(), value)]));
                    self.block(body, true)?;
                    self.env.pop();
                    Value::Scalar
                }
                StmtKind::NoGc { budget, body } => {
                    self.expr(budget, DEFAULT)?;
                    let old = self.guard;
                    self.guard = true;
                    let result = self.block(body, true);
                    self.guard = old;
                    result?;
                    Value::Scalar
                }
                StmtKind::LoopContract { expression, .. } => {
                    self.expr(expression, DEFAULT)?;
                    Value::Scalar
                }
                StmtKind::Match { scrutinee, arms } => {
                    let operation = call(scrutinee).filter(|(n, _)| builtin(self.module, n));
                    let scoped = operation.as_ref().is_some_and(|(n, _)| {
                        matches!(
                            n.as_str(),
                            "memory.try_region" | "memory.try_no_gc" | "memory.try_session"
                        )
                    });
                    if scoped {
                        for a in operation.as_ref().unwrap().1 {
                            if a.name.as_deref() != Some("mode")
                                && !matches!(self.expr(&a.value, DEFAULT)?, Value::Scalar)
                            {
                                return Err(diagnostic(
                                    "memory bounds require copied scalar values",
                                    a.value.span,
                                ));
                            }
                        }
                    }
                    let value = if scoped {
                        Value::Scalar
                    } else {
                        self.expr(scrutinee, DEFAULT)?
                    };
                    // Only a resolved builtin Result with an actually copied
                    // error payload can drop the successful graph's brand.
                    // User variants named Err and Result<_, heap> retain it.
                    let scalar_error = scoped
                        || matches!(self.ty(scrutinee), Some(Type::Variant { name, arguments })
                            if name == "cgx2.core.Result"
                                && arguments.get(1).is_some_and(|t| !is_heap(t)));
                    let brand = format!(
                        "region@{}:{}..{}",
                        self.function.name, scrutinee.span.start, scrutinee.span.end
                    );
                    let mut result = None;
                    for arm in arms {
                        self.env.push(BTreeMap::new());
                        let mut guard = self.guard;
                        if let Pattern::Case { case, bindings, .. } = &arm.pattern {
                            for b in bindings {
                                let payload = if scoped && case == "Ok" {
                                    let name = &operation.as_ref().unwrap().0;
                                    if name == "memory.try_no_gc" {
                                        guard = true;
                                    }
                                    Value::Capability {
                                        region: if name == "memory.try_region" {
                                            Some(brand.clone())
                                        } else {
                                            None
                                        },
                                        borrowed: false,
                                    }
                                } else if case == "Err" && scalar_error {
                                    Value::Scalar
                                } else {
                                    value.clone()
                                };
                                self.env.last_mut().unwrap().insert(b.name.clone(), payload);
                            }
                        }
                        let old = self.guard;
                        self.guard = guard;
                        let branch = self.block(&arm.body, false);
                        self.guard = old;
                        self.env.pop();
                        let branch = branch?;
                        if matches!(&branch,Value::Heap(r) if scoped&&*r==brand)
                            || matches!(branch, Value::Capability { .. })
                        {
                            return Err(diagnostic("named region value or memory capability escapes its successful arm",arm.span));
                        }
                        if result.as_ref().is_some_and(|old| old != &branch) {
                            return Err(diagnostic(
                                "match outcomes have different region provenance",
                                arm.span,
                            ));
                        }
                        result = Some(branch);
                    }
                    result.unwrap_or(Value::Scalar)
                }
            };
        }
        if nested {
            self.env.pop();
        }
        Ok(tail)
    }
}
pub(super) fn validate(
    original: &Module,
    _erased: &Module,
    typed: &TypedModule,
    signatures: &[MemorySignatureV1],
) -> Checked<()> {
    entry_policy(original)?;
    let mut collecting = BTreeSet::new();
    for _ in 0..=original.functions.len() {
        let old = collecting.clone();
        for f in &original.functions {
            let mut collect = false;
            visit_stmts(&f.body, &mut |_| {}, &mut |e| {
                if let Some((n, _)) = call(e) {
                    if (n == "memory.collect" && builtin(original, &n))
                        || (n == "managed.collect"
                            && local_name(original, &n).is_none()
                            && typed
                                .bodies
                                .get(&f.name)
                                .and_then(|types| types.get(e.span))
                                == Some(&Type::Unit))
                        || old.contains(&n)
                        || local_name(original, &n).is_some_and(|name| old.contains(name))
                    {
                        collect = true;
                    }
                }
            });
            if collect {
                collecting.insert(f.name.clone());
            }
        }
        if old == collecting {
            break;
        }
    }
    for f in &original.functions {
        for p in &f.parameters {
            validate_type(&p.ty, None)?;
        }
        if let Some(t) = &f.return_type {
            validate_type(t, None)?;
        }
        let signature = typed
            .functions
            .get(&f.name)
            .ok_or_else(|| diagnostic("missing checked function signature", f.span))?;
        let env = f
            .parameters
            .iter()
            .zip(&signature.parameters)
            .map(|(p, t)| {
                let value = if let Some(region) = cap(&p.ty) {
                    Value::Capability {
                        region: Some(region),
                        borrowed: true,
                    }
                } else if is_heap(t) {
                    Value::Heap(qualification(&p.ty).unwrap_or_else(|| DEFAULT.into()))
                } else {
                    Value::Scalar
                };
                (p.name.clone(), value)
            })
            .collect();
        let mut analyzer = Analyzer {
            module: original,
            typed,
            function: f,
            signatures,
            env: vec![env],
            collecting: &collecting,
            guard: false,
        };
        for e in f.requires.iter().chain(&f.ensures) {
            analyzer.expr(e, DEFAULT)?;
        }
        let result = analyzer.block(&f.body, false)?;
        analyzer.expected_return(&result, f.span)?;
    }
    Ok(())
}
fn runtime_error(p: &Program, e: usize, message: &str) -> Vec<TypeDiagnostic> {
    let loc = p.expressions[e].loc;
    diagnostic(
        message,
        Span {
            start: loc.start,
            end: loc.end,
        },
    )
}
pub(super) fn validate_runtime(memory: &RuntimeProgram) -> Checked<()> {
    let p = &memory.program;
    // Inspect actual transitive bodies, including recursive SCCs, before
    // allowing anything to execute on the failed-entry path.
    fn expr(
        ops: &BTreeMap<usize, MemoryOpV1>,
        p: &Program,
        id: usize,
        seen: &mut BTreeSet<usize>,
    ) -> Checked<()> {
        if ops.contains_key(&id) {
            return Err(runtime_error(
                p,
                id,
                "Session fallback cannot access memory resources or capabilities",
            ));
        }
        match &p.expressions[id].op {
            Op::EInt(_) | Op::EBool(_) | Op::EUnit | Op::ELocal(_) => Ok(()),
            Op::EBinary(a, _, b) => {
                expr(ops, p, *a, seen)?;
                expr(ops, p, *b, seen)
            }
            Op::ECall(f, args) => {
                for a in args {
                    expr(ops, p, *a, seen)?;
                }
                if seen.insert(*f) {
                    let f = &p.functions[*f];
                    if !f.capabilities.is_empty()
                        || f.behavioral_effects
                            .iter()
                            .any(|n| !matches!(p.strings[*n].as_str(), "Mutate"))
                    {
                        return Err(runtime_error(
                            p,
                            id,
                            "Session fallback callee has a resource or unknown summary",
                        ));
                    }
                    for e in f.requires.iter().chain(&f.ensures) {
                        expr(ops, p, *e, seen)?;
                    }
                    block(ops, p, &f.body, seen)?;
                }
                Ok(())
            }
            _ => Err(runtime_error(
                p,
                id,
                "Session fallback admits only copied scalar operations and entry-error matching",
            )),
        }
    }
    fn block(
        ops: &BTreeMap<usize, MemoryOpV1>,
        p: &Program,
        body: &[Statement],
        seen: &mut BTreeSet<usize>,
    ) -> Checked<()> {
        for s in body {
            match s {
                Statement::SLet(_, e) | Statement::SExpr(e) | Statement::SAssign(_, _, e) => {
                    expr(ops, p, *e, seen)?
                }
                Statement::SReturn(e) => {
                    if let Some(e) = e {
                        expr(ops, p, *e, seen)?;
                    }
                }
                Statement::SIf(e, a, b) => {
                    expr(ops, p, *e, seen)?;
                    block(ops, p, a, seen)?;
                    block(ops, p, b, seen)?;
                }
                Statement::SRange(_, a, b, body) => {
                    expr(ops, p, *a, seen)?;
                    expr(ops, p, *b, seen)?;
                    block(ops, p, body, seen)?;
                }
                Statement::SMatch(e, arms) => {
                    expr(ops, p, *e, seen)?;
                    for a in arms {
                        block(ops, p, &a.body, seen)?;
                    }
                }
                _ => {
                    return Err(diagnostic(
                        "Session fallback contains a resource scope or unsupported operation",
                        Span { start: 0, end: 0 },
                    ))
                }
            }
        }
        Ok(())
    }
    if memory.entry_policy == EntryPolicyV1::Recoverable {
        let f = &p.functions[p.entry];
        if !matches!(
            p.layouts[f.result].shape,
            Shape::KInt | Shape::KBool | Shape::KUnit
        ) {
            return Err(diagnostic(
                "Session entry result must be copied scalar",
                Span {
                    start: f.loc.start,
                    end: f.loc.end,
                },
            ));
        }
        if let Statement::SMatch(_, arms) = &f.body[0] {
            let arm = arms.iter().find(|a| a.case == Some(1)).unwrap();
            block(&memory.operations, p, &arm.body, &mut BTreeSet::new())?;
        }
    }
    Ok(())
}
