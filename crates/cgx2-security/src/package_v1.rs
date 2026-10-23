//! Bounded explicit-flow advisory analysis over provenance-checked packages.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use cgx2_effect_index::package_v1::{
    index_checked_package, index_package, location_json, Identity, Location, PackageAnalysis,
    Resolution,
};
use cgx2_package::CheckedPackage;
use cgx2_syntax::{AssignOp, Expr, ExprKind, Function, Stmt, StmtKind};
use serde_json::{json, Value};

use crate::{
    parse_policy, sarif_region, uri_path, Policy, MAX_CALL_DEPTH, MAX_POLICY_BYTES, MAX_TAINT_STEPS,
};

#[derive(Clone, Debug)]
pub struct PackagePolicy {
    pub effects: Policy,
    pub allowed_capabilities: BTreeSet<String>,
}

/// Package policy schema 2 adds a required capability allowlist to schema 1.
pub fn parse_package_policy(source: &str) -> Result<PackagePolicy, String> {
    if source.len() > MAX_POLICY_BYTES {
        return Err("package policy exceeds byte limit".into());
    }
    let mut value: Value = serde_json::from_str(source).map_err(|e| e.to_string())?;
    let object = value
        .as_object_mut()
        .ok_or("package policy must be an object")?;
    if object.get("schema_version").and_then(Value::as_u64) != Some(2) {
        return Err("package policy schema_version must be 2".into());
    }
    let caps = object
        .remove("allowed_capabilities")
        .ok_or("allowed_capabilities is required")?;
    let mut allowed_capabilities = BTreeSet::new();
    for cap in caps
        .as_array()
        .ok_or("allowed_capabilities must be an array")?
    {
        let cap = cap
            .as_str()
            .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
            .ok_or("invalid capability")?;
        if !allowed_capabilities.insert(cap.to_owned()) {
            return Err("duplicate capability".into());
        }
    }
    object.insert("schema_version".into(), json!(1));
    let effects = parse_policy(&value.to_string())?;
    Ok(PackagePolicy {
        effects,
        allowed_capabilities,
    })
}

