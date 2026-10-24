use std::collections::{BTreeMap, BTreeSet};

use cgx2_syntax::{
    AssignOp, CallArg, Expr, ExprKind, Function, Module, Path, Record, Span, Stmt, StmtKind,
};

use crate::{effects, EffectSummary, Type, TypeDiagnostic};

fn block_returns(body: &[Stmt]) -> bool {
    body.iter().any(|stmt| match &stmt.kind {
        StmtKind::Return(_) => true,
        StmtKind::Match { arms, .. } => {
            !arms.is_empty() && arms.iter().all(|arm| block_returns(&arm.body))
        }
        StmtKind::If {
            then_body,
            else_body,
            ..
        } => block_returns(then_body) && block_returns(else_body),
        _ => false,
    })
}

/// A resolved function signature. Private unannotated returns are inferred from the body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionSignature {
    pub public: bool,
    pub parameters: Vec<Type>,
    pub return_type: Type,
    pub span: Span,
}

/// Expression type index. Spans are indexed by their exact `(start, end)` byte offsets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpanTypes(BTreeMap<(usize, usize), Type>);

impl SpanTypes {
    pub fn get(&self, span: Span) -> Option<&Type> {
        self.0.get(&(span.start, span.end))
    }

    pub fn iter(&self) -> impl Iterator<Item = (Span, &Type)> {
        self.0
            .iter()
            .map(|(&(start, end), ty)| (Span { start, end }, ty))
    }

    fn insert(&mut self, span: Span, ty: Type) {
        self.0.insert((span.start, span.end), ty);
    }
}

/// Typed summary of an unchanged syntax tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedModule {
    pub functions: BTreeMap<String, FunctionSignature>,
    pub expressions: SpanTypes,
    pub effects: BTreeMap<String, EffectSummary>,
}

/// Package-supplied declarations. Keys in `public_functions` are fully qualified
/// `module.function` names; only explicit source imports make them callable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportedSignatures {
    pub modules: BTreeSet<String>,
    pub public_functions: BTreeMap<String, FunctionSignature>,
    pub private_functions: BTreeSet<String>,
    /// Keys and record names are defining-module-qualified nominal identities.
    pub public_records: BTreeMap<String, Record>,
    pub private_records: BTreeSet<String>,
    pub public_variants: BTreeMap<String, cgx2_syntax::Variant>,
    pub private_variants: BTreeSet<String>,
    /// Opaque body-checked summaries. Defaults to no trusted imported effects.
    pub checked_effects: crate::CheckedImportedEffects,
}

fn record_identity(module: &Module, name: &str, _imported: &ImportedSignatures) -> String {
    crate::nominal::identity(module, name)
}
fn resolve_in_module(
    ty: &cgx2_syntax::TypeSyntax,
    module: &Module,
    imported: &ImportedSignatures,
    diagnostics: &mut Vec<TypeDiagnostic>,
) -> Option<Type> {
    crate::nominal::resolve(ty, module, imported, diagnostics)
}

#[derive(Clone)]
struct DraftSignature {
    public: bool,
    parameters: Vec<Option<Type>>,
    result: Option<Type>,
    span: Span,
}

#[derive(Clone)]
struct Binding {
    ty: Type,
    mutable: bool,
}

struct Checker {
    functions: BTreeMap<String, DraftSignature>,
    expressions: SpanTypes,
    diagnostics: Vec<TypeDiagnostic>,
    scopes: Vec<BTreeMap<String, Binding>>,
    imports: BTreeSet<String>,
    imported: ImportedSignatures,
    effect_summaries: BTreeMap<String, EffectSummary>,
    module: Module,
}

/// Check all functions and script statements, returning every discovered diagnostic.
pub fn check_module(module: &Module) -> Result<TypedModule, Vec<TypeDiagnostic>> {
    if let Some(import) = module.imports.first() {
        return Err(vec![TypeDiagnostic::new(
            "CGT011",
            "imports require static package checking; imported code cannot execute yet".into(),
            import.span,
        )]);
    }
    check_module_with_imports(module, &ImportedSignatures::default())
}

