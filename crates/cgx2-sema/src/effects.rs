use std::collections::{BTreeMap, BTreeSet};

use cgx2_syntax::{Expr, ExprKind, Function, Module, Span, Stmt, StmtKind};

use crate::TypeDiagnostic;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectSummary {
    pub effects: BTreeSet<String>,
    pub capabilities: BTreeSet<String>,
    pub effects_explicit: bool,
    pub capabilities_explicit: bool,
}

#[derive(Clone)]
struct Site {
    callee: Option<String>,
    effect: Option<&'static str>,
    capability: Option<&'static str>,
    span: Span,
}

fn expr_sites(expr: &Expr, sites: &mut Vec<Site>) {
    match &expr.kind {
        ExprKind::Binary { left, right, .. } => {
            expr_sites(left, sites);
            expr_sites(right, sites);
        }
        ExprKind::Call { callee, arguments } => {
            for argument in arguments {
                expr_sites(&argument.value, sites);
            }
            let (name, effect, capability) = match &callee.kind {
                ExprKind::Name(path) => {
                    let name = path.segments.join(".");
                    match name.as_str() {
                        "$cgx2_cmp_eq" | "$cgx2_cmp_ne" | "$cgx2_cmp_lt" | "$cgx2_cmp_le"
                        | "$cgx2_cmp_gt" | "$cgx2_cmp_ge" => return,
                        "arg" | "arg_count" => (None, Some("ReadArgs"), Some("ArgsRead")),
                        "memory.collect" => (None, Some("Collect"), Some("MemoryCollect")),
                        n if n.starts_with("memory.alloc")
                            || n == "memory.region"
                            || n == "memory.owned_bytes" =>
                        {
                            (None, Some("ManagedAlloc"), None)
                        }
                        n if n.starts_with("foreign.") || n.starts_with("python.") => {
                            (None, Some("Foreign"), Some("ForeignCall"))
                        }
                        n if n.starts_with("memory.") => (None, Some("Mutate"), None),
                        n if n.contains('.') => (None, Some("Unknown"), None),
                        _ => (Some(name), None, None),
                    }
                }
                _ => (None, Some("Unknown"), None),
            };
            sites.push(Site {
                callee: name,
                effect,
                capability,
                span: expr.span,
            });
        }
        _ => {}
    }
}

fn stmt_sites(stmt: &Stmt, sites: &mut Vec<Site>) {
    match &stmt.kind {
        StmtKind::Let { value, .. } | StmtKind::Expr(value) | StmtKind::Return(Some(value)) => {
            expr_sites(value, sites)
        }
        StmtKind::Assign { value, .. } => {
            sites.push(Site {
                callee: None,
                effect: Some("Mutate"),
                capability: None,
                span: stmt.span,
            });
            expr_sites(value, sites);
        }
        StmtKind::For { iter, body, .. } => {
            expr_sites(iter, sites);
            for nested in body {
                stmt_sites(nested, sites);
            }
        }
        StmtKind::NoGc { budget, body } => {
            expr_sites(budget, sites);
            for nested in body {
                stmt_sites(nested, sites);
            }
        }
        StmtKind::Return(None) => {}
    }
}

