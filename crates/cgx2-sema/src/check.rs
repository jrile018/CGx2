use std::collections::{BTreeMap, BTreeSet};

use cgx2_syntax::{AssignOp, Expr, ExprKind, Function, Module, Path, Span, Stmt, StmtKind};

use crate::types::resolve;
use crate::{effects, EffectSummary, Type, TypeDiagnostic};

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
        if !module.imports.is_empty() {
            return Err(vec![TypeDiagnostic::new(
                "CGM001",
                "imports in memory scripts are unsupported".into(),
                module.imports[0].span,
            )]);
        }
        return result.map_err(|mut diagnostics| {
            let (_, effect_diagnostics) = effects::analyze(module);
            diagnostics.extend(effect_diagnostics);
            diagnostics
        });
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
    let (effect_summaries, effect_diagnostics) = effects::analyze(module);
    checker.effect_summaries = effect_summaries;
    checker.diagnostics.extend(effect_diagnostics);
    checker.insert_builtin("arg_count", vec![], Type::I64);
    checker.insert_builtin("arg", vec![Type::I64], Type::String);
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
            .map(|p| resolve(&p.ty, &mut errors))
            .collect();
        let result = match &function.return_type {
            Some(ty) => resolve(ty, &mut errors),
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
        let parameters = function
            .parameters
            .iter()
            .map(|p| resolve(&p.ty, &mut self.diagnostics))
            .collect();
        let result = function
            .return_type
            .as_ref()
            .and_then(|ty| resolve(ty, &mut self.diagnostics));
        if function.public && function.return_type.is_none() {
            self.report(
                "CGT006",
                "public function requires a return annotation",
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
        if let Some((actual, span)) = tail {
            if let Some(expected) = &signature.result {
                if matches!(
                    function.body.last().map(|stmt| &stmt.kind),
                    Some(StmtKind::Return(_))
                ) {
                    return;
                }
                self.expect_type(&actual, expected, span);
            } else if function.return_type.is_none() {
                if let Some(entry) = self.functions.get_mut(&function.name) {
                    entry.result = Some(actual);
                }
            }
        }
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
            StmtKind::Let {
                mutable,
                name,
                annotation,
                value,
            } => {
                let value_ty = self.check_expr(value);
                let annotated = annotation
                    .as_ref()
                    .and_then(|ty| resolve(ty, &mut self.diagnostics));
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
                let element = match self.check_expr(iter) {
                    Some(Type::List(inner)) => Some(*inner),
                    Some(_) => {
                        self.report("CGT003", "for iterator must be a List<T>", iter.span);
                        None
                    }
                    None => None,
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

    fn expect_type(&mut self, actual: &Type, expected: &Type, span: Span) {
        if actual != expected {
            self.report(
                "CGT003",
                format!("expected {expected}, found {actual}"),
                span,
            );
        }
    }

    fn check_expr(&mut self, expr: &Expr) -> Option<Type> {
        let ty = match &expr.kind {
            ExprKind::Name(path) => match self.lookup(path) {
                Some(binding) => Some(binding.ty),
                None => {
                    self.report(
                        "CGT001",
                        format!("unknown name `{}`", path.segments.join(".")),
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