/// Check a module against declared, local package signatures without executing imports.
pub fn check_module_with_imports(
    module: &Module,
    imported: &ImportedSignatures,
) -> Result<TypedModule, Vec<TypeDiagnostic>> {
    let generic = module
        .records
        .iter()
        .flat_map(|r| &r.type_parameters)
        .chain(module.variants.iter().flat_map(|v| &v.type_parameters))
        .chain(module.functions.iter().flat_map(|f| &f.type_parameters));
    let deferred: Vec<_> = generic
        .map(|p| {
            TypeDiagnostic::new(
                "CGT032",
                "user generic declarations and named constraint semantics are deferred to S2b"
                    .into(),
                p.span,
            )
        })
        .collect();
    if !deferred.is_empty() {
        return Err(deferred);
    }
    crate::tensor::tensor_profile(module)?;
    let mut foreign_errors = Vec::new();
    for function in &module.functions {
        if let Some(binding) = &function.foreign {
            if function.name == "main" {
                foreign_errors.push(TypeDiagnostic::new(
                    "CGT016",
                    "foreign main is unsupported; declare a local main that calls the binding"
                        .into(),
                    function.span,
                ));
            }
            if binding.route == "native" {
                let tensor_shape = binding.target.segments.get(1).is_some_and(|name| {
                    name == "cgx2_torch_add"
                        || name == "cgx2_torch_matmul"
                        || name == "cgx2_torch_matmul_f32"
                }) && binding.runtime.starts_with("cgx2.libtorch.cpu@")
                    && function.parameters.len() == 2
                    && function
                        .parameters
                        .iter()
                        .all(|parameter| parameter.ty.to_string() == "String")
                    && function
                        .return_type
                        .as_ref()
                        .is_some_and(|ty| ty.to_string() == "String");
                let valid_shape = binding.unsafe_call
                    && binding.ownership == "copied"
                    && binding.target.segments.len() == 2
                    && binding
                        .runtime
                        .split_once('@')
                        .is_some_and(|(library, version)| {
                            !library.is_empty() && !version.is_empty()
                        })
                    && (tensor_shape
                        || (function.parameters.len() == 1
                            && function.parameters[0].ty.to_string() == "i64"
                            && function
                                .return_type
                                .as_ref()
                                .is_some_and(|ty| ty.to_string() == "i64")));
                if !valid_shape {
                    foreign_errors.push(TypeDiagnostic::new(
                        "CGT016",
                        "native C calls require unsafe foreign native, one i64 input, i64 result, binding.symbol target, LIBRARY@VERSION runtime and copied ownership".into(),
                        binding.span,
                    ));
                }
                if !function
                    .effects
                    .as_ref()
                    .is_some_and(|decl| decl.names.iter().any(|name| name.name == "Foreign"))
                {
                    foreign_errors.push(TypeDiagnostic::new(
                        "CGT012",
                        "native declaration requires effects(Foreign)".into(),
                        function.span,
                    ));
                }
                if !function
                    .capabilities
                    .as_ref()
                    .is_some_and(|decl| decl.names.iter().any(|name| name.name == "native"))
                {
                    foreign_errors.push(TypeDiagnostic::new(
                        "CGT013",
                        "native declaration requires caps(native)".into(),
                        function.span,
                    ));
                }
                continue;
            }
            if binding.unsafe_call {
                foreign_errors.push(TypeDiagnostic::new(
                    "CGT016",
                    "unsafe foreign is reserved for the native route".into(),
                    binding.span,
                ));
            }
            let supported = |ty: &cgx2_syntax::TypeSyntax| {
                matches!(ty.to_string().as_str(), "i64" | "Bool" | "String")
            };
            if function
                .return_type
                .as_ref()
                .is_none_or(|ty| !supported(ty))
            {
                foreign_errors.push(TypeDiagnostic::new(
                    "CGT016",
                    "foreign result must be i64, Bool, or String".into(),
                    function
                        .return_type
                        .as_ref()
                        .map_or(function.span, |ty| ty.span),
                ));
            }
            for parameter in &function.parameters {
                if !supported(&parameter.ty) {
                    foreign_errors.push(TypeDiagnostic::new(
                        "CGT016",
                        "foreign parameter must be copied i64, Bool, or String".into(),
                        parameter.ty.span,
                    ));
                }
            }
            if binding.route != "python"
                || binding.ownership != "copied"
                || binding.runtime != "local:cpython"
            {
                foreign_errors.push(TypeDiagnostic::new("CGT016", "Python foreign declaration requires copied ownership and runtime \"local:cpython\"".into(), binding.span));
            }
            if binding
                .target
                .segments
                .first()
                .is_some_and(|name| name == "torch")
            {
                foreign_errors.push(TypeDiagnostic::new(
                    "CGT016",
                    "torch is reserved for the direct LibTorch route".into(),
                    binding.target.span,
                ));
            }
            let target_name = binding.target.segments.join(".");
            if !((binding.target.segments.len() == 2
                && binding.target.segments[0] == "math"
                && !binding.target.segments[1].starts_with('_'))
                || matches!(target_name.as_str(), "builtins.str" | "builtins.bool"))
            {
                foreign_errors.push(TypeDiagnostic::new(
                    "CGT016",
                    "Python source calls currently allow only public math functions and builtins.str/bool; other targets need a reviewed effect and capability contract".into(),
                    binding.target.span,
                ));
            }
            if binding.target.segments.len() < 2
                || !binding.target.segments.iter().all(|part| {
                    part.bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                })
            {
                foreign_errors.push(TypeDiagnostic::new(
                    "CGT016",
                    "Python target must be a dotted ASCII module and callable".into(),
                    binding.target.span,
                ));
            }
            if function
                .effects
                .as_ref()
                .is_none_or(|decl| !decl.names.iter().any(|name| name.name == "Foreign"))
            {
                foreign_errors.push(TypeDiagnostic::new(
                    "CGT012",
                    "foreign declaration requires effects(Foreign)".into(),
                    function.span,
                ));
            }
            if function
                .capabilities
                .as_ref()
                .is_none_or(|decl| !decl.names.iter().any(|name| name.name == "ForeignCall"))
            {
                foreign_errors.push(TypeDiagnostic::new(
                    "CGT013",
                    "foreign declaration requires caps(ForeignCall)".into(),
                    function.span,
                ));
            }
        }
    }
    if !foreign_errors.is_empty() {
        return Err(foreign_errors);
    }
    if let Some(result) = crate::memory::check_memory_module(module) {
        if let Some(span) = cgx2_syntax::s2a_feature_span(module) {
            return Err(vec![TypeDiagnostic::new(
                "CGT023",
                "S2a aggregates and syntax cannot cross the memory script profile".into(),
                span,
            )]);
        }
        if let Some(record) = module.records.first() {
            return Err(vec![TypeDiagnostic::new(
                "CGT023",
                "record values cannot cross the managed or owned memory profile".into(),
                record.span,
            )]);
        }
        if !module.imports.is_empty() {
            return Err(vec![TypeDiagnostic::new(
                "CGM001",
                "imports in memory scripts are unsupported".into(),
                module.imports[0].span,
            )]);
        }
        return result;
    }
    let mut checker = Checker {
        functions: BTreeMap::new(),
        expressions: SpanTypes::default(),
        diagnostics: Vec::new(),
        scopes: Vec::new(),
        imports: module
            .imports
            .iter()
            .map(|p| p.segments.join("."))
            .collect(),
        imported: imported.clone(),
        effect_summaries: BTreeMap::new(),
        module: module.clone(),
    };
    let (effect_summaries, effect_diagnostics) = effects::analyze_with_imports(module, imported);
    checker.effect_summaries = effect_summaries;
    checker.diagnostics.extend(effect_diagnostics);
    checker.insert_builtin("arg_count", vec![], Type::I64);
    checker.insert_builtin("arg", vec![Type::I64], Type::String);
    checker.check_records();
    for function in &module.functions {
        checker.declare_function(function);
    }
    // Check callees before callers so a private tail return can be inferred
    // even when its declaration appears later in the source.
    for function in function_order(&module.functions) {
        checker.check_function(function);
    }
    if !module.statements.is_empty() {
        checker.scopes.push(BTreeMap::new());
        checker.check_block(&module.statements, None);
        checker.scopes.pop();
    }
    if !checker.diagnostics.is_empty() {
        return Err(checker.diagnostics);
    }
    let functions = checker
        .functions
        .into_iter()
        .map(|(name, signature)| {
            (
                name,
                FunctionSignature {
                    public: signature.public,
                    parameters: signature
                        .parameters
                        .into_iter()
                        .map(|ty| ty.unwrap_or(Type::Unit))
                        .collect(),
                    return_type: signature.result.unwrap_or(Type::Unit),
                    span: signature.span,
                },
            )
        })
        .collect();
    Ok(TypedModule {
        functions,
        expressions: checker.expressions,
        effects: checker.effect_summaries,
    })
}

/// Collect explicitly annotated public scalar signatures without checking bodies.
/// Package loading uses this before type-checking callers, independent of file order.
pub fn collect_public_signatures(
    module: &Module,
) -> Result<BTreeMap<String, FunctionSignature>, Vec<TypeDiagnostic>> {
    collect_public_signatures_with_imports(module, &ImportedSignatures::default())
}