pub(crate) fn analyze(module: &Module) -> (BTreeMap<String, EffectSummary>, Vec<TypeDiagnostic>) {
    let mut sites = BTreeMap::new();
    let mut summaries = BTreeMap::new();
    for function in &module.functions {
        let mut observations = Vec::new();
        for stmt in &function.body {
            stmt_sites(stmt, &mut observations);
        }
        sites.insert(function.name.clone(), observations);
        let mut summary = EffectSummary {
            effects_explicit: function.effects.is_some(),
            capabilities_explicit: function.capabilities.is_some(),
            ..EffectSummary::default()
        };
        if function.foreign.is_some() {
            summary.effects.insert("Foreign".into());
            summary.capabilities.insert("ForeignCall".into());
        }
        summaries.insert(function.name.clone(), summary);
    }
    let mut diagnostics = Vec::new();
    let limit = module.functions.len().saturating_mul(10).saturating_add(1);
    let mut settled = false;
    for _ in 0..limit {
        let prior = summaries.clone();
        for (name, observations) in &sites {
            let summary = summaries.get_mut(name).expect("declared summary");
            for site in observations {
                if let Some(effect) = site.effect {
                    summary.effects.insert(effect.into());
                }
                if let Some(capability) = site.capability {
                    summary.capabilities.insert(capability.into());
                }
                if let Some(callee) = &site.callee {
                    if let Some(called) = prior.get(callee) {
                        summary.effects.extend(called.effects.iter().cloned());
                        summary
                            .capabilities
                            .extend(called.capabilities.iter().cloned());
                    } else {
                        summary.effects.insert("Unknown".into());
                    }
                }
            }
        }
        if summaries == prior {
            settled = true;
            break;
        }
    }
    if !settled {
        diagnostics.push(TypeDiagnostic::new(
            "CGT015",
            "effect analysis limit exceeded".into(),
            module.span,
        ));
    }
    for function in &module.functions {
        let inferred = &summaries[&function.name];
        if function.foreign.is_none() && inferred.effects.contains("Foreign") {
            if function.effects.is_none() {
                diagnostics.push(TypeDiagnostic::new(
                    "CGT012",
                    "Foreign effect must be explicit".into(),
                    function.span,
                ));
            }
            if function.capabilities.is_none() {
                diagnostics.push(TypeDiagnostic::new(
                    "CGT013",
                    "ForeignCall capability must be explicit".into(),
                    function.span,
                ));
            }
        }
        for site in &sites[&function.name] {
            let (effects, capabilities) = if let Some(callee) = &site.callee {
                if let Some(called) = summaries.get(callee) {
                    (&called.effects, &called.capabilities)
                } else {
                    static UNKNOWN: std::sync::LazyLock<BTreeSet<String>> =
                        std::sync::LazyLock::new(|| BTreeSet::from(["Unknown".into()]));
                    (&*UNKNOWN, &BTreeSet::new())
                }
            } else {
                continue;
            };
            check_site(function, site.span, effects, capabilities, &mut diagnostics);
        }
        for site in &sites[&function.name] {
            let effects = site
                .effect
                .map(|s| BTreeSet::from([s.into()]))
                .unwrap_or_default();
            let capabilities = site
                .capability
                .map(|s| BTreeSet::from([s.into()]))
                .unwrap_or_default();
            check_site(
                function,
                site.span,
                &effects,
                &capabilities,
                &mut diagnostics,
            );
        }
        if function.effects.is_some()
            && inferred.effects.contains("Unknown")
            && sites[&function.name].is_empty()
        {
            diagnostics.push(TypeDiagnostic::new(
                "CGT012",
                "unknown effect in explicit signature".into(),
                function.span,
            ));
        }
    }
    (summaries, diagnostics)
}

fn check_site(
    function: &Function,
    span: Span,
    effects: &BTreeSet<String>,
    capabilities: &BTreeSet<String>,
    diagnostics: &mut Vec<TypeDiagnostic>,
) {
    if let Some(declared) = &function.effects {
        let allowed: BTreeSet<_> = declared.names.iter().map(|n| n.name.as_str()).collect();
        for effect in effects {
            if !allowed.contains(effect.as_str()) || effect == "Unknown" {
                diagnostics.push(TypeDiagnostic::new(
                    "CGT012",
                    format!("effect `{effect}` is not permitted by explicit declaration"),
                    span,
                ));
            }
        }
    }
    if let Some(declared) = &function.capabilities {
        let allowed: BTreeSet<_> = declared.names.iter().map(|n| n.name.as_str()).collect();
        for capability in capabilities {
            if !allowed.contains(capability.as_str()) {
                diagnostics.push(TypeDiagnostic::new(
                    "CGT013",
                    format!("capability `{capability}` is required"),
                    span,
                ));
            }
        }
    }
}

pub(crate) fn contract_calls_are_safe(
    expr: &Expr,
    summaries: &BTreeMap<String, EffectSummary>,
    module: &Module,
) -> bool {
    fn visit(
        name: &str,
        summaries: &BTreeMap<String, EffectSummary>,
        module: &Module,
        active: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
        depth: usize,
    ) -> bool {
        if depth > 256 || active.contains(name) {
            return false;
        }
        if done.contains(name) {
            return true;
        }
        let Some(summary) = summaries.get(name) else {
            return false;
        };
        if !summary.effects.is_empty() {
            return false;
        }
        let Some(function) = module.functions.iter().find(|f| f.name == name) else {
            return false;
        };
        active.insert(name.into());
        let mut next = Vec::new();
        for contract in function.requires.iter().chain(&function.ensures) {
            expr_sites(contract, &mut next);
        }
        for stmt in &function.body {
            stmt_sites(stmt, &mut next);
        }
        for site in next {
            if site.effect.is_some()
                || site.callee.as_deref().is_some_and(|callee| {
                    !visit(callee, summaries, module, active, done, depth + 1)
                })
            {
                return false;
            }
        }
        active.remove(name);
        done.insert(name.into());
        true
    }
    let mut sites = Vec::new();
    expr_sites(expr, &mut sites);
    let mut pending: Vec<String> = Vec::new();
    for site in sites {
        if site.effect.is_some() {
            return false;
        }
        if let Some(callee) = site.callee {
            pending.push(callee);
        }
    }
    let mut active = BTreeSet::new();
    let mut done = BTreeSet::new();
    pending
        .iter()
        .all(|name| visit(name, summaries, module, &mut active, &mut done, 0))
}
