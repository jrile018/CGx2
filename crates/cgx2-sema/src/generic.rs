// Included in check.rs: generic inference and finite data expression checking.
impl Checker {
    fn context_known(&self, t: &Type) -> bool {
        match t {
            Type::Parameter { .. } => self.binders.values().any(|p| p == t),
            Type::List(t) | Type::Array(t) => self.context_known(t),
            Type::Variant { arguments, .. } | Type::AppliedRecord { arguments, .. } => {
                arguments.iter().all(|t| self.context_known(t))
            }
            _ => true,
        }
    }
    fn check_expr_as(&mut self, expr: &Expr, expected: Option<&Type>) -> Option<Type> {
        let prior = self
            .expected
            .replace(expected.cloned().unwrap_or(Type::Unit));
        if expected.is_none() {
            self.expected = None;
        }
        let result = self.check_expr(expr);
        self.expected = prior;
        result
    }
    fn pattern_type(
        &self,
        syntax: &cgx2_syntax::TypeSyntax,
        actual: &Type,
    ) -> cgx2_syntax::TypeSyntax {
        let mut result = syntax.clone();
        if syntax.arguments.is_empty() {
            if let Type::Variant { name, arguments } = actual {
                let given = syntax.path.segments.join(".");
                if given == *name
                    || crate::nominal::identity(&self.module, &given) == *name
                    || format!("cgx2.core.{given}") == *name
                {
                    result.arguments = arguments
                        .iter()
                        .map(|t| crate::foundation::syntax_type(t, syntax.span))
                        .collect();
                }
            }
        }
        result
    }
    fn unify(
        &mut self,
        pattern: &Type,
        actual: &Type,
        map: &mut BTreeMap<String, Type>,
        span: Span,
    ) {
        match (pattern, actual) {
            (Type::Parameter { name, .. }, _) => {
                if let Some(prior) = map.get(name) {
                    self.expect_type(actual, &prior.clone(), span);
                } else {
                    map.insert(name.clone(), actual.clone());
                }
            }
            (Type::List(a), Type::List(b)) | (Type::Array(a), Type::Array(b)) => {
                self.unify(a, b, map, span)
            }
            (
                Type::Variant {
                    name: a,
                    arguments: aa,
                },
                Type::Variant {
                    name: b,
                    arguments: bb,
                },
            )
            | (
                Type::AppliedRecord {
                    name: a,
                    arguments: aa,
                },
                Type::AppliedRecord {
                    name: b,
                    arguments: bb,
                },
            ) if a == b && aa.len() == bb.len() => {
                for (a, b) in aa.iter().zip(bb) {
                    self.unify(a, b, map, span);
                }
            }
            _ => self.expect_type(actual, pattern, span),
        }
    }
    fn infer_call(
        &mut self,
        name: &str,
        signature: &DraftSignature,
        explicit: &[cgx2_syntax::TypeSyntax],
        args: &[CallArg],
        hint: Option<&Type>,
        span: Span,
    ) -> Option<Type> {
        let mut substitutions = BTreeMap::new();
        if !explicit.is_empty() && explicit.len() != signature.type_parameters.len() {
            self.report(
                "CGT031",
                format!(
                    "`{name}` requires {} type arguments",
                    signature.type_parameters.len()
                ),
                span,
            );
        }
        for (p, t) in signature.type_parameters.iter().zip(explicit) {
            if let Type::Parameter { name, .. } = p {
                if let Some(t) = crate::nominal::scoped(
                    t,
                    &self.module,
                    &self.imported,
                    &self.binders,
                    &mut self.diagnostics,
                ) {
                    substitutions.insert(name.clone(), t);
                }
            }
        }
        if let (Some(result), Some(expected)) = (&signature.result, hint) {
            self.unify(result, expected, &mut substitutions, span);
        }
        if args.len() != signature.parameters.len() {
            self.report(
                "CGT005",
                format!(
                    "expected {} arguments, found {}",
                    signature.parameters.len(),
                    args.len()
                ),
                span,
            );
        }
        let mut indices = (0..args.len()).collect::<Vec<_>>();
        indices.sort_by_key(|i| needs_context(&args[*i].value));
        for i in indices {
            let arg = &args[i];
            if arg.name.is_some() {
                self.report("CGT005", "function requires positional arguments", arg.span);
            }
            let template = signature.parameters.get(i).and_then(Option::as_ref);
            let expected = template
                .map(|t| crate::nominal::substitute(t, &substitutions))
                .filter(|t| self.context_known(t));
            if let Some(actual) = self.check_expr_as(&arg.value, expected.as_ref()) {
                if let Some(p) = template {
                    self.unify(p, &actual, &mut substitutions, arg.value.span);
                }
            }
        }
        let mut inferred = Vec::new();
        for p in &signature.type_parameters {
            let Type::Parameter {
                name: parameter,
                constraints,
            } = p
            else {
                continue;
            };
            let Some(actual) = substitutions.get(parameter) else {
                self.report(
                    "CGT040",
                    format!("ambiguous type parameter `{p}`; supply explicit type arguments"),
                    span,
                );
                return None;
            };
            // A parameter cannot be inferred from an empty literal's own placeholder.
            if actual == p && !self.binders.values().any(|b| b == p) {
                self.report("CGT040", "ambiguous generic application", span);
                return None;
            }
            for c in constraints {
                if !crate::nominal::satisfies(actual, c, &self.module, &self.imported, 0) {
                    self.report("CGT039", format!("`{actual}` does not satisfy `{c}`"), span);
                }
            }
            inferred.push(actual.clone());
        }
        if !inferred.is_empty() {
            self.calls.insert(
                (self.owner.clone(), span.start, span.end),
                ResolvedCall {
                    callee: name.into(),
                    arguments: inferred,
                },
            );
        }
        signature
            .result
            .as_ref()
            .map(|t| crate::nominal::substitute(t, &substitutions))
            .or_else(|| {
                self.report("CGT003", "callee result needs an annotation", span);
                None
            })
    }
    fn infer_aggregate(
        &mut self,
        syntax: &cgx2_syntax::TypeSyntax,
        case: Option<&str>,
        args: &[CallArg],
        hint: Option<&Type>,
        span: Span,
    ) -> Option<Type> {
        let n = syntax.path.segments.join(".");
        // Type binders shadow nominal names in annotations and constructor heads.
        // A parameter has no statically known fields or variant cases.
        if self.binders.contains_key(&n) {
            self.report(
                "CGT002",
                format!("type parameter `{n}` cannot be used as a constructor"),
                syntax.path.span,
            );
            return None;
        }
        let own = crate::nominal::identity(&self.module, &n);
        let record = self
            .module
            .records
            .iter()
            .find(|r| crate::nominal::identity(&self.module, &r.name) == own)
            .or_else(|| self.imported.public_records.get(&n))
            .cloned();
        let variant = self
            .module
            .variants
            .iter()
            .find(|v| crate::nominal::identity(&self.module, &v.name) == own)
            .or_else(|| self.imported.public_variants.get(&n))
            .cloned();
        let parameters = record
            .as_ref()
            .map(|r| r.type_parameters.clone())
            .or_else(|| variant.as_ref().map(|v| v.type_parameters.clone()))
            .unwrap_or_else(|| {
                let count = match n.as_str() {
                    "Option" | "cgx2.core.Option" => 1,
                    "Result" | "cgx2.core.Result" => 2,
                    _ => 0,
                };
                (0..count)
                    .map(|i| cgx2_syntax::TypeParameter {
                        name: format!("T{i}"),
                        constraints: vec![],
                        span,
                    })
                    .collect()
            });
        let scope = crate::nominal::binders(&self.module, &n, &parameters, &mut self.diagnostics);
        let params: Vec<_> = parameters.iter().map(|p| scope[&p.name].clone()).collect();
        let mut template_syntax = syntax.clone();
        template_syntax.arguments = params
            .iter()
            .map(|t| crate::foundation::syntax_type(t, span))
            .collect();
        let mut scoped = self.binders.clone();
        scoped.extend(scope.clone());
        // Resolve the nominal head separately from its declaration's binders:
        // in `record T<T>`, the argument T must not replace the record head.
        let template = if let Some(record) = &record {
            let name = crate::nominal::identity(&self.module, &record.name);
            if params.is_empty() {
                Type::Record(name)
            } else {
                Type::AppliedRecord {
                    name,
                    arguments: params.clone(),
                }
            }
        } else if let Some(variant) = &variant {
            Type::Variant {
                name: crate::nominal::identity(&self.module, &variant.name),
                arguments: params.clone(),
            }
        } else {
            crate::nominal::scoped(
                &template_syntax,
                &self.module,
                &self.imported,
                &scoped,
                &mut self.diagnostics,
            )?
        };
        let diagnostic_count = self.diagnostics.len();
        let fields = if case.is_none() {
            crate::nominal::members(
                &template,
                &self.module,
                &self.imported,
                &mut self.diagnostics,
            )
        } else {
            let cases = crate::nominal::cases(
                &template,
                &self.module,
                &self.imported,
                &mut self.diagnostics,
            );
            let Some(cases) = cases else {
                if self.diagnostics.len() == diagnostic_count {
                    self.report(
                        "CGT002",
                        "constructor requires a nominal variant",
                        syntax.path.span,
                    );
                }
                return None;
            };
            let Some((_, payloads)) = cases.into_iter().find(|(c, _)| Some(c.as_str()) == case)
            else {
                self.report(
                    "CGT033",
                    format!("unknown case `{}`", case.unwrap_or("")),
                    span,
                );
                return None;
            };
            Some(
                payloads
                    .into_iter()
                    .enumerate()
                    .map(|(i, t)| (i.to_string(), t))
                    .collect(),
            )
        };
        let Some(fields) = fields else {
            if self.diagnostics.len() == diagnostic_count {
                self.report(
                    "CGT002",
                    "constructor requires a nominal record",
                    syntax.path.span,
                );
            }
            return None;
        };
        let mut substitutions = BTreeMap::new();
        if !syntax.arguments.is_empty() && syntax.arguments.len() != params.len() {
            self.report("CGT031", "wrong aggregate type argument count", syntax.span);
            return None;
        }
        for (p, t) in params.iter().zip(&syntax.arguments) {
            if let Type::Parameter { name, .. } = p {
                if let Some(t) = crate::nominal::scoped(
                    t,
                    &self.module,
                    &self.imported,
                    &self.binders,
                    &mut self.diagnostics,
                ) {
                    substitutions.insert(name.clone(), t);
                }
            }
        }
        if let Some(hint) = hint {
            self.unify(&template, hint, &mut substitutions, span);
        }
        let mut supplied = BTreeSet::new();
        let mut indices = (0..args.len()).collect::<Vec<_>>();
        indices.sort_by_key(|i| needs_context(&args[*i].value));
        for i in indices {
            let arg = &args[i];
            let field = if case.is_none() {
                arg.name
                    .as_ref()
                    .and_then(|n| fields.iter().find(|(f, _)| f == n))
            } else if arg.name.is_none() {
                fields.get(i)
            } else {
                None
            };
            if let Some((name, p)) = field {
                if !supplied.insert(name.clone()) {
                    self.report("CGT019", "duplicate aggregate field", arg.span);
                }
                let expected = crate::nominal::substitute(p, &substitutions);
                let expected = if self.context_known(&expected) {
                    Some(expected)
                } else {
                    None
                };
                if let Some(actual) = self.check_expr_as(&arg.value, expected.as_ref()) {
                    self.unify(p, &actual, &mut substitutions, arg.value.span);
                }
            } else {
                self.check_expr(&arg.value);
                self.report("CGT019", "unknown field or wrong payload naming", arg.span);
            }
        }
        if supplied.len() != fields.len() || args.len() != fields.len() {
            self.report(
                if case.is_none() { "CGT019" } else { "CGT034" },
                if case.is_none() {
                    "missing field or excess record payloads"
                } else {
                    "missing or excess aggregate payloads"
                },
                span,
            );
        }
        let mut result = syntax.clone();
        result.arguments.clear();
        for p in &params {
            let Type::Parameter { name, .. } = p else {
                continue;
            };
            let Some(t) = substitutions.get(name) else {
                self.report("CGT040", format!("ambiguous type argument `{p}`"), span);
                return None;
            };
            result
                .arguments
                .push(crate::foundation::syntax_type(t, span));
        }
        // Use original admission rules after inference, including privacy and constraints.
        crate::nominal::scoped(
            &result,
            &self.module,
            &self.imported,
            &self.binders,
            &mut self.diagnostics,
        )
    }
    fn foundation_expr(&mut self, expr: &Expr, hint: Option<&Type>) -> Option<Option<Type>> {
        match &expr.kind {
            ExprKind::Collection(values) => {
                if values.len() > 65536 {
                    self.report("CGT030", "collection length exceeds 65536", expr.span);
                    return Some(None);
                }
                let mut element = match hint {
                    Some(Type::List(t) | Type::Array(t)) => Some((**t).clone()),
                    _ => None,
                };
                let mut indices = (0..values.len()).collect::<Vec<_>>();
                indices.sort_by_key(|i| needs_context(&values[*i]));
                for i in indices {
                    let v = &values[i];
                    if let Some(t) = self.check_expr_as(v, element.as_ref()) {
                        if let Some(p) = &element {
                            self.expect_type(&t, p, v.span);
                        } else {
                            element = Some(t);
                        }
                    }
                }
                Some(
                    element
                        .map(|t| {
                            if matches!(hint, Some(Type::Array(_))) {
                                Type::Array(Box::new(t))
                            } else {
                                Type::List(Box::new(t))
                            }
                        })
                        .or_else(|| {
                            self.report(
                                "CGT040",
                                "empty collection needs an element type annotation",
                                expr.span,
                            );
                            None
                        }),
                )
            }
            ExprKind::Index { collection, index } => {
                let ct = self.check_expr(collection);
                let it = self.check_expr(index);
                if let Some(t) = it {
                    self.expect_type(&t, &Type::I64, index.span);
                }
                let element = match ct {
                    Some(Type::List(t) | Type::Array(t)) => Some(*t),
                    Some(Type::String) => Some(Type::String),
                    Some(_) => {
                        self.report(
                            "CGT003",
                            "indexing requires List, Array or String",
                            collection.span,
                        );
                        None
                    }
                    None => None,
                };
                Some(element.map(option_type))
            }
            ExprKind::Constructor {
                ty,
                case,
                arguments,
            } if !case.is_empty() => {
                Some(self.infer_aggregate(ty, Some(case), arguments, hint, expr.span))
            }
            ExprKind::Constructor { ty, arguments, .. } => {
                Some(self.foundation_call(&ty.path, &ty.arguments, arguments, hint, expr.span))
            }
            ExprKind::Call { callee, arguments } => {
                let ExprKind::Name(path) = &callee.kind else {
                    return None;
                };
                let n = path.segments.join(".");
                if matches!(n.as_str(), "range" | "arg" | "arg_count") {
                    return None;
                }
                if n.starts_with("$cgx2_cmp_") {
                    let types: Vec<_> = arguments
                        .iter()
                        .map(|a| self.check_expr(&a.value))
                        .collect();
                    let valid = match types.as_slice() {
                        [Some(a), Some(b)] if a == b => {
                            if matches!(n.as_str(), "$cgx2_cmp_eq" | "$cgx2_cmp_ne") {
                                crate::nominal::satisfies(a, "Eq", &self.module, &self.imported, 0)
                            } else {
                                *a == Type::I64
                            }
                        }
                        _ => false,
                    };
                    if !valid {
                        self.report(
                            "CGT039",
                            "comparison requires Eq or ordered i64 operands",
                            expr.span,
                        );
                    }
                    return Some(Some(Type::Bool));
                }
                if !self.functions.contains_key(&n) && !self.record_constructor(path) {
                    if let Some(result) = self.data_call(&n, arguments, hint, expr.span) {
                        return Some(result);
                    }
                }
                Some(self.foundation_call(path, &[], arguments, hint, expr.span))
            }
            _ => None,
        }
    }
    fn record_constructor(&self, path: &Path) -> bool {
        let name = path.segments.join(".");
        let own = crate::nominal::identity(&self.module, &name);
        self.module
            .records
            .iter()
            .any(|r| crate::nominal::identity(&self.module, &r.name) == own)
            || self.imported.public_records.contains_key(&name)
            || self.imported.private_records.contains(&name)
    }
    fn foundation_call(
        &mut self,
        path: &Path,
        explicit: &[cgx2_syntax::TypeSyntax],
        args: &[CallArg],
        hint: Option<&Type>,
        span: Span,
    ) -> Option<Type> {
        let name = path.segments.join(".");
        if path.segments.len() > 1 && self.local(&path.segments[0]).is_some() {
            self.report("CGT020", "ambiguous local value and module path", span);
            return None;
        }
        if path.segments.len() > 1 {
            let prefix = path.segments[..path.segments.len() - 1].join(".");
            let own = crate::nominal::identity(&self.module, &prefix);
            if matches!(prefix.as_str(), "Option" | "Result")
                || self
                    .module
                    .variants
                    .iter()
                    .any(|v| crate::nominal::identity(&self.module, &v.name) == own)
                || self.imported.public_variants.contains_key(&prefix)
                || self.imported.private_variants.contains(&prefix)
            {
                let syntax = cgx2_syntax::TypeSyntax {
                    path: Path {
                        segments: path.segments[..path.segments.len() - 1].to_vec(),
                        span,
                    },
                    arguments: vec![],
                    span,
                };
                return self.infer_aggregate(
                    &syntax,
                    path.segments.last().map(String::as_str),
                    args,
                    hint,
                    span,
                );
            }
        }
        if self.record_constructor(path) {
            return self.infer_aggregate(
                &cgx2_syntax::TypeSyntax {
                    path: path.clone(),
                    arguments: explicit.to_vec(),
                    span,
                },
                None,
                args,
                hint,
                span,
            );
        }
        let signature = if let Some(s) = self.functions.get(&name) {
            Some(s.clone())
        } else if path.segments.len() > 1 {
            self.imported_signature(path)
        } else {
            None
        };
        let Some(signature) = signature else {
            for a in args {
                self.check_expr(&a.value);
            }
            self.report(
                if args.iter().any(|a| a.name.is_some()) {
                    "CGT002"
                } else {
                    "CGT001"
                },
                if args.iter().any(|a| a.name.is_some()) {
                    format!("unknown record type `{name}`")
                } else {
                    format!("unknown function `{name}`")
                },
                path.span,
            );
            return None;
        };
        self.infer_call(&name, &signature, explicit, args, hint, span)
    }
    fn data_call(
        &mut self,
        name: &str,
        args: &[CallArg],
        hint: Option<&Type>,
        span: Span,
    ) -> Option<Option<Type>> {
        let arity = match name {
            "length" | "show" | "parse_i64" | "array" | "list" => 1,
            "get" | "append" | "concat" => 2,
            "updated" | "slice" => 3,
            _ => return None,
        };
        if args.len() != arity || args.iter().any(|a| a.name.is_some()) {
            self.report(
                "CGT005",
                format!("{name} requires {arity} positional arguments"),
                span,
            );
            for a in args {
                self.check_expr(&a.value);
            }
            return Some(None);
        }
        let first_hint = match (name, hint) {
            ("array", Some(Type::Array(t))) => Some(Type::List(t.clone())),
            ("list", Some(Type::List(t))) => Some(Type::Array(t.clone())),
            ("append", Some(t @ Type::List(_))) => Some(t.clone()),
            ("updated", Some(Type::Variant { name, arguments })) if name == "cgx2.core.Option" => {
                arguments.first().cloned()
            }
            ("get", Some(Type::Variant { name, arguments }))
                if name == "cgx2.core.Option"
                    && matches!(args[0].value.kind, ExprKind::Collection(_)) =>
            {
                arguments.first().map(|t| Type::List(Box::new(t.clone())))
            }
            _ => None,
        };
        let first_hint =
            if first_hint.is_none() && name == "append" && needs_context(&args[0].value) {
                self.check_expr(&args[1].value)
                    .map(|t| Type::List(Box::new(t)))
            } else {
                first_hint
            };
        let first = self.check_expr_as(&args[0].value, first_hint.as_ref());
        let element = match &first {
            Some(Type::List(t) | Type::Array(t)) => Some((**t).clone()),
            Some(Type::String) => Some(Type::String),
            _ => None,
        };
        let mut types = vec![first.clone()];
        for (i, a) in args.iter().enumerate().skip(1) {
            let expected = match (name, i) {
                ("get" | "slice" | "updated", 1) | ("slice", 2) => Some(Type::I64),
                ("updated", 2) | ("append", 1) => element.clone(),
                ("concat", 1) => Some(Type::String),
                _ => None,
            };
            let actual = self.check_expr_as(&a.value, expected.as_ref());
            if let (Some(a), Some(e)) = (&actual, &expected) {
                self.expect_type(a, e, args[i].span);
            }
            types.push(actual);
        }
        let result = match (name, first) {
            ("show", Some(t))
                if crate::nominal::satisfies(&t, "Show", &self.module, &self.imported, 0) =>
            {
                Some(Type::String)
            }
            ("length", Some(Type::List(_) | Type::Array(_) | Type::String)) => Some(Type::I64),
            ("get", _) => element.map(option_type),
            ("updated", Some(t @ (Type::List(_) | Type::Array(_)))) => Some(option_type(t)),
            ("append", Some(t @ Type::List(_))) => Some(t),
            ("array", Some(Type::List(t))) => Some(Type::Array(t)),
            ("list", Some(Type::Array(t))) => Some(Type::List(t)),
            ("concat", Some(Type::String)) => Some(Type::String),
            ("slice", Some(Type::String)) => Some(option_type(Type::String)),
            ("parse_i64", Some(Type::String)) => Some(Type::Variant {
                name: "cgx2.core.Result".into(),
                arguments: vec![Type::I64, Type::String],
            }),
            _ => {
                self.report(
                    "CGT039",
                    format!("unsupported {name} operand or missing constraint"),
                    span,
                );
                None
            }
        };
        Some(result)
    }
}
fn option_type(t: Type) -> Type {
    Type::Variant {
        name: "cgx2.core.Option".into(),
        arguments: vec![t],
    }
}

fn needs_context(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Collection(v) => v.is_empty() || v.iter().any(needs_context),
        ExprKind::Constructor { arguments, .. } | ExprKind::Call { arguments, .. } => {
            arguments.is_empty() || arguments.iter().any(|a| needs_context(&a.value))
        }
        _ => false,
    }
}