/// Collect signatures with the package record environment, before checking bodies.
pub fn collect_public_signatures_with_imports(
    module: &Module,
    imported: &ImportedSignatures,
) -> Result<BTreeMap<String, FunctionSignature>, Vec<TypeDiagnostic>> {
    if let Some(parameter) = module
        .records
        .iter()
        .flat_map(|r| &r.type_parameters)
        .chain(module.variants.iter().flat_map(|v| &v.type_parameters))
        .chain(module.functions.iter().flat_map(|f| &f.type_parameters))
        .next()
    {
        return Err(vec![TypeDiagnostic::new(
            "CGT032",
            "user generic declarations and named constraint semantics are deferred to S2b".into(),
            parameter.span,
        )]);
    }
    let mut signatures = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut errors = Vec::new();
    for function in &module.functions {
        if !seen.insert(function.name.clone()) {
            errors.push(TypeDiagnostic::new(
                "CGT001",
                format!("duplicate function `{}`", function.name),
                function.span,
            ));
            continue;
        }
        if !function.public {
            continue;
        }
        let parameters: Vec<_> = function
            .parameters
            .iter()
            .map(|p| resolve_in_module(&p.ty, module, imported, &mut errors))
            .collect();
        let result = match &function.return_type {
            Some(ty) => resolve_in_module(ty, module, imported, &mut errors),
            None => {
                errors.push(TypeDiagnostic::new(
                    "CGT006",
                    "public function requires a return annotation".into(),
                    function.span,
                ));
                None
            }
        };
        if let (Some(result), true) = (result, parameters.iter().all(Option::is_some)) {
            let is_private = |ty: &Type| crate::nominal::private_type(ty, module);
            if is_private(&result) || parameters.iter().flatten().any(is_private) {
                errors.push(TypeDiagnostic::new(
                    "CGT022",
                    "public signature exposes a private nominal type".into(),
                    function.span,
                ));
                continue;
            }

            signatures.insert(
                function.name.clone(),
                FunctionSignature {
                    public: true,
                    parameters: parameters.into_iter().flatten().collect(),
                    return_type: result,
                    span: function.span,
                },
            );
        }
    }
    if errors.is_empty() {
        Ok(signatures)
    } else {
        Err(errors)
    }
}

fn function_order(functions: &[Function]) -> Vec<&Function> {
    fn calls_in_expr(expr: &Expr, calls: &mut Vec<String>) {
        match &expr.kind {
            ExprKind::Constructor { arguments, .. } => {
                for arg in arguments {
                    calls_in_expr(&arg.value, calls);
                }
            }
            ExprKind::Collection(values) => {
                for value in values {
                    calls_in_expr(value, calls);
                }
            }
            ExprKind::Index { collection, index } => {
                calls_in_expr(collection, calls);
                calls_in_expr(index, calls);
            }
            ExprKind::Call { callee, arguments } => {
                if let ExprKind::Name(path) = &callee.kind {
                    if let [name] = path.segments.as_slice() {
                        calls.push(name.clone());
                    }
                }
                calls_in_expr(callee, calls);
                for argument in arguments {
                    calls_in_expr(&argument.value, calls);
                }
            }
            ExprKind::Binary { left, right, .. } => {
                calls_in_expr(left, calls);
                calls_in_expr(right, calls);
            }
            _ => {}
        }
    }

    fn calls_in_stmt(stmt: &Stmt, calls: &mut Vec<String>) {
        match &stmt.kind {
            StmtKind::Match { scrutinee, arms } => {
                calls_in_expr(scrutinee, calls);
                for arm in arms {
                    for stmt in &arm.body {
                        calls_in_stmt(stmt, calls);
                    }
                }
            }
            StmtKind::LoopContract { expression, .. } => calls_in_expr(expression, calls),
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                calls_in_expr(condition, calls);
                for statement in then_body.iter().chain(else_body) {
                    calls_in_stmt(statement, calls);
                }
            }
            StmtKind::Let { value, .. }
            | StmtKind::Assign { value, .. }
            | StmtKind::Expr(value) => calls_in_expr(value, calls),
            StmtKind::Return(Some(value)) => calls_in_expr(value, calls),
            StmtKind::For { iter, body, .. } => {
                calls_in_expr(iter, calls);
                for statement in body {
                    calls_in_stmt(statement, calls);
                }
            }
            StmtKind::NoGc { budget, body } => {
                calls_in_expr(budget, calls);
                for statement in body {
                    calls_in_stmt(statement, calls);
                }
            }
            StmtKind::Return(None) => {}
        }
    }

    fn visit<'a>(
        name: &str,
        index: &BTreeMap<&str, &'a Function>,
        state: &mut BTreeMap<String, u8>,
        order: &mut Vec<&'a Function>,
    ) {
        if state.get(name).copied().unwrap_or(0) != 0 {
            return;
        }
        state.insert(name.into(), 1);
        if let Some(function) = index.get(name) {
            let mut calls = Vec::new();
            for contract in function.requires.iter().chain(&function.ensures) {
                calls_in_expr(contract, &mut calls);
            }
            for statement in &function.body {
                calls_in_stmt(statement, &mut calls);
            }
            for called in calls {
                if index.contains_key(called.as_str()) {
                    visit(&called, index, state, order);
                }
            }
            order.push(function);
        }
        state.insert(name.into(), 2);
    }

    let index: BTreeMap<&str, &Function> = functions
        .iter()
        .map(|function| (function.name.as_str(), function))
        .collect();
    let mut state = BTreeMap::new();
    let mut order = Vec::new();
    for function in functions {
        visit(&function.name, &index, &mut state, &mut order);
    }
    order
}

