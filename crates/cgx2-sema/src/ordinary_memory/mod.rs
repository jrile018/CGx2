//! Authenticated ordinary-memory source producer. Legacy copied-value models
//! never authorize these semantics merely by carrying their erased types.
use crate::{TypeDiagnostic, TypedModule};
use cgx2_runtime::ordinary_memory::MemoryProgramV1 as RuntimeProgram;
pub use cgx2_runtime::ordinary_memory::{EntryPolicyV1, MemoryOpV1};
use cgx2_syntax::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
mod effects;
mod provenance;
pub use crate::default_managed::ManagedProvenanceV1 as MemoryProvenanceV1;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryEffectsV2 {
    pub may_allocate_managed: bool,
    pub may_auto_collect_at_allocation: bool,
    pub may_explicitly_collect: bool,
    pub may_allocate_root_bookkeeping: bool,
    pub may_enter_region: bool,
    pub may_create_region: bool,
    pub may_enter_guard: bool,
    pub may_allocate_owned: bool,
    pub may_transfer_graph: bool,
    pub may_enter_session: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemorySignatureV1 {
    pub name: String,
    pub region_parameters: Vec<String>,
    pub parameters: Vec<TypeSyntax>,
    pub result: Option<TypeSyntax>,
}
#[derive(Clone, Debug)]
pub struct MemoryProgramV1 {
    provenance: MemoryProvenanceV1,
    original: Module,
    typed: TypedModule,
    types: Vec<crate::Type>,
    runtime: RuntimeProgram,
    signatures: Vec<MemorySignatureV1>,
    effects: Vec<MemoryEffectsV2>,
    digest: String,
}
type Checked<T> = Result<T, Vec<TypeDiagnostic>>;
fn diagnostic(message: impl Into<String>, span: Span) -> Vec<TypeDiagnostic> {
    vec![TypeDiagnostic {
        code: "CGT095",
        message: message.into(),
        span,
    }]
}
fn path(e: &Expr) -> Option<String> {
    if let ExprKind::Name(p) = &e.kind {
        Some(p.segments.join("."))
    } else {
        None
    }
}
fn call(e: &Expr) -> Option<(String, &[CallArg])> {
    if let ExprKind::Call { callee, arguments } = &e.kind {
        Some((path(callee)?, arguments))
    } else {
        None
    }
}
fn builtin(module: &Module, name: &str) -> bool {
    cgx2_syntax::ordinary_memory_builtin(module, name)
}
fn nominal(module: &Module, name: &str) -> bool {
    let identity = |n: &str| {
        module.name.as_ref().map_or_else(
            || n.to_owned(),
            |m| format!("{}.{}", m.segments.join("."), n),
        )
    };
    module
        .records
        .iter()
        .any(|r| r.name == name || identity(&r.name) == name)
        || module
            .variants
            .iter()
            .any(|v| v.name == name || identity(&v.name) == name)
}
pub fn profile(module: &Module) -> bool {
    let found = std::cell::Cell::new(false);
    for body in std::iter::once(&module.statements).chain(module.functions.iter().map(|f| &f.body))
    {
        visit_stmts(body, &mut |_| {}, &mut |e| {
            if call(e).is_some_and(|(n, _)| n != "memory.collect" && builtin(module, &n)) {
                found.set(true);
            }
        });
    }
    found.get()
}
impl MemoryProgramV1 {
    pub fn types(&self) -> &[crate::Type] {
        &self.types
    }
    pub fn typed(&self) -> &TypedModule {
        &self.typed
    }
    pub fn signatures(&self) -> &[MemorySignatureV1] {
        &self.signatures
    }
    pub fn effects(&self) -> &[MemoryEffectsV2] {
        &self.effects
    }
    pub fn entry_policy(&self) -> EntryPolicyV1 {
        self.runtime.entry_policy
    }
    pub fn analysis_sha256(&self) -> &str {
        &self.digest
    }
    pub fn source_sha256(&self) -> String {
        match &self.provenance {
            MemoryProvenanceV1::Source { text } => format!("{:x}", Sha256::digest(text.as_bytes())),
            _ => String::new(),
        }
    }
    pub fn provenance_v1(&self) -> &MemoryProvenanceV1 {
        &self.provenance
    }
    pub fn runtime(&self) -> &RuntimeProgram {
        &self.runtime
    }
    pub fn authenticate(&self) -> Checked<()> {
        let rebuilt = match &self.provenance {
            MemoryProvenanceV1::Source { text } => compile_source(text)?,
            MemoryProvenanceV1::AstOnly => compile_ast(&self.original)?,
            MemoryProvenanceV1::SourceBundle { sources, entry } => {
                compile_source_bundle_v1(sources, entry)?
            }
        };
        if rebuilt.original != self.original
            || rebuilt.runtime != self.runtime
            || rebuilt.typed != self.typed
            || rebuilt.types != self.types
            || rebuilt.signatures != self.signatures
            || rebuilt.effects != self.effects
            || rebuilt.provenance != self.provenance
            || rebuilt.digest != self.digest
        {
            return Err(diagnostic(
                "ordinary-memory source/model authentication mismatch",
                self.original.span,
            ));
        }
        Ok(())
    }
    pub fn execute(&self, args: &[String]) -> Checked<cgx2_runtime::ordinary_memory::ExecutionV1> {
        self.authenticate()?;
        Ok(cgx2_runtime::ordinary_memory::run(&self.runtime, args))
    }
    pub fn execute_with_session(
        &self,
        args: &[String],
        session: cgx2_runtime::Session,
    ) -> Checked<cgx2_runtime::ordinary_memory::ExecutionV1> {
        self.authenticate()?;
        Ok(cgx2_runtime::ordinary_memory::run_with_session(
            &self.runtime,
            args,
            session,
        ))
    }
}
#[derive(Clone)]
enum Marker {
    DefaultRegion,
    TryIn,
    CopyTo,
    Collect,
    Session,
    Guard,
    Region {
        manual: bool,
        limit: usize,
        trigger: Option<usize>,
    },
}
struct Rewrite<'a> {
    original: &'a Module,
    markers: BTreeMap<String, Marker>,
    region_positions: BTreeMap<String, BTreeSet<usize>>,
    regions: BTreeSet<String>,
    stubs: Vec<Function>,
}
impl Rewrite<'_> {
    fn ty(&self, t: &mut TypeSyntax) -> Checked<()> {
        let name = t.path.segments.join(".");
        match name.as_str() {
            "memory.Value" => {
                if nominal(self.original, "memory.Value") {
                    return Err(diagnostic("memory.Value resolves to a user nominal declaration; it cannot authorize compiler provenance",t.span));
                }
                if t.arguments.len() != 2 {
                    return Err(diagnostic(
                        "memory.Value requires a region name and a managed-compatible type",
                        t.span,
                    ));
                }
                if provenance::contains_region_capability(&t.arguments[1]) {
                    return Err(diagnostic(
                        "memory.Value cannot hide a region capability or loan",
                        t.span,
                    ));
                }
                let region = t.arguments[0].path.segments.join(".");
                if !self.regions.contains(&region) && region != "memory.Default" {
                    return Err(diagnostic(
                        "unbound memory region name",
                        t.arguments[0].span,
                    ));
                }
                let mut inner = t.arguments[1].clone();
                self.ty(&mut inner)?;
                *t = TypeSyntax {
                    span: t.span,
                    ..inner
                };
                return Ok(());
            }
            "shared"
                if t.arguments.len() == 1
                    && t.arguments[0].path.segments.join(".") == "memory.Region" =>
            {
                let inner = &t.arguments[0];
                if nominal(self.original, "memory.Region") {
                    return Err(diagnostic("memory.Region resolves to a user nominal declaration; it cannot authorize a compiler capability",t.span));
                }
                if inner.arguments.len() != 1
                    || (!self
                        .regions
                        .contains(&inner.arguments[0].path.segments.join("."))
                        && inner.arguments[0].path.segments.join(".") != "memory.Default")
                {
                    return Err(diagnostic(
                        "shared region capability requires a bound region name",
                        t.span,
                    ));
                }
                t.path.segments = vec!["i64".into()];
                t.arguments.clear();
                return Ok(());
            }
            "memory.Region" => {
                return Err(diagnostic(
                    "region parameters require shared<memory.Region<R>>",
                    t.span,
                ))
            }
            "memory.RegionName" | "memory.Default" => {
                return Err(diagnostic(
                    "region names are compiler-only binders, not values",
                    t.span,
                ))
            }
            "owned" | "shared" | "exclusive" => return Err(diagnostic(
                "aggregate ownership lowering is not available in this ordinary-memory producer",
                t.span,
            )),
            _ => {}
        }
        let remove = self.region_positions.get(&name);
        let mut args = vec![];
        for (i, mut a) in std::mem::take(&mut t.arguments).into_iter().enumerate() {
            if remove.is_some_and(|r| r.contains(&i)) {
                let region = a.path.segments.join(".");
                if !a.arguments.is_empty()
                    || (!self.regions.contains(&region) && region != "memory.Default")
                {
                    return Err(diagnostic(
                        "explicit region arguments require compiler region names",
                        a.span,
                    ));
                }
                continue;
            }
            if provenance::contains_region_capability(&a) {
                return Err(diagnostic(
                    "region capabilities cannot be ordinary generic value arguments",
                    a.span,
                ));
            }
            self.ty(&mut a)?;
            args.push(a);
        }
        t.arguments = args;
        Ok(())
    }
    fn stub(
        &mut self,
        name: &str,
        parameters: &[CallArg],
        result: &str,
        body: &str,
        generic: bool,
    ) -> Checked<String> {
        let unique = format!("cgx2_om_{}_{}", name, self.stubs.len());
        let arguments = parameters
            .iter()
            .enumerate()
            .map(|(i, a)| {
                format!(
                    "p{i}: {}",
                    if name == "try_in" || name == "copy_to" {
                        if i == 1 {
                            "T"
                        } else {
                            "i64"
                        }
                    } else {
                        let _ = a;
                        "i64"
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let declaration = if name == "default_region" {
            "effects()"
        } else if name == "collect" {
            "effects(Collect) caps(MemoryCollect)"
        } else {
            "effects(ManagedAlloc)"
        };
        let body = if matches!(name, "default_region" | "collect") {
            body.to_owned()
        } else {
            format!("let cgx2_effect = managed.ref(0)\n    {body}")
        };
        let source = format!(
            "fn {unique}{}({arguments}) -> {result} {declaration}:\n    {body}\n",
            if generic { "<T>" } else { "" }
        );
        let mut m = parse_module(&source).map_err(|ds| {
            ds.into_iter()
                .map(|d| TypeDiagnostic {
                    code: d.code,
                    message: d.message,
                    span: d.span,
                })
                .collect::<Vec<_>>()
        })?;
        let mut function = m.functions.remove(0);
        let identity = format!("$cgx2$ordinary_memory$stub${}", self.stubs.len());
        function.name = identity.clone();
        // These parsed template references must target a compiler-only symbol.
        // '$' is not a source identifier, so source cannot call an injected stub.
        for statement in &mut function.body {
            if let StmtKind::Expr(Expr {
                kind: ExprKind::Constructor { arguments, .. },
                ..
            }) = &mut statement.kind
            {
                for argument in arguments {
                    if let ExprKind::Call { callee, .. } = &mut argument.value.kind {
                        if let ExprKind::Name(path) = &mut callee.kind {
                            if path.segments == ["cgx2_om_unit"] {
                                path.segments = vec!["$cgx2$ordinary_memory$unit".into()];
                            }
                        }
                    }
                }
            }
        }
        self.stubs.push(function);
        Ok(identity)
    }
    fn expr(&mut self, e: &mut Expr, direct: bool) -> Checked<()> {
        if let Some((name, args)) = call(e).map(|(n, a)| (n, a.to_vec())) {
            if builtin(self.original, &name) {
                let mut arguments = args;
                let marker = match name.as_str() {
                    "memory.try_session" => {
                        if !direct || !arguments.is_empty() {
                            return Err(diagnostic(
                                "memory.try_session must be the entire direct entry match",
                                e.span,
                            ));
                        }
                        Marker::Session
                    }
                    "memory.try_no_gc" => {
                        if !direct
                            || arguments.len() != 1
                            || arguments[0].name.as_deref() != Some("budget")
                        {
                            return Err(diagnostic(
                                "memory.try_no_gc requires a direct exhaustive match and budget: N",
                                e.span,
                            ));
                        }
                        literal_bound(&arguments[0].value, false)?;
                        Marker::Guard
                    }
                    "memory.try_region" => {
                        if !direct {
                            return Err(diagnostic(
                                "memory.try_region requires a direct exhaustive Result match",
                                e.span,
                            ));
                        }
                        let mut names = BTreeSet::new();
                        let mut mode = None;
                        let mut limit = None;
                        let mut trigger = None;
                        for (i, a) in arguments.iter_mut().enumerate() {
                            let n = a.name.as_deref().ok_or_else(|| {
                                diagnostic("region entry arguments must be named", a.span)
                            })?;
                            if !names.insert(n.to_owned()) {
                                return Err(diagnostic("duplicate region entry argument", a.span));
                            }
                            match n {
                                "mode" => {
                                    if nominal(self.original, "memory.Mode") {
                                        return Err(diagnostic("region mode resolves to a user nominal declaration, not the compiler mode",a.value.span));
                                    }
                                    let m = path(&a.value);
                                    mode=Some(match m.as_deref(){Some("memory.Mode.Manual")=>true,Some("memory.Mode.Auto")=>false,_=>return Err(diagnostic("region mode must be memory.Mode.Manual or memory.Mode.Auto",a.value.span))});
                                    a.value.kind = ExprKind::Integer("0".into());
                                }
                                "limit" => {
                                    literal_bound(&a.value, false)?;
                                    limit = Some(i);
                                }
                                "trigger" => {
                                    literal_bound(&a.value, true)?;
                                    trigger = Some(i);
                                }
                                _ => {
                                    return Err(diagnostic("unknown region entry argument", a.span))
                                }
                            }
                        }
                        let manual =
                            mode.ok_or_else(|| diagnostic("region mode is required", e.span))?;
                        let limit =
                            limit.ok_or_else(|| diagnostic("region limit is required", e.span))?;
                        if manual && trigger.is_some() {
                            return Err(diagnostic(
                                "Manual regions do not accept a trigger",
                                e.span,
                            ));
                        }
                        Marker::Region {
                            manual,
                            limit,
                            trigger,
                        }
                    }
                    "memory.default_region" => {
                        if !arguments.is_empty() {
                            return Err(diagnostic(
                                "memory.default_region takes no arguments",
                                e.span,
                            ));
                        }
                        Marker::DefaultRegion
                    }
                    "memory.try_in" | "memory.copy_to" => {
                        if arguments.len() != 2 || arguments.iter().any(|a| a.name.is_some()) {
                            return Err(diagnostic(
                                "graph allocation/transfer requires (borrow(region), value)",
                                e.span,
                            ));
                        }
                        if name == "memory.try_in" {
                            Marker::TryIn
                        } else {
                            Marker::CopyTo
                        }
                    }
                    "memory.collect" => {
                        if arguments.len() != 1 {
                            return Err(diagnostic(
                                "memory.collect requires one borrowed region",
                                e.span,
                            ));
                        }
                        Marker::Collect
                    }
                    _ => {
                        return Err(diagnostic(
                            "owned graph conversion is not yet connected to this producer",
                            e.span,
                        ))
                    }
                };
                for a in &mut arguments {
                    self.expr(&mut a.value, false)?;
                    a.name = None;
                }
                let (short, result, body, generic) = match marker {
                    Marker::DefaultRegion => ("default_region", "i64", "0", false),
                    Marker::Session => (
                        "session",
                        "cgx2.core.Result<Unit, memory.SessionEntryError>",
                        "cgx2.core.Result<Unit, memory.SessionEntryError>.Ok(cgx2_om_unit())",
                        false,
                    ),
                    Marker::Guard => (
                        "guard",
                        "cgx2.core.Result<Unit, cgx2.core.AllocationError>",
                        "cgx2.core.Result<Unit, cgx2.core.AllocationError>.Ok(cgx2_om_unit())",
                        false,
                    ),
                    Marker::Region { .. } => (
                        "region",
                        "cgx2.core.Result<i64, cgx2.core.AllocationError>",
                        "cgx2.core.Result<i64, cgx2.core.AllocationError>.Ok(0)",
                        false,
                    ),
                    Marker::TryIn => (
                        "try_in",
                        "cgx2.core.Result<T, cgx2.core.AllocationError>",
                        "cgx2.core.Result<T, cgx2.core.AllocationError>.Ok(p1)",
                        true,
                    ),
                    Marker::CopyTo => (
                        "copy_to",
                        "cgx2.core.Result<T, cgx2.core.AllocationError>",
                        "cgx2.core.Result<T, cgx2.core.AllocationError>.Ok(p1)",
                        true,
                    ),
                    Marker::Collect => ("collect", "Unit", "managed.collect()", false),
                };
                let stub = self.stub(short, &arguments, result, body, generic)?;
                self.markers.insert(stub.clone(), marker);
                e.kind = ExprKind::Call {
                    callee: Box::new(Expr {
                        kind: ExprKind::Name(Path {
                            segments: vec![stub],
                            span: e.span,
                        }),
                        span: e.span,
                    }),
                    arguments,
                };
                return Ok(());
            }
            if name == "borrow" && args.len() == 1 && args[0].name.is_none() {
                let span = e.span;
                *e = args[0].value.clone();
                e.span = span;
                return self.expr(e, false);
            }
        }
        match &mut e.kind {
            ExprKind::Call { callee, arguments } => {
                if let ExprKind::Name(path) = &mut callee.kind {
                    let name = path.segments.join(".");
                    if let Some(f) = self.original.functions.iter().find(|f| {
                        self.original
                            .name
                            .as_ref()
                            .is_some_and(|m| format!("{}.{}", m.segments.join("."), f.name) == name)
                    }) {
                        path.segments = vec![f.name.clone()];
                    }
                }
                self.expr(callee, false)?;
                for a in arguments {
                    self.expr(&mut a.value, false)?;
                }
            }
            ExprKind::Constructor { ty, arguments, .. } => {
                self.ty(ty)?;
                for a in arguments {
                    self.expr(&mut a.value, false)?;
                }
            }
            ExprKind::Collection(xs) => {
                for x in xs {
                    self.expr(x, false)?;
                }
            }
            ExprKind::Index { collection, index } => {
                self.expr(collection, false)?;
                self.expr(index, false)?;
            }
            ExprKind::Binary { left, right, .. } => {
                self.expr(left, false)?;
                self.expr(right, false)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn block(&mut self, body: &mut [Stmt]) -> Checked<()> {
        for s in body {
            match &mut s.kind {
                StmtKind::Match { scrutinee, arms } => {
                    if call(scrutinee).is_some_and(|(n, _)| {
                        builtin(self.original, &n)
                            && matches!(
                                n.as_str(),
                                "memory.try_region" | "memory.try_no_gc" | "memory.try_session"
                            )
                    }) {
                        let mut cases = BTreeSet::new();
                        for arm in arms.iter() {
                            if let Pattern::Case {
                                ty, case, bindings, ..
                            } = &arm.pattern
                            {
                                if !matches!(
                                    ty.path.segments.join(".").as_str(),
                                    "Result" | "cgx2.core.Result"
                                ) || bindings.len() != 1
                                    || !cases.insert(case.as_str())
                                {
                                    return Err(diagnostic("entry requires exact Result.Ok and Result.Err payload arms",arm.span));
                                }
                            } else {
                                return Err(diagnostic(
                                    "entry requires explicit exhaustive Result arms",
                                    arm.span,
                                ));
                            }
                        }
                        if cases != BTreeSet::from(["Ok", "Err"]) {
                            return Err(diagnostic(
                                "entry requires exact exhaustive Result.Ok/Result.Err arms",
                                s.span,
                            ));
                        }
                    }
                    self.expr(scrutinee, true)?;
                    for arm in arms {
                        if let Pattern::Case { ty, .. } = &mut arm.pattern {
                            self.ty(ty)?;
                        }
                        self.block(&mut arm.body)?;
                    }
                }
                StmtKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expr(condition, false)?;
                    self.block(then_body)?;
                    self.block(else_body)?;
                }
                StmtKind::For { iter, body, .. } => {
                    self.expr(iter, false)?;
                    self.block(body)?;
                }
                StmtKind::NoGc { budget, body } => {
                    self.expr(budget, false)?;
                    self.block(body)?;
                }
                StmtKind::Let {
                    annotation, value, ..
                } => {
                    if let Some(t) = annotation {
                        self.ty(t)?;
                    }
                    self.expr(value, false)?;
                }
                StmtKind::Assign { value, .. }
                | StmtKind::Expr(value)
                | StmtKind::Return(Some(value))
                | StmtKind::LoopContract {
                    expression: value, ..
                } => self.expr(value, false)?,
                StmtKind::Return(None) => {}
            }
        }
        Ok(())
    }
}
fn literal_bound(e: &Expr, positive: bool) -> Checked<()> {
    if let ExprKind::Integer(n) = &e.kind {
        if n.replace('_', "")
            .parse::<i64>()
            .ok()
            .is_none_or(|n| n < 0 || (positive && n == 0))
        {
            return Err(diagnostic("invalid literal memory bound", e.span));
        }
    }
    Ok(())
}
fn region_parameters(parameters: &[TypeParameter]) -> BTreeSet<usize> {
    parameters
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            p.constraints
                .iter()
                .any(|c| c.segments.join(".") == "memory.RegionName")
                .then_some(i)
        })
        .collect()
}
pub fn compile_source(source: &str) -> Checked<MemoryProgramV1> {
    if source.len() > 4 << 20 {
        return Err(diagnostic(
            "ordinary-memory source exceeds 4 MiB",
            Span { start: 0, end: 0 },
        ));
    }
    let original = parse_module(source).map_err(|ds| {
        ds.into_iter()
            .map(|d| TypeDiagnostic {
                code: d.code,
                message: d.message,
                span: d.span,
            })
            .collect::<Vec<_>>()
    })?;
    compile(
        original,
        MemoryProvenanceV1::Source {
            text: source.into(),
        },
    )
}

/// AST construction authenticates the supplied tree without claiming original source bytes.
pub fn compile_ast(module: &Module) -> Checked<MemoryProgramV1> {
    // SpanTypes is indexed per function by (start,end). A caller-supplied AST
    // must not alias a heap expression's checked type with a scalar expression.
    for f in &module.functions {
        let mut spans = BTreeSet::new();
        let mut duplicate = None;
        let mut inspect = |e: &Expr| {
            if !spans.insert((e.span.start, e.span.end)) {
                duplicate = Some(e.span);
            }
        };
        for e in f.requires.iter().chain(&f.ensures) {
            visit_expr(e, &mut inspect);
        }
        visit_stmts(&f.body, &mut |_| {}, &mut inspect);
        if let Some(span) = duplicate {
            return Err(diagnostic(
                "ordinary-memory AST expressions require distinct spans within each function",
                span,
            ));
        }
    }
    compile(module.clone(), MemoryProvenanceV1::AstOnly)
}

/// Reparse and check every dependency, including private and unused bodies,
/// before merging. Source identities remain separate in the authenticated input.
pub fn compile_source_bundle_v1(
    sources: &[(String, String)],
    entry: &str,
) -> Checked<MemoryProgramV1> {
    let merged =
        crate::default_managed::source_bundle_module(sources, entry, |module, modules| {
            let mut resolution = module.clone();
            for dependency in modules.iter().filter(|candidate| {
                candidate
                    .name
                    .as_ref()
                    .is_some_and(|name| module.imports.iter().any(|p| p.segments == name.segments))
            }) {
                let prefix = dependency.name.as_ref().unwrap().segments.join(".");
                for f in &dependency.functions {
                    let mut f = f.clone();
                    f.name = format!("{prefix}.{}", f.name);
                    resolution.functions.push(f);
                }
                for r in &dependency.records {
                    let mut r = r.clone();
                    r.name = format!("{prefix}.{}", r.name);
                    resolution.records.push(r);
                }
                for v in &dependency.variants {
                    let mut v = v.clone();
                    v.name = format!("{prefix}.{}", v.name);
                    resolution.variants.push(v);
                }
            }
            rewrite_module(module, &resolution).map(|(module, _)| module)
        })?;
    compile(
        merged,
        MemoryProvenanceV1::SourceBundle {
            sources: sources.into(),
            entry: entry.into(),
        },
    )
}

fn compile(original: Module, provenance: MemoryProvenanceV1) -> Checked<MemoryProgramV1> {
    // AST-only callers can construct names the lexer cannot spell. They must
    // not address or collide with compiler-injected operation templates.
    let mut internal = None;
    let identifier = |name: &str| {
        let mut chars = name.chars();
        chars.next().is_some_and(|c| c == '_' || c.is_alphabetic())
            && chars.all(|c| c == '_' || c.is_alphanumeric())
    };
    for parameter in original
        .functions
        .iter()
        .flat_map(|f| &f.type_parameters)
        .chain(original.records.iter().flat_map(|r| &r.type_parameters))
        .chain(original.variants.iter().flat_map(|v| &v.type_parameters))
    {
        if !identifier(&parameter.name) {
            return Err(diagnostic("memory type binders require source identifiers; compiler region names cannot be manufactured", parameter.span));
        }
    }
    for f in &original.functions {
        if f.name.contains('$') {
            internal = Some(f.span);
        }
        let mut inspect = |e: &Expr| {
            let path = match &e.kind {
                ExprKind::Name(path) => Some(path),
                ExprKind::Constructor { ty, .. } => Some(&ty.path),
                _ => None,
            };
            if path.is_some_and(|p| {
                p.segments
                    .iter()
                    .any(|s| s.starts_with("$cgx2$ordinary_memory$"))
            }) {
                internal = Some(e.span);
            }
        };
        for e in f.requires.iter().chain(&f.ensures) {
            cgx2_syntax::visit_expr(e, &mut inspect);
        }
        visit_stmts(&f.body, &mut |_| {}, &mut inspect);
    }
    if let Some(span) = internal {
        return Err(diagnostic(
            "compiler-internal identifiers are not admissible in ordinary-memory input ASTs",
            span,
        ));
    }
    if !original.statements.is_empty() {
        return Err(diagnostic(
            "ordinary-memory source requires ordinary functions",
            original.span,
        ));
    }
    if original
        .records
        .iter()
        .any(|r| r.name.starts_with("memory."))
        || original
            .variants
            .iter()
            .any(|r| r.name.starts_with("memory."))
    {
        return Err(diagnostic(
            "user declarations cannot manufacture compiler memory identities",
            original.span,
        ));
    }
    provenance::validate_capability_positions(&original)?;
    let signatures = original
        .functions
        .iter()
        .map(|f| MemorySignatureV1 {
            name: f.name.clone(),
            region_parameters: region_parameters(&f.type_parameters)
                .into_iter()
                .map(|i| f.type_parameters[i].name.clone())
                .collect(),
            parameters: f.parameters.iter().map(|p| p.ty.clone()).collect(),
            result: f.return_type.clone(),
        })
        .collect::<Vec<_>>();
    let (module, markers) = rewrite_module(&original, &original)?;
    let typed = crate::check_module(&module)?;
    provenance::validate(&original, &module, &typed, &signatures)?;
    let concrete = crate::lower_module(&module, &typed)?;
    let checked = crate::check_module_with_imports(&concrete, &Default::default())?;
    let (program, types) = crate::default_managed::lower::lower(&concrete, &checked)?;
    let mut operations = BTreeMap::new();
    for (i, e) in program.expressions.iter().enumerate() {
        let cgx2_runtime::default_managed::model::Op::ECall(callee, args) = &e.op else {
            continue;
        };
        let name = &program.strings[program.functions[*callee].name];
        if let Some(marker) = markers.get(name.split('<').next().unwrap_or(name)) {
            let op = match marker {
                Marker::DefaultRegion => MemoryOpV1::DefaultRegion,
                Marker::TryIn => MemoryOpV1::TryIn {
                    region: args[0],
                    value: args[1],
                },
                Marker::CopyTo => MemoryOpV1::CopyTo {
                    region: args[0],
                    value: args[1],
                },
                Marker::Collect => MemoryOpV1::Collect { region: args[0] },
                Marker::Guard => MemoryOpV1::GuardEntry { budget: args[0] },
                Marker::Session => MemoryOpV1::SessionEntry,
                Marker::Region {
                    manual,
                    limit,
                    trigger,
                } => MemoryOpV1::RegionEntry {
                    manual: *manual,
                    arguments: args.clone(),
                    limit: *limit,
                    trigger: *trigger,
                },
            };
            operations.insert(i, op);
        }
    }
    let entry_policy = provenance::entry_policy(&original)?;
    let runtime = RuntimeProgram {
        program,
        operations,
        entry_policy,
    };
    provenance::validate_runtime(&runtime)?;
    let effects = effects::infer(&runtime);
    let digest = format!(
        "{:x}",
        Sha256::digest(
            format!("CGX2-ORDINARY-MEMORY-V1\0{provenance:?}\0{original:?}\0{runtime:?}{signatures:?}{effects:?}")
                .as_bytes()
        )
    );
    Ok(MemoryProgramV1 {
        provenance,
        original,
        typed,
        types,
        runtime,
        signatures,
        effects,
        digest,
    })
}

fn rewrite_module(
    original: &Module,
    resolution: &Module,
) -> Checked<(Module, BTreeMap<String, Marker>)> {
    let mut rewrite = Rewrite {
        original: resolution,
        markers: BTreeMap::new(),
        region_positions: BTreeMap::new(),
        regions: BTreeSet::new(),
        stubs: vec![],
    };
    for (n, p) in resolution
        .functions
        .iter()
        .map(|f| (&f.name, &f.type_parameters))
        .chain(
            resolution
                .records
                .iter()
                .map(|r| (&r.name, &r.type_parameters)),
        )
        .chain(
            resolution
                .variants
                .iter()
                .map(|v| (&v.name, &v.type_parameters)),
        )
    {
        rewrite
            .region_positions
            .insert(n.clone(), region_parameters(p));
    }
    let mut module = original.clone();
    for f in &mut module.functions {
        rewrite.regions = f
            .type_parameters
            .iter()
            .filter(|p| {
                p.constraints
                    .iter()
                    .any(|c| c.segments.join(".") == "memory.RegionName")
            })
            .map(|p| p.name.clone())
            .collect();
        if f.type_parameters.iter().any(|p| {
            p.constraints
                .iter()
                .any(|c| c.segments.join(".") == "memory.RegionName")
                && p.constraints.len() != 1
        }) {
            return Err(diagnostic(
                "RegionName cannot be combined with value constraints",
                f.span,
            ));
        }
        f.type_parameters
            .retain(|p| !rewrite.regions.contains(&p.name));
        for p in &mut f.parameters {
            rewrite.ty(&mut p.ty)?;
        }
        if let Some(t) = &mut f.return_type {
            rewrite.ty(t)?;
        }
        for e in f.requires.iter_mut().chain(&mut f.ensures) {
            rewrite.expr(e, false)?;
        }
        rewrite.block(&mut f.body)?;
    }
    for r in &mut module.records {
        rewrite.regions = r
            .type_parameters
            .iter()
            .filter(|p| {
                p.constraints
                    .iter()
                    .any(|c| c.segments.join(".") == "memory.RegionName")
            })
            .map(|p| p.name.clone())
            .collect();
        r.type_parameters
            .retain(|p| !rewrite.regions.contains(&p.name));
        for f in &mut r.fields {
            rewrite.ty(&mut f.ty)?;
        }
    }
    for v in &mut module.variants {
        rewrite.regions = v
            .type_parameters
            .iter()
            .filter(|p| {
                p.constraints
                    .iter()
                    .any(|c| c.segments.join(".") == "memory.RegionName")
            })
            .map(|p| p.name.clone())
            .collect();
        v.type_parameters
            .retain(|p| !rewrite.regions.contains(&p.name));
        for c in &mut v.cases {
            for t in &mut c.payloads {
                rewrite.ty(t)?;
            }
        }
    }
    // Compiler-owned copied entry errors, represented only by the entry engine.
    let mut intrinsic=parse_module("variant SessionReserveSite:\n    SessionRegions\nvariant SessionEntryError:\n    HostAllocation(SessionReserveSite)\n    IdentifierExhausted\n").unwrap();
    intrinsic.variants[0].name = "memory.SessionReserveSite".into();
    intrinsic.variants[1].name = "memory.SessionEntryError".into();
    intrinsic.variants[1].cases[0].payloads[0].path.segments =
        vec!["memory".into(), "SessionReserveSite".into()];
    module.variants.extend(intrinsic.variants);
    module.functions.extend(rewrite.stubs);
    let mut unit = parse_module("fn cgx2_om_unit() -> Unit:\n    return\n")
        .unwrap()
        .functions
        .remove(0);
    unit.name = "$cgx2$ordinary_memory$unit".into();
    module.functions.push(unit);
    Ok((module, rewrite.markers))
}

#[cfg(test)]
mod authentication_tests {
    use super::*;
    #[test]
    fn effects_signatures_and_provenance_are_rebuilt_before_execution() {
        let p = compile_source("fn main() -> i64 effects(ManagedAlloc):\n    match memory.try_no_gc(budget: 4096):\n        Result.Ok(guard):\n            42\n        Result.Err(error):\n            -1\n").unwrap();
        let mut effects = p.clone();
        effects.effects[effects.runtime.program.entry].may_enter_guard = false;
        let mut signature = p.clone();
        signature.signatures[0]
            .region_parameters
            .push("forged".into());
        let mut provenance = p.clone();
        provenance.provenance = MemoryProvenanceV1::AstOnly;
        for forged in [effects, signature, provenance] {
            let errors = forged.execute(&[]).unwrap_err();
            assert_eq!(errors[0].code, "CGT095");
            assert!(errors[0].message.contains("authentication"));
        }
    }
}
