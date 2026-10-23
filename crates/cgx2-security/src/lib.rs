//! Bounded, syntax-level security policy preview for one CGx2 source file.
pub mod package_v1;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use cgx2_effect_index::{index_source, Call, CallKind, Effect, Index, MAX_SOURCE_BYTES};
use cgx2_syntax::{parse_module, AssignOp, Expr, ExprKind, Function, Module, Span, Stmt, StmtKind};
use serde_json::{json, Value};

pub const MAX_POLICY_BYTES: usize = 65_536;
const MAX_TAINT_STEPS: usize = 16_384;
const MAX_CALL_DEPTH: usize = 16;

#[derive(Clone, Debug)]
pub struct Policy {
    pub allowed_effects: BTreeSet<Effect>,
    pub allow_unknown: bool,
}

pub fn parse_policy(source: &str) -> Result<Policy, String> {
    if source.len() > MAX_POLICY_BYTES {
        return Err(format!("policy exceeds {MAX_POLICY_BYTES} byte limit"));
    }
    let value: Value =
        serde_json::from_str(source).map_err(|e| format!("invalid policy JSON: {e}"))?;
    let object = value.as_object().ok_or("policy must be a JSON object")?;
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "schema_version" | "allowed_effects" | "allow_unknown"
        )
    }) {
        return Err("policy contains an unsupported field".into());
    }
    if object.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err("policy schema_version must be 1".into());
    }
    let items = object
        .get("allowed_effects")
        .and_then(Value::as_array)
        .ok_or("policy allowed_effects must be an array")?;
    let mut allowed_effects = BTreeSet::new();
    for item in items {
        let effect = match item.as_str() {
            Some("Collect") => Effect::Collect,
            Some("Foreign") => Effect::Foreign,
            Some("ManagedAlloc") => Effect::ManagedAlloc,
            Some("Mutate") => Effect::Mutate,
            Some("ReadArgs") => Effect::ReadArgs,
            _ => return Err("policy allowed_effects contains an unknown effect".into()),
        };
        if !allowed_effects.insert(effect) {
            return Err("policy allowed_effects contains a duplicate".into());
        }
    }
    let allow_unknown = match object.get("allow_unknown") {
        Some(value) => value
            .as_bool()
            .ok_or("policy allow_unknown must be Boolean")?,
        None => false,
    };
    Ok(Policy {
        allowed_effects,
        allow_unknown,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub rule_id: &'static str,
    pub severity: &'static str,
    pub message: String,
    pub span: Span,
    pub related: Option<Span>,
}

#[derive(Clone, Debug)]
pub struct Report {
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn to_json(&self) -> String {
        let findings = self
            .findings
            .iter()
            .map(|f| {
                json!({
                    "rule_id": f.rule_id,
                    "severity": f.severity,
                    "message": f.message,
                    "span": span_value(f.span),
                    "related": f.related.map(span_value),
                })
            })
            .collect::<Vec<_>>();
        json!({"schema_version":1,"span_unit":"utf8_byte_offset","findings":findings}).to_string()
    }

    pub fn to_text(&self, file: &str, source: &str) -> String {
        let mut out = String::new();
        for finding in &self.findings {
            let (line, col) = line_column(source, finding.span.start);
            out.push_str(&format!(
                "{} {} {file}:{line}:{col} [{}..{}): {}\n",
                finding.rule_id,
                finding.severity,
                finding.span.start,
                finding.span.end,
                finding.message
            ));
        }
        out
    }

    pub fn to_sarif(&self, file: &str, source: &str) -> String {
        let uri = uri_path(file);
        let rules = [
            ("CGSEC001", "Disallowed effect"),
            ("CGSEC002", "Unknown call"),
            ("CGSEC003", "Tainted foreign sink"),
            ("CGSEC004", "Unknown taint path"),
        ]
        .into_iter()
        .map(|(id, name)| json!({"id":id,"name":name}))
        .collect::<Vec<_>>();
        let results = self.findings.iter().map(|f| {
            let mut result = json!({
                "ruleId": f.rule_id,
                "level": if f.severity == "error" { "error" } else { "warning" },
                "message": {"text": f.message},
                "locations": [{"physicalLocation": {"artifactLocation": {"uri": uri.as_str()}, "region": sarif_region(source, f.span)}}]
            });
            if let Some(related) = f.related {
                result["relatedLocations"] = json!([{"id":1,"message":{"text":"taint source"},"physicalLocation":{"artifactLocation":{"uri":uri.as_str()},"region":sarif_region(source, related)}}]);
            }
            result
        }).collect::<Vec<_>>();
        json!({
            "$schema":"https://json.schemastore.org/sarif-2.1.0.json",
            "version":"2.1.0",
            "runs":[{"tool":{"driver":{"name":"cgx2-security","rules":rules}},"columnKind":"unicodeCodePoints","results":results}]
        })
        .to_string()
    }
}

fn span_value(span: Span) -> Value {
    json!({"start":span.start,"end":span.end})
}

fn uri_path(path: &str) -> String {
    let mut uri = String::new();
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            uri.push(char::from(byte));
        } else {
            write!(uri, "%{byte:02X}").expect("write to string");
        }
    }
    uri
}