impl Checker {
    fn check_records(&mut self) {
        let mut names = BTreeSet::new();
        let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let declarations: Vec<_> = self
            .module
            .records
            .iter()
            .map(|r| {
                (
                    r.name.clone(),
                    r.public,
                    r.span,
                    r.fields.iter().map(|f| f.ty.clone()).collect::<Vec<_>>(),
                )
            })
            .chain(self.module.variants.iter().map(|v| {
                (
                    v.name.clone(),
                    v.public,
                    v.span,
                    v.cases
                        .iter()
                        .flat_map(|c| c.payloads.clone())
                        .collect::<Vec<_>>(),
                )
            }))
            .collect();
        fn dependencies(ty: &Type, out: &mut Vec<String>) {
            match ty {
                Type::Record(n) => out.push(n.clone()),
                Type::Variant { name, arguments } => {
                    out.push(name.clone());
                    for arg in arguments {
                        dependencies(arg, out);
                    }
                }
                Type::List(t) => dependencies(t, out),
                _ => {}
            }
        }
        for (name, public, span, payloads) in &declarations {
            if !names.insert(name.clone())
                || self.module.functions.iter().any(|f| f.name == *name)
                || matches!(
                    name.as_str(),
                    "i64"
                        | "Bool"
                        | "String"
                        | "Unit"
                        | "List"
                        | "Array"
                        | "Option"
                        | "Result"
                        | "arg"
                        | "arg_count"
                        | "range"
                )
            {
                self.report(
                    "CGT017",
                    format!("duplicate or reserved nominal type `{name}`"),
                    *span,
                );
            }
            let mut edges = Vec::new();
            for payload in payloads {
                if let Some(ty) =
                    resolve_in_module(payload, &self.module, &self.imported, &mut self.diagnostics)
                {
                    if *public && crate::nominal::private_type(&ty, &self.module) {
                        self.report(
                            "CGT022",
                            "public aggregate exposes a private nominal type",
                            payload.span,
                        );
                    }
                    if matches!(ty, Type::List(_)) {
                        self.report(
                            "CGT032",
                            "collection aggregate payload semantics are deferred to S2b",
                            payload.span,
                        );
                    }
                    dependencies(&ty, &mut edges);
                }
            }
            graph.insert(record_identity(&self.module, name, &self.imported), edges);
        }
        fn cycle(
            node: &str,
            graph: &BTreeMap<String, Vec<String>>,
            active: &mut BTreeSet<String>,
            done: &mut BTreeSet<String>,
        ) -> bool {
            if active.contains(node) {
                return true;
            }
            if done.contains(node) {
                return false;
            }
            // Source/module limits are not a recursion guarantee. Fail closed.
            if active.len() >= 64 {
                return true;
            }
            active.insert(node.to_owned());
            if graph
                .get(node)
                .is_some_and(|edges| edges.iter().any(|n| cycle(n, graph, active, done)))
            {
                return true;
            }
            active.remove(node);
            done.insert(node.to_owned());
            false
        }
        let mut done = BTreeSet::new();
        for (name, _, span, _) in &declarations {
            if cycle(
                &record_identity(&self.module, name, &self.imported),
                &graph,
                &mut BTreeSet::new(),
                &mut done,
            ) {
                self.report(
                    "CGT030",
                    "recursive or excessively deep aggregate value types are unsupported",
                    *span,
                );
            }
        }
    }

    fn check_constructor(
        &mut self,
        syntax: &cgx2_syntax::TypeSyntax,
        case: &str,
        arguments: &[CallArg],
        span: Span,
    ) -> Option<Type> {
        let ty = resolve_in_module(syntax, &self.module, &self.imported, &mut self.diagnostics)?;
        let Some(cases) =
            crate::nominal::cases(&ty, &self.module, &self.imported, &mut self.diagnostics)
        else {
            self.report("CGT033", "constructor requires a variant type", syntax.span);
            return None;
        };
        let Some((_, payloads)) = cases.iter().find(|(name, _)| name == case) else {
            for arg in arguments {
                self.check_expr(&arg.value);
            }
            self.report(
                "CGT033",
                format!("variant `{ty}` has no case `{case}`"),
                span,
            );
            return None;
        };
        if arguments.len() != payloads.len() || arguments.iter().any(|arg| arg.name.is_some()) {
            self.report(
                "CGT034",
                format!(
                    "case `{case}` requires {} positional payloads",
                    payloads.len()
                ),
                span,
            );
        }
        for (index, arg) in arguments.iter().enumerate() {
            if let Some(actual) = self.check_expr(&arg.value) {
                if let Some(expected) = payloads.get(index) {
                    self.expect_type(&actual, expected, arg.value.span);
                }
            }
        }
        Some(ty)
    }