#[derive(Clone, Copy, Debug)]
pub struct AnalysisLimits {
    pub max_steps: usize,
    pub max_call_depth: usize,
}
impl Default for AnalysisLimits {
    fn default() -> Self {
        Self {
            max_steps: MAX_TAINT_STEPS,
            max_call_depth: MAX_CALL_DEPTH,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageFinding {
    pub rule_id: &'static str,
    pub severity: &'static str,
    pub message: String,
    pub location: Location,
    /// Source origin followed by relevant checked callee declarations, when known.
    pub related: Vec<Location>,
}

#[derive(Clone, Debug)]
pub struct PackageReport {
    analysis: PackageAnalysis,
    findings: Vec<PackageFinding>,
    limits: AnalysisLimits,
}

pub fn audit_package(
    root: &Path,
    policy: &PackagePolicy,
    limits: AnalysisLimits,
) -> Result<PackageReport, String> {
    analyze(index_package(root).map_err(diagnostics)?, policy, limits)
}
pub fn audit_checked_package(
    checked: &CheckedPackage,
    policy: &PackagePolicy,
    limits: AnalysisLimits,
) -> Result<PackageReport, String> {
    analyze(
        index_checked_package(checked).map_err(diagnostics)?,
        policy,
        limits,
    )
}
fn diagnostics(errors: Vec<cgx2_package::PackageDiagnostic>) -> String {
    errors
        .iter()
        .map(|e| format!("{} {}: {}", e.code, e.file.display(), e.message))
        .collect::<Vec<_>>()
        .join("\n")
}
fn analyze(
    analysis: PackageAnalysis,
    policy: &PackagePolicy,
    limits: AnalysisLimits,
) -> Result<PackageReport, String> {
    if limits.max_steps == 0
        || limits.max_steps > MAX_TAINT_STEPS
        || limits.max_call_depth == 0
        || limits.max_call_depth > MAX_CALL_DEPTH
    {
        return Err(format!(
            "limits require 1..={MAX_TAINT_STEPS} steps and 1..={MAX_CALL_DEPTH} call depth"
        ));
    }
    let mut findings = Vec::new();
    for entry in analysis.entries() {
        let mut check = |location: &Location,
                         effects: &BTreeSet<String>,
                         capabilities: &BTreeSet<String>,
                         related: Vec<Location>| {
            for effect in effects {
                let rule = if effect == "Unknown" {
                    if policy.effects.allow_unknown {
                        continue;
                    }
                    "CGSEC002"
                } else if policy
                    .effects
                    .allowed_effects
                    .iter()
                    .any(|e| e.as_str() == effect)
                {
                    continue;
                } else {
                    "CGSEC001"
                };
                findings.push(PackageFinding {
                    rule_id: rule,
                    severity: "warning",
                    message: format!("effect `{effect}` is unknown or absent from package policy"),
                    location: location.clone(),
                    related: related.clone(),
                });
            }
            for cap in capabilities.difference(&policy.allowed_capabilities) {
                findings.push(PackageFinding {
                    rule_id: "CGSEC005",
                    severity: "warning",
                    message: format!("capability `{cap}` is absent from package policy"),
                    location: location.clone(),
                    related: related.clone(),
                });
            }
        };
        check(
            &entry.declaration,
            &entry.effects,
            &entry.capabilities,
            vec![],
        );
        for call in &entry.calls {
            if let Resolution::Function(id) = &call.resolution {
                let target = analysis.entry(id).expect("checked entry");
                check(
                    &call.location,
                    &target.effects,
                    &target.capabilities,
                    vec![target.declaration.clone()],
                );
            } else if call.resolution == Resolution::Unknown && !policy.effects.allow_unknown {
                check(
                    &call.location,
                    &BTreeSet::from(["Unknown".into()]),
                    &BTreeSet::new(),
                    vec![],
                );
            }
        }
    }
    Analyzer {
        analysis: &analysis,
        findings: &mut findings,
        limits,
        steps: 0,
        exhausted: false,
    }
    .run();
    findings.sort_by(|a, b| {
        location_key(&a.location)
            .cmp(&location_key(&b.location))
            .then(a.rule_id.cmp(b.rule_id))
            .then(a.message.cmp(&b.message))
            .then_with(|| {
                a.related
                    .iter()
                    .map(location_key)
                    .cmp(b.related.iter().map(location_key))
            })
    });
    findings.dedup();
    analysis.verify_unchanged().map_err(diagnostics)?;
    Ok(PackageReport {
        analysis,
        findings,
        limits,
    })
}
fn location_key(l: &Location) -> (&Path, usize, usize) {
    (&l.source, l.span.start, l.span.end)
}
impl PackageReport {
    pub fn findings(&self) -> &[PackageFinding] {
        &self.findings
    }
    pub fn analysis(&self) -> &PackageAnalysis {
        &self.analysis
    }
    pub fn to_json(&self) -> Result<String, String> {
        self.analysis.verify_unchanged().map_err(diagnostics)?;
        Ok(json!({"schema_version":2,"analysis":"checked_package_security_v1","advisory":true,"span_unit":"utf8_byte_offset",
            "manifest_sha256":self.analysis.checked().manifest_sha256(),
            "provenance":self.analysis.checked().modules().iter().map(|m| json!({"source":m.source(),"sha256":m.source_sha256()})).collect::<Vec<_>>(),
            "limits":{"max_steps":self.limits.max_steps,"max_call_depth":self.limits.max_call_depth},
            "findings":self.findings.iter().map(|f| json!({"rule_id":f.rule_id,"severity":f.severity,"message":f.message,"location":location_json(&f.location),"related":f.related.iter().map(location_json).collect::<Vec<_>>()})).collect::<Vec<_>>()
        }).to_string())
    }
    pub fn to_text(&self) -> Result<String, String> {
        self.analysis.verify_unchanged().map_err(diagnostics)?;
        Ok(self
            .findings
            .iter()
            .map(|f| {
                format!(
                    "{}:{} [{}..{}): {}\n",
                    f.location.source.display(),
                    f.rule_id,
                    f.location.span.start,
                    f.location.span.end,
                    f.message
                )
            })
            .collect())
    }
    pub fn to_sarif(&self) -> Result<String, String> {
        self.analysis.verify_unchanged().map_err(diagnostics)?;
        let physical = |l: &Location| {
            let module = self
                .analysis
                .checked()
                .modules()
                .iter()
                .find(|m| m.source() == l.source)
                .expect("snapshot location");
            json!({"artifactLocation":{"uri":uri_path(&l.source.to_string_lossy())},"region":sarif_region(module.text(), l.span)})
        };
        let rules = [
            ("CGSEC001", "Disallowed effect"),
            ("CGSEC002", "Unknown call"),
            ("CGSEC003", "Tainted foreign sink"),
            ("CGSEC004", "Unknown taint path"),
            ("CGSEC005", "Disallowed capability"),
        ];
        Ok(json!({"$schema":"https://json.schemastore.org/sarif-2.1.0.json","version":"2.1.0","runs":[{"tool":{"driver":{"name":"cgx2-security","rules":rules.iter().map(|(id,name)| json!({"id":id,"name":name})).collect::<Vec<_>>()}},"columnKind":"unicodeCodePoints","results":self.findings.iter().map(|f| json!({"ruleId":f.rule_id,"level":f.severity,"message":{"text":f.message},"locations":[{"physicalLocation":physical(&f.location)}],"relatedLocations":f.related.iter().enumerate().map(|(i,l)| json!({"id":i+1,"message":{"text":"taint origin or checked declaration"},"physicalLocation":physical(l)})).collect::<Vec<_>>()})).collect::<Vec<_>>()}]}).to_string())
    }
}

#[derive(Clone, Debug, Default)]
struct Flow {
    origin: Option<Location>,
    unknown: bool,
}
impl Flow {
    fn unknown() -> Self {
        Self {
            origin: None,
            unknown: true,
        }
    }
    fn join(mut self, other: Self) -> Self {
        self.unknown |= other.unknown;
        if let Some(origin) = other.origin {
            if self
                .origin
                .as_ref()
                .is_none_or(|prior| location_key(&origin) < location_key(prior))
            {
                self.origin = Some(origin);
            }
        }
        self
    }
}
type Env = BTreeMap<String, Flow>;
struct Analyzer<'a> {
    analysis: &'a PackageAnalysis,
    findings: &'a mut Vec<PackageFinding>,
    limits: AnalysisLimits,
    steps: usize,
    exhausted: bool,
}
impl Analyzer<'_> {
    fn finding(
        &mut self,
        rule_id: &'static str,
        location: Location,
        message: &str,
        related: Vec<Location>,
    ) {
        self.findings.push(PackageFinding {
            rule_id,
            severity: "warning",
            message: message.into(),
            location,
            related,
        });
    }
    fn step(&mut self, location: &Location) -> bool {
        if self.steps >= self.limits.max_steps {
            if !self.exhausted {
                self.finding(
                    "CGSEC004",
                    location.clone(),
                    "package taint step budget exhausted; remaining paths are unknown",
                    vec![],
                );
                self.exhausted = true;
            }
            return false;
        }
        self.steps += 1;
        true
    }
    fn run(&mut self) {
        for module in self.analysis.checked().modules() {
            for function in &module.syntax().functions {
                let mut env = function
                    .parameters
                    .iter()
                    .map(|p| (p.name.clone(), Flow::unknown()))
                    .collect();
                let id = Identity {
                    source: module.source().into(),
                    name: function.name.clone(),
                };
                self.statements(
                    module.source(),
                    &function.body,
                    &mut env,
                    &mut vec![id],
                    false,
                );
            }
            self.statements(
                module.source(),
                &module.syntax().statements,
                &mut Env::new(),
                &mut Vec::new(),
                false,
            );
        }
    }
    fn function(&self, id: &Identity) -> &Function {
        self.analysis
            .checked()
            .modules()
            .iter()
            .find(|m| m.source() == id.source)
            .expect("checked module")
            .syntax()
            .functions
            .iter()
            .find(|f| f.name == id.name)
            .expect("checked function")
    }
    fn statements(
        &mut self,
        source: &Path,
        statements: &[Stmt],
        env: &mut Env,
        stack: &mut Vec<Identity>,
        scoped: bool,
    ) -> Flow {
        let mut shadows = BTreeMap::new();
        let mut result = Flow::default();
        for stmt in statements {
            let location = Location {
                source: source.into(),
                span: stmt.span,
            };
            if !self.step(&location) {
                result = result.join(Flow::unknown());
                break;
            }
            match &stmt.kind {
                StmtKind::Match { .. } | StmtKind::LoopContract { .. } => unreachable!(
                    "S2a profile rejected by effect-index admission before security analysis"
                ),
                StmtKind::Let { name, value, .. } => {
                    let flow = self.expr(source, value, env, stack);
                    if scoped {
                        shadows
                            .entry(name.clone())
                            .or_insert_with(|| env.get(name).cloned());
                    }
                    env.insert(name.clone(), flow);
                }
                StmtKind::Assign { target, op, value } => {
                    let value = self.expr(source, value, env, stack);
                    if target.segments.len() == 1 {
                        let name = &target.segments[0];
                        let value = if *op == AssignOp::Add {
                            env.get(name)
                                .cloned()
                                .unwrap_or_else(Flow::unknown)
                                .join(value)
                        } else {
                            value
                        };
                        env.insert(name.clone(), value);
                    } else {
                        self.finding(
                            "CGSEC004",
                            location,
                            "qualified assignment has unknown taint",
                            vec![],
                        );
                        for value in env.values_mut() {
                            value.unknown = true;
                        }
                    }
                }
                StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) => {
                    result = result.join(self.expr(source, expr, env, stack));
                }
                StmtKind::Return(None) => {}
                StmtKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expr(source, condition, env, stack);
                    let mut left = env.clone();
                    let mut right = env.clone();
                    result = result
                        .join(self.statements(source, then_body, &mut left, stack, true))
                        .join(self.statements(source, else_body, &mut right, stack, true));
                    for (name, flow) in env.iter_mut() {
                        *flow = left[name].clone().join(right[name].clone());
                    }
                }
                StmtKind::NoGc { budget, body } => {
                    self.expr(source, budget, env, stack);
                    result = result.join(self.statements(source, body, env, stack, true));
                }
                StmtKind::For {
                    binding,
                    iter,
                    body,
                } => {
                    self.expr(source, iter, env, stack);
                    self.finding(
                        "CGSEC004",
                        location,
                        "loop-carried taint is conservatively unknown",
                        vec![],
                    );
                    let mut nested = env.clone();
                    for value in nested.values_mut() {
                        value.unknown = true;
                    }
                    nested.insert(binding.clone(), Flow::unknown());
                    result = result
                        .join(self.statements(source, body, &mut nested, stack, true))
                        .join(Flow::unknown());
                    for (name, flow) in env.iter_mut() {
                        *flow = flow
                            .clone()
                            .join(nested.get(name).cloned().unwrap_or_else(Flow::unknown))
                            .join(Flow::unknown());
                    }
                }
            }
        }
        for (name, old) in shadows {
            if let Some(value) = old {
                env.insert(name, value);
            } else {
                env.remove(&name);
            }
        }
        result
    }
    fn expr(
        &mut self,
        source: &Path,
        expr: &Expr,
        env: &mut Env,
        stack: &mut Vec<Identity>,
    ) -> Flow {
        let location = Location {
            source: source.into(),
            span: expr.span,
        };
        if !self.step(&location) {
            return Flow::unknown();
        }
        match &expr.kind {
            ExprKind::Constructor { .. } | ExprKind::Collection(_) | ExprKind::Index { .. } => {
                unreachable!(
                    "S2a profile rejected by effect-index admission before security analysis"
                )
            }
            ExprKind::Integer(_) | ExprKind::Boolean(_) | ExprKind::StringLiteral(_) => {
                Flow::default()
            }
            ExprKind::Name(path) => env
                .get(&path.segments.join("."))
                .cloned()
                .unwrap_or_else(Flow::unknown),
            ExprKind::Binary { left, right, .. } => self
                .expr(source, left, env, stack)
                .join(self.expr(source, right, env, stack)),
            ExprKind::Call { callee, arguments } => {
                let name = if let ExprKind::Name(path) = &callee.kind {
                    Some(path.segments.join("."))
                } else {
                    self.expr(source, callee, env, stack);
                    None
                };
                let args: Vec<_> = arguments
                    .iter()
                    .map(|a| self.expr(source, &a.value, env, stack))
                    .collect();
                let joined = args.iter().cloned().fold(Flow::default(), Flow::join);
                let call = self.analysis.call(source, expr.span).cloned();
                let shadowed = name
                    .as_ref()
                    .is_some_and(|n| env.contains_key(n.split('.').next().unwrap_or(n)));
                if let Some(call) = call.filter(|_| !shadowed) {
                    match call.resolution {
                        Resolution::Function(id) => {
                            let function = self.function(&id).clone();
                            if function.foreign.is_some() {
                                let mut related = joined.origin.iter().cloned().collect::<Vec<_>>();
                                related.extend(call.declaration.iter().cloned());
                                if joined.origin.is_some() {
                                    self.finding(
                                        "CGSEC003",
                                        location.clone(),
                                        "tainted argument reaches checked foreign declaration",
                                        related.clone(),
                                    );
                                }
                                if joined.unknown {
                                    self.finding(
                                        "CGSEC004",
                                        location.clone(),
                                        "foreign argument taint is unknown",
                                        related,
                                    );
                                }
                                self.finding("CGSEC004", location, "foreign return taint is unknown; implementation is outside checked source", call.declaration.into_iter().collect());
                                return Flow::unknown();
                            }
                            if stack.len() >= self.limits.max_call_depth || stack.contains(&id) {
                                self.finding(
                                    "CGSEC004",
                                    location,
                                    "recursive or over-depth package call has unknown taint",
                                    call.declaration.into_iter().collect(),
                                );
                                return joined.join(Flow::unknown());
                            }
                            if function.parameters.len() != args.len()
                                || arguments.iter().any(|a| a.name.is_some())
                            {
                                self.finding(
                                    "CGSEC004",
                                    location,
                                    "unsupported argument mapping has unknown taint",
                                    call.declaration.into_iter().collect(),
                                );
                                return joined.join(Flow::unknown());
                            }
                            let mut nested = function
                                .parameters
                                .iter()
                                .zip(args)
                                .map(|(p, v)| (p.name.clone(), v))
                                .collect();
                            stack.push(id.clone());
                            let value = self.statements(
                                &id.source,
                                &function.body,
                                &mut nested,
                                stack,
                                false,
                            );
                            stack.pop();
                            return value;
                        }
                        Resolution::Builtin => {
                            if name.as_deref() == Some("arg") {
                                return Flow {
                                    origin: Some(location),
                                    unknown: false,
                                };
                            }
                            if name.as_deref() == Some("arg_count")
                                || name
                                    .as_deref()
                                    .is_some_and(|n| n.starts_with("$cgx2_cmp_") || n == "range")
                            {
                                return joined;
                            }
                            if name.as_deref().is_some_and(|n| n.starts_with("memory.")) {
                                self.finding(
                                    "CGSEC004",
                                    location,
                                    "memory alias/state taint is outside the scalar package model",
                                    vec![],
                                );
                                return joined.join(Flow::unknown());
                            }
                            // Checked record construction joins every field, without sanitizing.
                            return joined;
                        }
                        Resolution::Unknown => {}
                    }
                }
                self.finding(
                    "CGSEC004",
                    location,
                    "unresolved, computed, or shadowed package call has unknown taint",
                    vec![],
                );
                joined.join(Flow::unknown())
            }
        }
    }
}