fn line_column(source: &str, offset: usize) -> (usize, usize) {
    let mut offset = offset.min(source.len());
    while !source.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &source[..offset];
    let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    (line, column)
}

fn sarif_region(source: &str, span: Span) -> Value {
    let (start_line, start_column) = line_column(source, span.start);
    let (end_line, end_column) = line_column(source, span.end);
    json!({"startLine":start_line,"startColumn":start_column,"endLine":end_line,"endColumn":end_column})
}

pub fn audit(source: &str, policy: &Policy) -> Result<Report, String> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(format!("source exceeds {MAX_SOURCE_BYTES} byte limit"));
    }
    let index = index_source(source, true).map_err(|errors| {
        errors
            .iter()
            .map(|e| {
                format!(
                    "{} {} [{}..{}): {}",
                    e.code, e.stage, e.span.start, e.span.end, e.message
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let module = parse_module(source).map_err(|errors| {
        errors
            .iter()
            .map(|e| e.message.clone())
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let mut findings = Vec::new();
    policy_findings(&index, &module, policy, &mut findings);
    let mut analyzer = TaintAnalyzer::new(&module, &mut findings);
    analyzer.run_module(&module);
    findings.sort_by(|a, b| {
        (
            a.span.start,
            a.span.end,
            a.rule_id,
            &a.message,
            a.related.map(|s| s.start),
        )
            .cmp(&(
                b.span.start,
                b.span.end,
                b.rule_id,
                &b.message,
                b.related.map(|s| s.start),
            ))
    });
    findings.dedup();
    Ok(Report { findings })
}

fn policy_findings(index: &Index, module: &Module, policy: &Policy, out: &mut Vec<Finding>) {
    let local: BTreeMap<&str, &BTreeSet<Effect>> = index
        .functions
        .iter()
        .map(|f| (f.name.as_str(), &f.transitive_effects))
        .collect();
    for entry in index
        .functions
        .iter()
        .chain(std::iter::once(&index.top_level))
    {
        for call in &entry.calls {
            let effects = call_effects(call, &local, entry.name != "<top-level>");
            for effect in effects {
                if effect == Effect::Unknown {
                    if !policy.allow_unknown {
                        out.push(Finding {
                            rule_id: "CGSEC002",
                            severity: "error",
                            message: format!(
                                "unknown effect from call {}",
                                call.target.as_deref().unwrap_or("<computed>")
                            ),
                            span: call.span,
                            related: None,
                        });
                    }
                } else if !policy.allowed_effects.contains(&effect) {
                    out.push(Finding {
                        rule_id: "CGSEC001",
                        severity: "error",
                        message: format!("effect {} is not allowed", effect.as_str()),
                        span: call.span,
                        related: None,
                    });
                }
            }
        }
    }
    for function in &module.functions {
        assignment_findings(&function.body, policy, out);
    }
    assignment_findings(&module.statements, policy, out);
}

fn call_effects(
    call: &Call,
    local: &BTreeMap<&str, &BTreeSet<Effect>>,
    function_body: bool,
) -> BTreeSet<Effect> {
    let mut result = BTreeSet::new();
    match call.kind {
        CallKind::Local => {
            if let Some(effects) = call.target.as_deref().and_then(|name| local.get(name)) {
                result.extend(effects.iter().copied());
            } else {
                result.insert(Effect::Unknown);
            }
        }
        CallKind::Unknown => {
            result.insert(Effect::Unknown);
        }
        CallKind::Foreign => {
            result.extend([Effect::Foreign, Effect::Unknown]);
        }
        CallKind::Builtin => {
            let target = call.target.as_deref().unwrap_or("");
            if target.starts_with("memory.") && function_body {
                result.insert(Effect::Unknown);
            }
            match target {
                "arg" | "arg_count" => {
                    result.insert(Effect::ReadArgs);
                }
                "memory.region"
                | "memory.owned_bytes"
                | "memory.alloc_bytes"
                | "memory.copy_to_owned" => {
                    result.insert(Effect::ManagedAlloc);
                }
                "memory.promote_owned" => {
                    result.extend([Effect::ManagedAlloc, Effect::Mutate]);
                }
                "memory.collect" => {
                    result.insert(Effect::Collect);
                }
                "memory.move" | "memory.borrow_mut" | "memory.end_borrow" | "memory.drop"
                | "memory.close" => {
                    result.insert(Effect::Mutate);
                }
                _ => {}
            }
        }
    }
    result
}

fn assignment_findings(statements: &[Stmt], policy: &Policy, out: &mut Vec<Finding>) {
    for stmt in statements {
        match &stmt.kind {
            StmtKind::If {
                then_body,
                else_body,
                ..
            } => {
                assignment_findings(then_body, policy, out);
                assignment_findings(else_body, policy, out);
            }
            StmtKind::Assign { .. } if !policy.allowed_effects.contains(&Effect::Mutate) => out
                .push(Finding {
                    rule_id: "CGSEC001",
                    severity: "error",
                    message: "effect Mutate is not allowed".into(),
                    span: stmt.span,
                    related: None,
                }),
            StmtKind::NoGc { body, .. } | StmtKind::For { body, .. } => {
                assignment_findings(body, policy, out)
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy)]
enum Taint {
    Clean,
    Tainted(Span),
    Unknown,
}

impl Taint {
    fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Tainted(a), Self::Tainted(b)) => {
                Self::Tainted(if a.start <= b.start { a } else { b })
            }
            (Self::Tainted(a), _) | (_, Self::Tainted(a)) => Self::Tainted(a),
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            _ => Self::Clean,
        }
    }
}

struct TaintAnalyzer<'a> {
    functions: BTreeMap<&'a str, &'a Function>,
    imported_roots: BTreeSet<&'a str>,
    findings: &'a mut Vec<Finding>,
    steps: usize,
}

impl<'a> TaintAnalyzer<'a> {
    fn new(module: &'a Module, findings: &'a mut Vec<Finding>) -> Self {
        Self {
            functions: module
                .functions
                .iter()
                .map(|f| (f.name.as_str(), f))
                .collect(),
            imported_roots: module
                .imports
                .iter()
                .filter_map(|path| path.segments.first().map(String::as_str))
                .collect(),
            findings,
            steps: 0,
        }
    }

    fn run(&mut self) {
        for function in self.functions.values().copied().collect::<Vec<_>>() {
            let mut env = function
                .parameters
                .iter()
                .map(|p| (p.name.clone(), Taint::Unknown))
                .collect();
            self.statements(&function.body, &mut env, &mut vec![function.name.as_str()]);
        }
    }

    fn run_module(&mut self, module: &'a Module) {
        self.run();
        self.statements(&module.statements, &mut BTreeMap::new(), &mut Vec::new());
    }

    fn statements(
        &mut self,
        statements: &'a [Stmt],
        env: &mut BTreeMap<String, Taint>,
        stack: &mut Vec<&'a str>,
    ) -> Taint {
        let mut result = Taint::Clean;
        for stmt in statements {
            if self.steps >= MAX_TAINT_STEPS {
                self.unknown(stmt.span, "taint analysis step limit reached");
                return Taint::Unknown;
            }
            self.steps += 1;
            match &stmt.kind {
                StmtKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expr(condition, env, stack);
                    let mut left = env.clone();
                    let mut right = env.clone();
                    let a = self.branch(then_body, &mut left, stack);
                    let b = self.branch(else_body, &mut right, stack);
                    for (name, value) in env.iter_mut() {
                        *value = left[name].join(right[name]);
                    }
                    result = result.join(a).join(b);
                }
                StmtKind::Let { name, value, .. } => {
                    let value = self.expr(value, env, stack);
                    env.insert(name.clone(), value);
                }
                StmtKind::Assign { target, op, value } => {
                    let value = self.expr(value, env, stack);
                    if target.segments.len() == 1 {
                        let name = &target.segments[0];
                        let value = if *op == AssignOp::Add {
                            env.get(name).copied().unwrap_or(Taint::Unknown).join(value)
                        } else {
                            value
                        };
                        env.insert(name.clone(), value);
                    } else {
                        self.unknown(
                            stmt.span,
                            "alias or qualified assignment is outside taint model",
                        );
                    }
                }
                StmtKind::Return(Some(expr)) => {
                    return result.join(self.expr(expr, env, stack));
                }
                StmtKind::Expr(expr) => {
                    result = result.join(self.expr(expr, env, stack));
                }
                StmtKind::NoGc { budget, body } => {
                    self.expr(budget, env, stack);
                    let mut nested = env.clone();
                    result = result.join(self.statements(body, &mut nested, stack));
                    for (key, value) in nested {
                        if env.contains_key(&key) {
                            env.insert(key, value);
                        }
                    }
                }
                StmtKind::For { iter, body, .. } => {
                    self.expr(iter, env, stack);
                    self.unknown(stmt.span, "loop taint flow is outside bounded model");
                    let mut nested = env.clone();
                    self.statements(body, &mut nested, stack);
                    for value in env.values_mut() {
                        *value = Taint::Unknown;
                    }
                    result = Taint::Unknown;
                }
                StmtKind::Return(None) => {
                    return result;
                }
            }
        }
        result
    }

    fn expr(
        &mut self,
        expr: &'a Expr,
        env: &mut BTreeMap<String, Taint>,
        stack: &mut Vec<&'a str>,
    ) -> Taint {
        if self.steps >= MAX_TAINT_STEPS {
            self.unknown(expr.span, "taint analysis step limit reached");
            return Taint::Unknown;
        }
        self.steps += 1;
        match &expr.kind {
            ExprKind::Name(path) if path.segments.len() == 1 => env
                .get(&path.segments[0])
                .copied()
                .unwrap_or(Taint::Unknown),
            ExprKind::Name(_) => Taint::Unknown,
            ExprKind::Integer(_) | ExprKind::StringLiteral(_) | ExprKind::Boolean(_) => {
                Taint::Clean
            }
            ExprKind::Binary { left, right, .. } => self
                .expr(left, env, stack)
                .join(self.expr(right, env, stack)),
            ExprKind::Call { callee, arguments } => {
                let name = match &callee.kind {
                    ExprKind::Name(path) => Some(path.segments.join(".")),
                    _ => None,
                };
                let args = arguments
                    .iter()
                    .map(|arg| self.expr(&arg.value, env, stack))
                    .collect::<Vec<_>>();
                if name.as_deref().is_some_and(|name| {
                    let root = name.split('.').next().unwrap_or(name);
                    env.contains_key(root) || self.imported_roots.contains(root)
                }) {
                    if args.iter().any(|value| matches!(value, Taint::Tainted(_))) {
                        self.unknown(expr.span, "tainted value enters shadowed or imported call");
                    }
                    return Taint::Unknown;
                }
                match name.as_deref() {
                    Some(name) if self.functions.contains_key(name) => {
                        if stack.len() >= MAX_CALL_DEPTH || stack.contains(&name) {
                            self.unknown(
                                expr.span,
                                "recursive or deep local call has unknown taint",
                            );
                            return Taint::Unknown;
                        }
                        let function = self.functions[name];
                        if function.parameters.len() != args.len() {
                            self.unknown(expr.span, "local call argument count is unknown");
                            return Taint::Unknown;
                        }
                        let mut local = function
                            .parameters
                            .iter()
                            .zip(args)
                            .map(|(p, v)| (p.name.clone(), v))
                            .collect();
                        stack.push(function.name.as_str());
                        let result = self.statements(&function.body, &mut local, stack);
                        stack.pop();
                        result
                    }
                    Some("arg") => Taint::Tainted(expr.span),
                    Some("arg_count") => Taint::Clean,
                    Some("ffi.call" | "python.call") => {
                        for value in args {
                            match value {
                                Taint::Tainted(source) => self.findings.push(Finding {
                                    rule_id: "CGSEC003",
                                    severity: "warning",
                                    message: format!(
                                        "tainted arg(...) value reaches {}",
                                        name.as_deref().unwrap_or("foreign sink")
                                    ),
                                    span: expr.span,
                                    related: Some(source),
                                }),
                                Taint::Unknown => self.unknown(
                                    expr.span,
                                    "foreign sink receives value with unknown taint",
                                ),
                                Taint::Clean => {}
                            }
                        }
                        Taint::Unknown
                    }
                    _ => {
                        if args.iter().any(|value| matches!(value, Taint::Tainted(_))) {
                            self.unknown(expr.span, "tainted value enters unresolved call");
                        }
                        Taint::Unknown
                    }
                }
            }
        }
    }

    fn branch(
        &mut self,
        body: &'a [Stmt],
        env: &mut BTreeMap<String, Taint>,
        stack: &mut Vec<&'a str>,
    ) -> Taint {
        let mut shadows = BTreeMap::new();
        let mut result = Taint::Clean;
        for stmt in body {
            if let StmtKind::Let { name, .. } = &stmt.kind {
                shadows
                    .entry(name.clone())
                    .or_insert_with(|| env.get(name).copied());
            }
            result = result.join(self.statements(std::slice::from_ref(stmt), env, stack));
        }
        for (name, value) in shadows {
            if let Some(value) = value {
                env.insert(name, value);
            } else {
                env.remove(&name);
            }
        }
        result
    }

    fn unknown(&mut self, span: Span, message: &str) {
        self.findings.push(Finding {
            rule_id: "CGSEC004",
            severity: "warning",
            message: message.into(),
            span,
            related: None,
        });
    }
}