    fn check_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[cgx2_syntax::MatchArm],
        expected: Option<&Type>,
        span: Span,
    ) -> Option<(Type, Span)> {
        let ty = self.check_expr(scrutinee)?;
        let Some(cases) =
            crate::nominal::cases(&ty, &self.module, &self.imported, &mut self.diagnostics)
        else {
            self.report(
                "CGT035",
                "match requires a nominal variant scrutinee",
                scrutinee.span,
            );
            return None;
        };
        let mut covered = BTreeSet::new();
        let mut wildcard = false;
        let mut result: Option<Type> = None;
        for arm in arms {
            if wildcard || covered.len() == cases.len() {
                self.report("CGT037", "unreachable match arm", arm.span);
            }
            self.scopes.push(BTreeMap::new());
            match &arm.pattern {
                cgx2_syntax::Pattern::Wildcard(_) => wildcard = true,
                cgx2_syntax::Pattern::Case {
                    ty: syntax,
                    case,
                    bindings,
                    span: pattern_span,
                } => {
                    if let Some(pattern_ty) = resolve_in_module(
                        syntax,
                        &self.module,
                        &self.imported,
                        &mut self.diagnostics,
                    ) {
                        self.expect_type(&pattern_ty, &ty, syntax.span);
                    }
                    if !covered.insert(case.clone()) {
                        self.report(
                            "CGT037",
                            format!("duplicate match case `{case}`"),
                            *pattern_span,
                        );
                    }
                    if let Some((_, payloads)) = cases.iter().find(|(name, _)| name == case) {
                        if bindings.len() != payloads.len() {
                            self.report(
                                "CGT034",
                                format!("pattern `{case}` requires {} bindings", payloads.len()),
                                *pattern_span,
                            );
                        }
                        for (binding, payload) in bindings.iter().zip(payloads) {
                            if binding.name != "_" {
                                self.define(&binding.name, payload.clone(), false, binding.span);
                            }
                        }
                    } else {
                        self.report(
                            "CGT033",
                            format!("unknown case `{case}` for `{ty}`"),
                            *pattern_span,
                        );
                    }
                }
            }
            // `expected` constrains explicit returns anywhere in the arm. Only
            // fallthrough values participate in the match's own value join;
            // the enclosing block checks its final value against the function.
            let tail = self.check_block(&arm.body, expected);
            if !block_returns(&arm.body) {
                let (branch, branch_span) = tail.unwrap_or((Type::Unit, arm.span));
                if let Some(result) = &result {
                    self.expect_type(&branch, result, branch_span);
                } else {
                    result = Some(branch);
                }
            }
            self.scopes.pop();
        }
        if !wildcard {
            let missing: Vec<_> = cases
                .iter()
                .filter(|(name, _)| !covered.contains(name))
                .map(|(name, _)| name.as_str())
                .collect();
            if !missing.is_empty() {
                self.report(
                    "CGT036",
                    format!("nonexhaustive match; missing {}", missing.join(", ")),
                    span,
                );
            }
        }
        result.map(|ty| (ty, span))
    }

    fn report(&mut self, code: &'static str, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(TypeDiagnostic::new(code, message.into(), span));
    }

    fn insert_builtin(&mut self, name: &str, parameters: Vec<Type>, result: Type) {
        self.functions.insert(
            name.into(),
            DraftSignature {
                public: false,
                parameters: parameters.into_iter().map(Some).collect(),
                result: Some(result),
                span: Span { start: 0, end: 0 },
            },
        );
    }

    fn declare_function(&mut self, function: &Function) {
        let name_span = Span {
            start: function.span.start + if function.public { 7 } else { 3 },
            end: function.span.start + if function.public { 7 } else { 3 } + function.name.len(),
        };
        if self.functions.contains_key(&function.name) {
            self.report(
                "CGT001",
                format!("duplicate function `{}`", function.name),
                name_span,
            );
            return;
        }
        let parameters: Vec<Option<Type>> = function
            .parameters
            .iter()
            .map(|p| resolve_in_module(&p.ty, &self.module, &self.imported, &mut self.diagnostics))
            .collect();
        let result = function.return_type.as_ref().and_then(|ty| {
            resolve_in_module(ty, &self.module, &self.imported, &mut self.diagnostics)
        });
        if function.public && function.return_type.is_none() {
            self.report(
                "CGT006",
                "public function requires a return annotation",
                function.span,
            );
        }
        if function.public
            && (parameters
                .iter()
                .flatten()
                .any(|ty| crate::nominal::private_type(ty, &self.module))
                || result
                    .as_ref()
                    .is_some_and(|ty| crate::nominal::private_type(ty, &self.module)))
        {
            self.report(
                "CGT022",
                "public signature exposes a private nominal type",
                function.span,
            );
        }
        self.functions.insert(
            function.name.clone(),
            DraftSignature {
                public: function.public,
                parameters,
                result,
                span: function.span,
            },
        );
    }

    fn check_function(&mut self, function: &Function) {
        if function.foreign.is_some() {
            return;
        }
        let Some(signature) = self.functions.get(&function.name).cloned() else {
            return;
        };
        if signature.span != function.span {
            return;
        }
        self.scopes.push(BTreeMap::new());
        for (parameter, ty) in function.parameters.iter().zip(&signature.parameters) {
            if let Some(ty) = ty {
                let name_span = Span {
                    start: parameter.span.start,
                    end: parameter.span.start + parameter.name.len(),
                };
                self.define(&parameter.name, ty.clone(), false, name_span);
            }
        }
        if !function.requires.is_empty() || !function.ensures.is_empty() {
            let scalar = |ty: &Type| matches!(ty, Type::I64 | Type::Bool);
            if signature.parameters.iter().flatten().any(|ty| !scalar(ty))
                || signature.result.as_ref().is_none_or(|ty| !scalar(ty))
            {
                self.report(
                    "CGT014",
                    "contracts require annotated i64/Bool parameters and return",
                    function.span,
                );
            }
            if function.parameters.iter().any(|p| p.name == "result") {
                self.report(
                    "CGT014",
                    "`result` is reserved in contracted functions",
                    function.span,
                );
            }
            for expr in &function.requires {
                self.check_contract(expr);
            }
            if let Some(result) = &signature.result {
                self.scopes.last_mut().expect("scope").insert(
                    "result".into(),
                    Binding {
                        ty: result.clone(),
                        mutable: false,
                    },
                );
            }
            for expr in &function.ensures {
                self.check_contract(expr);
            }
            self.scopes.last_mut().expect("scope").remove("result");
        }
        let tail = self.check_block(&function.body, signature.result.as_ref());
        self.scopes.pop();
        if let Some(expected) = &signature.result {
            if !block_returns(&function.body) {
                if let Some((actual, span)) = tail {
                    self.expect_type(&actual, expected, span);
                }
            }
        } else {
            if let Some(inferred) = self.check_inferred_result(&function.body, tail) {
                if let Some(entry) = self.functions.get_mut(&function.name) {
                    entry.result = Some(inferred);
                }
            }
        }
    }

    fn check_inferred_result(&mut self, body: &[Stmt], tail: Option<(Type, Span)>) -> Option<Type> {
        fn returns(body: &[Stmt], types: &SpanTypes, out: &mut Vec<(Type, Span)>) {
            for stmt in body {
                match &stmt.kind {
                    StmtKind::Return(Some(expr)) => {
                        if let Some(ty) = types.get(expr.span) {
                            out.push((ty.clone(), expr.span));
                        }
                    }
                    StmtKind::Return(None) => out.push((Type::Unit, stmt.span)),
                    StmtKind::If {
                        then_body,
                        else_body,
                        ..
                    } => {
                        returns(then_body, types, out);
                        returns(else_body, types, out);
                    }
                    StmtKind::Match { arms, .. } => {
                        for arm in arms {
                            returns(&arm.body, types, out);
                        }
                    }
                    StmtKind::For { body, .. } => returns(body, types, out),
                    _ => {}
                }
            }
        }
        let mut returned = Vec::new();
        returns(body, &self.expressions, &mut returned);
        if !block_returns(body) {
            if let Some(tail) = tail {
                returned.push(tail);
            }
        }
        if let Some((inferred, _)) = returned.first().cloned() {
            for (actual, span) in returned {
                self.expect_type(&actual, &inferred, span);
            }
            return Some(inferred);
        }
        None
    }

    fn check_contract(&mut self, expr: &Expr) {
        if !effects::contract_calls_are_safe(expr, &self.effect_summaries, &self.module) {
            self.report(
                "CGT014",
                "contract call has effects, is unknown, or may recurse",
                expr.span,
            );
        }
        if let Some(ty) = self.check_expr(expr) {
            self.expect_type(&ty, &Type::Bool, expr.span);
        }
    }

    fn define(&mut self, name: &str, ty: Type, mutable: bool, span: Span) {
        let current = self.scopes.last_mut().expect("scope exists");
        if current.contains_key(name) {
            self.report("CGT001", format!("duplicate name `{name}`"), span);
        } else {
            current.insert(name.into(), Binding { ty, mutable });
        }
    }

    fn lookup(&self, path: &Path) -> Option<Binding> {
        if path.segments.len() != 1 {
            return None;
        }
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(&path.segments[0]).cloned())
    }

    fn check_block(
        &mut self,
        statements: &[Stmt],
        expected: Option<&Type>,
    ) -> Option<(Type, Span)> {
        let mut last = None;
        for statement in statements {
            last = self.check_stmt(statement, expected);
        }
        last
    }

    fn check_stmt(&mut self, statement: &Stmt, expected: Option<&Type>) -> Option<(Type, Span)> {
        match &statement.kind {
            StmtKind::Match { scrutinee, arms } => {
                self.check_match(scrutinee, arms, expected, statement.span)
            }
            StmtKind::LoopContract { expression, .. } => {
                self.report(
                    "CGT032",
                    "annotated loop semantics are deferred to S2b/S3-V",
                    expression.span,
                );
                None
            }
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                if let Some(ty) = self.check_expr(condition) {
                    self.expect_type(&ty, &Type::Bool, condition.span);
                }
                for body in [then_body, else_body] {
                    self.scopes.push(BTreeMap::new());
                    self.check_block(body, expected);
                    self.scopes.pop();
                }
                Some((Type::Unit, statement.span))
            }
            StmtKind::Let {
                mutable,
                name,
                annotation,
                value,
            } => {
                let value_ty = self.check_expr(value);
                let annotated = annotation.as_ref().and_then(|ty| {
                    resolve_in_module(ty, &self.module, &self.imported, &mut self.diagnostics)
                });
                if let (Some(actual), Some(expected)) = (&value_ty, &annotated) {
                    self.expect_type(actual, expected, value.span);
                }
                if let Some(ty) = annotated.or(value_ty) {
                    let start = statement.span.start + if *mutable { 8 } else { 4 };
                    self.define(
                        name,
                        ty,
                        *mutable,
                        Span {
                            start,
                            end: start + name.len(),
                        },
                    );
                }
                Some((Type::Unit, statement.span))
            }
            StmtKind::Assign { target, op, value } => {
                let value_ty = self.check_expr(value);
                if let [base, field] = target.segments.as_slice() {
                    if let Some(binding) = self.local(base) {
                        if matches!(binding.ty, Type::Record(_)) {
                            self.report(
                                "CGT004",
                                format!("cannot assign to immutable record field `{base}.{field}`"),
                                target.span,
                            );
                            return Some((Type::Unit, statement.span));
                        }
                    }
                }
                if let Some(binding) = self.lookup(target) {
                    if !binding.mutable {
                        self.report(
                            "CGT004",
                            format!("cannot assign to immutable `{}`", target.segments.join(".")),
                            target.span,
                        );
                    }
                    if let Some(actual) = &value_ty {
                        if *op == AssignOp::Add {
                            if binding.ty != Type::I64 || *actual != Type::I64 {
                                self.report("CGT003", "`+=` requires i64 operands", statement.span);
                            }
                        } else {
                            self.expect_type(actual, &binding.ty, value.span);
                        }
                    }
                } else {
                    self.report(
                        "CGT001",
                        format!("unknown name `{}`", target.segments.join(".")),
                        target.span,
                    );
                }
                Some((Type::Unit, statement.span))
            }
            StmtKind::Return(value) => {
                let actual = match value {
                    Some(value) => self.check_expr(value).map(|ty| (ty, value.span)),
                    None => Some((Type::Unit, statement.span)),
                };
                if let (Some((actual, span)), Some(expected)) = (&actual, expected) {
                    self.expect_type(actual, expected, *span);
                }
                actual
            }
            StmtKind::Expr(expr) => self.check_expr(expr).map(|ty| (ty, expr.span)),
            StmtKind::For {
                binding,
                iter,
                body,
            } => {
                let element = match &iter.kind {
                    ExprKind::Call { callee, arguments } if matches!(&callee.kind, ExprKind::Name(path) if path.segments.as_slice() == ["range"]) => {
                        if arguments.len() != 2 || arguments.iter().any(|arg| arg.name.is_some()) {
                            self.report(
                                "CGT005",
                                "range requires exactly two positional i64 arguments",
                                iter.span,
                            );
                            None
                        } else {
                            for arg in arguments {
                                if !effects::contract_calls_are_safe(
                                    &arg.value,
                                    &self.effect_summaries,
                                    &self.module,
                                ) {
                                    self.report(
                                        "CGT012",
                                        "range bounds require pure local calls",
                                        arg.value.span,
                                    );
                                }
                                if let Some(ty) = self.check_expr(&arg.value) {
                                    self.expect_type(&ty, &Type::I64, arg.value.span);
                                }
                            }
                            Some(Type::I64)
                        }
                    }
                    _ => match self.check_expr(iter) {
                        Some(Type::List(inner)) => Some(*inner),
                        Some(_) => {
                            self.report(
                                "CGT003",
                                "for iterator must be a List<T> or range(start, end)",
                                iter.span,
                            );
                            None
                        }
                        None => None,
                    },
                };
                self.scopes.push(BTreeMap::new());
                if let Some(element) = element {
                    let start = statement.span.start + 4;
                    self.define(
                        binding,
                        element,
                        false,
                        Span {
                            start,
                            end: start + binding.len(),
                        },
                    );
                }
                if matches!(&iter.kind, ExprKind::Call { callee, .. }
                    if matches!(&callee.kind, ExprKind::Name(path) if path.segments.as_slice() == ["range"]))
                {
                    self.check_range_body_policy(body);
                }
                self.check_block(body, expected);
                self.scopes.pop();
                Some((Type::Unit, statement.span))
            }
            StmtKind::NoGc { .. } => {
                self.report(
                    "CGM001",
                    "no_gc requires the memory source checker",
                    statement.span,
                );
                None
            }
        }
    }

    fn check_range_body_policy(&mut self, body: &[Stmt]) {
        for stmt in body {
            let expression = match &stmt.kind {
                StmtKind::Match { scrutinee, arms } => {
                    for arm in arms {
                        self.check_range_body_policy(&arm.body);
                    }
                    Some(scrutinee)
                }
                StmtKind::LoopContract { expression, .. } => {
                    self.report("CGT032", "loop contracts are deferred to S3-V", stmt.span);
                    Some(expression)
                }
                StmtKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.check_range_body_policy(then_body);
                    self.check_range_body_policy(else_body);
                    Some(condition)
                }
                StmtKind::Let { value, .. }
                | StmtKind::Assign { value, .. }
                | StmtKind::Expr(value) => Some(value),
                StmtKind::Return(_) => {
                    self.report("CGT003", "return in a range loop is unsupported", stmt.span);
                    None
                }
                StmtKind::For { body, .. } => {
                    self.check_range_body_policy(body);
                    None
                }
                StmtKind::NoGc { .. } => {
                    self.report("CGT003", "no_gc in a range loop is unsupported", stmt.span);
                    None
                }
            };
            if let Some(expr) = expression {
                if !effects::contract_calls_are_safe(expr, &self.effect_summaries, &self.module) {
                    self.report(
                        "CGT012",
                        "range loop body calls must be pure local",
                        expr.span,
                    );
                }
            }
        }
    }

    fn expect_type(&mut self, actual: &Type, expected: &Type, span: Span) {
        if actual != expected {
            self.report(
                "CGT003",
                format!("expected {expected}, found {actual}"),
                span,
            );
        }
    }

    fn local(&self, name: &str) -> Option<Binding> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    fn check_record_constructor(
        &mut self,
        record: &Record,
        arguments: &[CallArg],
        span: Span,
    ) -> Type {
        let mut supplied = BTreeSet::new();
        for arg in arguments {
            let actual = self.check_expr(&arg.value);
            let Some(name) = &arg.name else {
                self.report(
                    "CGT019",
                    "record constructor requires named fields",
                    arg.span,
                );
                continue;
            };
            if !supplied.insert(name.clone()) {
                self.report(
                    "CGT019",
                    format!("duplicate record field `{name}`"),
                    arg.span,
                );
            }
            if let Some(field) = record.fields.iter().find(|field| field.name == *name) {
                let expected = crate::nominal::resolve_member(
                    &field.ty,
                    &self.module,
                    &self.imported,
                    &mut self.diagnostics,
                );
                if let (Some(actual), Some(expected)) = (actual, expected) {
                    self.expect_type(&actual, &expected, arg.value.span);
                }
            } else {
                self.report(
                    "CGT019",
                    format!("unknown field `{name}` for record `{}`", record.name),
                    arg.span,
                );
            }
        }
        for field in &record.fields {
            if !supplied.contains(&field.name) {
                self.report(
                    "CGT019",
                    format!(
                        "missing field `{}` for record `{}`",
                        field.name, record.name
                    ),
                    span,
                );
            }
        }
        Type::Record(if record.name.contains('.') {
            record.name.clone()
        } else {
            record_identity(&self.module, &record.name, &self.imported)
        })
    }

    fn check_expr(&mut self, expr: &Expr) -> Option<Type> {
        let ty = match &expr.kind {
            ExprKind::Constructor {
                ty,
                case,
                arguments,
            } => self.check_constructor(ty, case, arguments, expr.span),
            ExprKind::Collection(values) => {
                for value in values {
                    self.check_expr(value);
                }
                self.report(
                    "CGT032",
                    "collection semantics are deferred to S2b",
                    expr.span,
                );
                None
            }
            ExprKind::Index { collection, index } => {
                self.check_expr(collection);
                self.check_expr(index);
                self.report(
                    "CGT032",
                    "checked indexing semantics are deferred to S2b",
                    expr.span,
                );
                None
            }
            ExprKind::Name(path) => match path.segments.as_slice() {
                [name] => match self.local(name) {
                    Some(binding) => Some(binding.ty),
                    None => {
                        self.report("CGT001", format!("unknown name `{name}`"), path.span);
                        None
                    }
                },
                [base, field] => match self.local(base) {
                    Some(binding) => {
                        if self.imports.contains(base) {
                            self.report(
                                "CGT020",
                                format!(
                                    "ambiguous `{base}.{field}`: local value and imported module"
                                ),
                                path.span,
                            );
                            None
                        } else if let Type::Record(record_name) = binding.ty {
                            let found = self
                                .module
                                .records
                                .iter()
                                .find(|record| {
                                    record_identity(&self.module, &record.name, &self.imported)
                                        == record_name
                                })
                                .or_else(|| self.imported.public_records.get(&record_name))
                                .and_then(|record| {
                                    record.fields.iter().find(|item| item.name == *field)
                                });
                            match found {
                                Some(item) => crate::nominal::resolve_member(
                                    &item.ty,
                                    &self.module,
                                    &self.imported,
                                    &mut self.diagnostics,
                                ),
                                None => {
                                    self.report(
                                        "CGT021",
                                        format!("record `{record_name}` has no field `{field}`"),
                                        path.span,
                                    );
                                    None
                                }
                            }
                        } else {
                            self.report("CGT021", format!("`{base}` is not a record"), path.span);
                            None
                        }
                    }
                    None => {
                        self.report(
                            "CGT001",
                            format!("unknown name `{}`", path.segments.join(".")),
                            path.span,
                        );
                        None
                    }
                },
                _ => {
                    self.report(
                        "CGT021",
                        "field access requires one record field segment",
                        path.span,
                    );
                    None
                }
            },
            ExprKind::Integer(value) => {
                if value.replace('_', "").parse::<i64>().is_ok() {
                    Some(Type::I64)
                } else {
                    self.report(
                        "CGT007",
                        "integer literal is outside signed i64 range",
                        expr.span,
                    );
                    None
                }
            }
            ExprKind::StringLiteral(_) => Some(Type::String),
            ExprKind::Boolean(_) => Some(Type::Bool),
            ExprKind::Binary { left, right, .. } => {
                let left_ty = self.check_expr(left);
                let right_ty = self.check_expr(right);
                match (left_ty, right_ty) {
                    (Some(Type::I64), Some(Type::I64)) => Some(Type::I64),
                    (Some(_), Some(_)) => {
                        self.report("CGT003", "arithmetic requires i64 operands", expr.span);
                        None
                    }
                    _ => None,
                }
            }
            ExprKind::Call { callee, arguments } => {
                if matches!(&callee.kind, ExprKind::Name(path) if path.segments.as_slice() == ["range"])
                {
                    self.report("CGT003", "range is only valid in a for iterator", expr.span);
                    return None;
                }
                if let ExprKind::Name(path) = &callee.kind {
                    let name = path.segments.join(".");
                    if path.segments.len() >= 2 {
                        let type_name = path.segments[..path.segments.len() - 1].join(".");
                        if matches!(type_name.as_str(), "Option" | "Result")
                            || self.module.variants.iter().any(|v| v.name == type_name)
                            || self.imported.public_variants.contains_key(&type_name)
                            || self.imported.private_variants.contains(&type_name)
                        {
                            let syntax = cgx2_syntax::TypeSyntax {
                                path: Path {
                                    segments: path.segments[..path.segments.len() - 1].to_vec(),
                                    span: path.span,
                                },
                                arguments: vec![],
                                span: path.span,
                            };
                            let ty = self.check_constructor(
                                &syntax,
                                path.segments.last().expect("case"),
                                arguments,
                                expr.span,
                            );
                            if let Some(ty) = &ty {
                                self.expressions.insert(expr.span, ty.clone());
                            }
                            return ty;
                        }
                    }
                    let record = if path.segments.len() == 1 {
                        self.module
                            .records
                            .iter()
                            .find(|record| record.name == name)
                            .cloned()
                    } else if self
                        .imports
                        .contains(&path.segments[..path.segments.len() - 1].join("."))
                    {
                        self.imported.public_records.get(&name).cloned()
                    } else {
                        None
                    };
                    if let Some(record) = record {
                        if path.segments.len() > 1 && self.local(&path.segments[0]).is_some() {
                            self.report(
                                "CGT020",
                                "ambiguous local value and imported module",
                                path.span,
                            );
                            return None;
                        }
                        let ty = self.check_record_constructor(&record, arguments, expr.span);
                        self.expressions.insert(expr.span, ty.clone());
                        return Some(ty);
                    }
                    if self.imported.private_records.contains(&name) {
                        self.report("CGT022", format!("record `{name}` is private"), path.span);
                        return None;
                    }
                    if let [name] = path.segments.as_slice() {
                        if !self.functions.contains_key(name)
                            && arguments.iter().any(|arg| arg.name.is_some())
                        {
                            for arg in arguments {
                                self.check_expr(&arg.value);
                            }
                            self.report(
                                "CGT002",
                                format!("unknown record type `{name}`"),
                                path.span,
                            );
                            return None;
                        }
                    }
                }
                let mut arg_types = Vec::new();
                for arg in arguments {
                    if arg.name.is_some() {
                        self.report(
                            "CGT005",
                            "named arguments are unsupported for scalar calls",
                            arg.span,
                        );
                    }
                    arg_types.push(self.check_expr(&arg.value));
                }
                let ExprKind::Name(path) = &callee.kind else {
                    self.check_expr(callee);
                    self.report("CGT003", "callee must be a function name", expr.span);
                    return None;
                };
                if let Some(base) = path
                    .segments
                    .first()
                    .filter(|base| path.segments.len() > 1 && self.local(base).is_some())
                {
                    let message = if self.imports.contains(base) {
                        format!(
                            "ambiguous `{}`: local value and imported module",
                            path.segments.join(".")
                        )
                    } else {
                        format!("record field `{}` is not callable", path.segments.join("."))
                    };
                    self.report("CGT020", message, path.span);
                    return None;
                }
                if let [name] = path.segments.as_slice() {
                    if matches!(
                        name.as_str(),
                        "$cgx2_cmp_eq"
                            | "$cgx2_cmp_ne"
                            | "$cgx2_cmp_lt"
                            | "$cgx2_cmp_le"
                            | "$cgx2_cmp_gt"
                            | "$cgx2_cmp_ge"
                    ) {
                        if arguments.len() != 2 {
                            self.report("CGT005", "comparison requires two operands", expr.span);
                            return None;
                        }
                        let valid =
                            matches!(arg_types.as_slice(), [Some(Type::I64), Some(Type::I64)])
                                || (matches!(name.as_str(), "$cgx2_cmp_eq" | "$cgx2_cmp_ne")
                                    && matches!(
                                        arg_types.as_slice(),
                                        [Some(Type::Bool), Some(Type::Bool)]
                                    ));
                        if !valid {
                            self.report(
                                "CGT003",
                                "comparison requires compatible scalar operands",
                                expr.span,
                            );
                            return None;
                        }
                        self.expressions.insert(expr.span, Type::Bool);
                        return Some(Type::Bool);
                    }
                }
                let signature = if path.segments.len() == 1 {
                    self.functions.get(&path.segments[0]).cloned()
                } else {
                    self.imported_signature(path)
                };
                if let Some(signature) = signature {
                    if arguments.len() != signature.parameters.len() {
                        self.report(
                            "CGT005",
                            format!(
                                "expected {} arguments, found {}",
                                signature.parameters.len(),
                                arguments.len()
                            ),
                            expr.span,
                        );
                    }
                    for (actual, expected) in arg_types.iter().zip(signature.parameters.iter()) {
                        if let (Some(actual), Some(expected)) = (actual, expected) {
                            if actual != expected {
                                self.report(
                                    "CGT003",
                                    format!("expected {expected}, found {actual}"),
                                    expr.span,
                                );
                            }
                        }
                    }
                    if signature.result.is_none() {
                        self.report(
                            "CGT003",
                            "callee return type is not yet inferred; add a return annotation",
                            expr.span,
                        );
                    }
                    signature.result
                } else {
                    if path.segments.len() == 1 {
                        self.report(
                            "CGT001",
                            format!("unknown function `{}`", path.segments[0]),
                            path.span,
                        );
                    }
                    None
                }
            }
        };
        if let Some(ty) = &ty {
            self.expressions.insert(expr.span, ty.clone());
        }
        ty
    }
}

impl Checker {
    fn imported_signature(&mut self, path: &Path) -> Option<DraftSignature> {
        let module = path.segments[..path.segments.len() - 1].join(".");
        let name = path.segments.join(".");
        if module == "memory" {
            self.report(
                "CGM001",
                "memory intrinsics cannot be called through an import",
                path.span,
            );
        } else if !self.imports.contains(&module) {
            self.report(
                "CGT008",
                format!("module `{module}` is not imported"),
                path.span,
            );
        } else if !self.imported.modules.contains(&module) {
            self.report(
                "CGT001",
                format!("unknown imported module `{module}`"),
                path.span,
            );
        } else if self.imported.private_functions.contains(&name) {
            self.report("CGT009", format!("function `{name}` is private"), path.span);
        } else if let Some(signature) = self.imported.public_functions.get(&name) {
            for ty in signature
                .parameters
                .iter()
                .chain(std::iter::once(&signature.return_type))
            {
                if let Type::Record(identity) = ty {
                    if let Some((defining, _)) = identity.rsplit_once('.') {
                        let own = self.module.name.as_ref().map(|p| p.segments.join("."));
                        if own.as_deref() != Some(defining) && !self.imports.contains(defining) {
                            self.report(
                                "CGT008",
                                format!(
                                    "record `{identity}` requires a direct import of `{defining}`"
                                ),
                                path.span,
                            );
                            return None;
                        }
                    }
                }
            }
            if signature
                .parameters
                .iter()
                .any(|ty| matches!(ty, Type::List(_)))
                || matches!(signature.return_type, Type::List(_))
            {
                self.report(
                    "CGT010",
                    format!("imported function `{name}` has a non-scalar signature"),
                    path.span,
                );
                return None;
            }
            return Some(DraftSignature {
                public: true,
                parameters: signature.parameters.iter().cloned().map(Some).collect(),
                result: Some(signature.return_type.clone()),
                span: signature.span,
            });
        } else {
            self.report(
                "CGT001",
                format!("unknown public function `{name}`"),
                path.span,
            );
        }
        None
    }
}
