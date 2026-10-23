//! Advisory, source-located call and effect summaries for one CGx2 module.
pub mod package_v1;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write;

use cgx2_sema::check_module;
use cgx2_syntax::{parse_module, Expr, ExprKind, Module, Span, Stmt, StmtKind};

pub const MAX_SOURCE_BYTES: usize = 1_048_576;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub stage: &'static str,
    pub code: String,
    pub message: String,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Effect {
    Collect,
    Foreign,
    ManagedAlloc,
    Mutate,
    ReadArgs,
    Unknown,
}

impl Effect {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Collect => "Collect",
            Self::Foreign => "Foreign",
            Self::ManagedAlloc => "ManagedAlloc",
            Self::Mutate => "Mutate",
            Self::ReadArgs => "ReadArgs",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallKind {
    Local,
    Builtin,
    Foreign,
    Unknown,
}

impl CallKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Builtin => "builtin",
            Self::Foreign => "foreign",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    pub target: Option<String>,
    pub kind: CallKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub ty: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub span: Span,
    pub public: bool,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<String>,
    pub calls: Vec<Call>,
    pub direct_effects: BTreeSet<Effect>,
    pub transitive_effects: BTreeSet<Effect>,
    pub recursive: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Index {
    pub schema_version: u32,
    pub module: Option<String>,
    pub check_mode: &'static str,
    pub functions: Vec<Entry>,
    pub top_level: Entry,
    pub cycles: Vec<Vec<String>>,
}

/// Parse, optionally check, and index a module. `syntax_only` is an explicit
/// fallback for source the current semantic checker cannot accept.
pub fn index_source(source: &str, syntax_only: bool) -> Result<Index, Vec<Diagnostic>> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(vec![Diagnostic {
            stage: "input",
            code: "CGI001".into(),
            message: format!("source exceeds {MAX_SOURCE_BYTES} byte limit"),
            span: Span { start: 0, end: 0 },
        }]);
    }
    let module = parse_module(source).map_err(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|d| Diagnostic {
                stage: "parse",
                code: d.code.into(),
                message: d.message,
                span: d.span,
            })
            .collect::<Vec<_>>()
    })?;
    let typed = if syntax_only {
        None
    } else {
        Some(check_module(&module).map_err(|diagnostics| {
            diagnostics
                .into_iter()
                .map(|d| Diagnostic {
                    stage: "check",
                    code: d.code.into(),
                    message: d.message,
                    span: d.span,
                })
                .collect::<Vec<_>>()
        })?)
    };
    Ok(build_index(&module, typed.as_ref()))
}

fn build_index(module: &Module, typed: Option<&cgx2_sema::TypedModule>) -> Index {
    let names: BTreeSet<String> = module.functions.iter().map(|f| f.name.clone()).collect();
    let mut records: BTreeSet<String> = module
        .records
        .iter()
        .map(|record| record.name.clone())
        .collect();
    if let Some(typed) = typed {
        for (_, ty) in typed.expressions.iter() {
            if let cgx2_sema::Type::Record(name) = ty {
                records.insert(name.clone());
            }
        }
    }
    let imported_roots: BTreeSet<String> = module
        .imports
        .iter()
        .filter_map(|path| path.segments.first().cloned())
        .collect();
    let mut functions = module
        .functions
        .iter()
        .map(|function| {
            let mut visitor =
                Visitor::new(&names, &records, &imported_roots, typed.is_none(), true);
            visitor
                .bindings
                .push(function.parameters.iter().map(|p| p.name.clone()).collect());
            // Contracts see parameters, but never bindings introduced by the body.
            for contract in &function.requires {
                visitor.expr(contract);
            }
            // The implicit result binding exists only in postconditions.
            visitor.bindings.push(BTreeSet::from(["result".into()]));
            for contract in &function.ensures {
                visitor.expr(contract);
            }
            visitor.bindings.pop();
            visitor.statements(&function.body);
            let direct_effects = visitor.effects;
            visitor.calls.sort_by(|a, b| {
                a.span
                    .start
                    .cmp(&b.span.start)
                    .then(a.span.end.cmp(&b.span.end))
                    .then(a.target.cmp(&b.target))
            });
            Entry {
                name: function.name.clone(),
                span: function.span,
                public: function.public,
                parameters: function
                    .parameters
                    .iter()
                    .map(|p| Parameter {
                        name: p.name.clone(),
                        ty: p.ty.to_string(),
                    })
                    .collect(),
                return_type: function
                    .return_type
                    .as_ref()
                    .map(ToString::to_string)
                    .or_else(|| {
                        typed
                            .and_then(|t| t.functions.get(&function.name))
                            .map(|s| s.return_type.to_string())
                    }),
                calls: visitor.calls,
                transitive_effects: direct_effects.clone(),
                direct_effects,
                recursive: false,
            }
        })
        .collect::<Vec<_>>();
    functions.sort_by(|a, b| a.name.cmp(&b.name).then(a.span.start.cmp(&b.span.start)));
    let mut visitor = Visitor::new(&names, &records, &imported_roots, typed.is_none(), false);
    visitor.bindings.push(BTreeSet::new());
    visitor.statements(&module.statements);
    visitor.calls.sort_by(|a, b| {
        a.span
            .start
            .cmp(&b.span.start)
            .then(a.span.end.cmp(&b.span.end))
            .then(a.target.cmp(&b.target))
    });
    let direct_effects = visitor.effects;
    let top_span = module
        .statements
        .first()
        .zip(module.statements.last())
        .map_or(Span { start: 0, end: 0 }, |(first, last)| Span {
            start: first.span.start,
            end: last.span.end,
        });
    let mut top_level = Entry {
        name: "<top-level>".into(),
        span: top_span,
        public: false,
        parameters: Vec::new(),
        return_type: None,
        calls: visitor.calls,
        transitive_effects: direct_effects.clone(),
        direct_effects,
        recursive: false,
    };
    let cycles = propagate_and_cycles(&mut functions, &mut top_level);
    Index {
        schema_version: 1,
        module: module.name.as_ref().map(|name| name.segments.join(".")),
        check_mode: if typed.is_some() {
            "checked"
        } else {
            "syntax_only"
        },
        functions,
        top_level,
        cycles,
    }
}

struct Visitor<'a> {
    names: &'a BTreeSet<String>,
    records: &'a BTreeSet<String>,
    imported_roots: &'a BTreeSet<String>,
    syntax_only: bool,
    function_body: bool,
    bindings: Vec<BTreeSet<String>>,
    calls: Vec<Call>,
    effects: BTreeSet<Effect>,
}

impl<'a> Visitor<'a> {
    fn new(
        names: &'a BTreeSet<String>,
        records: &'a BTreeSet<String>,
        imported_roots: &'a BTreeSet<String>,
        syntax_only: bool,
        function_body: bool,
    ) -> Self {
        Self {
            names,
            records,
            imported_roots,
            syntax_only,
            function_body,
            bindings: Vec::new(),
            calls: Vec::new(),
            effects: BTreeSet::new(),
        }
    }

    fn is_bound(&self, name: &str) -> bool {
        self.bindings.iter().rev().any(|scope| scope.contains(name))
    }

    fn statements(&mut self, statements: &[Stmt]) {
        for statement in statements {
            match &statement.kind {
                StmtKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expr(condition);
                    for body in [then_body, else_body] {
                        self.bindings.push(BTreeSet::new());
                        self.statements(body);
                        self.bindings.pop();
                    }
                }
                StmtKind::NoGc { budget, body } => {
                    self.expr(budget);
                    self.bindings.push(BTreeSet::new());
                    self.statements(body);
                    self.bindings.pop();
                }
                StmtKind::Let { name, value, .. } => {
                    self.expr(value);
                    if let Some(scope) = self.bindings.last_mut() {
                        scope.insert(name.clone());
                    }
                }
                StmtKind::Assign { value, .. } => {
                    self.effects.insert(Effect::Mutate);
                    self.expr(value);
                }
                StmtKind::Return(Some(expr)) | StmtKind::Expr(expr) => self.expr(expr),
                StmtKind::Return(None) => {}
                StmtKind::For {
                    binding,
                    iter,
                    body,
                } => {
                    self.expr(iter);
                    self.bindings.push(BTreeSet::from([binding.clone()]));
                    self.statements(body);
                    self.bindings.pop();
                }
            }
        }
    }

    fn expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Name(path) if self.syntax_only => {
                let name = path.segments.join(".");
                if !self.is_bound(&name) {
                    self.effects.insert(Effect::Unknown);
                }
            }
            ExprKind::Binary { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            ExprKind::Call { callee, arguments } => {
                if !matches!(callee.kind, ExprKind::Name(_)) {
                    self.expr(callee);
                }
                let target = match &callee.kind {
                    ExprKind::Name(path) => Some(path.segments.join(".")),
                    _ => None,
                };
                for arg in arguments {
                    if target.as_deref() == Some("memory.region")
                        && arg.name.as_deref() == Some("mode")
                        && matches!(&arg.value.kind, ExprKind::Name(path) if path.segments.as_slice() == ["auto"] || path.segments.as_slice() == ["manual"])
                    {
                        continue;
                    }
                    self.expr(&arg.value);
                }
                let kind = if let Some(name) = target.as_deref() {
                    self.classify(name)
                } else {
                    self.effects.insert(Effect::Unknown);
                    CallKind::Unknown
                };
                self.calls.push(Call {
                    target,
                    kind,
                    span: expr.span,
                });
            }
            _ => {}
        }
    }

    fn classify(&mut self, name: &str) -> CallKind {
        let root = name.split('.').next().unwrap_or(name);
        if !self.is_bound(root) && self.records.contains(name) {
            return CallKind::Builtin;
        }
        if self.is_bound(root) || self.imported_roots.contains(root) {
            self.effects.insert(Effect::Unknown);
            return CallKind::Unknown;
        }
        if !name.contains('.') && self.names.contains(name) {
            return CallKind::Local;
        }
        if !name.contains('.') && self.records.contains(name) {
            return CallKind::Builtin;
        }
        if self.syntax_only && self.function_body && name.starts_with("memory.") {
            // The frozen checker models memory operations in scripts only.
            self.effects.insert(Effect::Unknown);
        }
        match name {
            "arg" | "arg_count" => {
                self.effects.insert(Effect::ReadArgs);
                CallKind::Builtin
            }
            "memory.region" | "memory.owned_bytes" | "memory.alloc_bytes" => {
                self.effects.insert(Effect::ManagedAlloc);
                CallKind::Builtin
            }
            "memory.promote_owned" => {
                self.effects.insert(Effect::ManagedAlloc);
                self.effects.insert(Effect::Mutate);
                CallKind::Builtin
            }
            "memory.copy_to_owned" => {
                self.effects.insert(Effect::ManagedAlloc);
                CallKind::Builtin
            }
            "memory.collect" => {
                self.effects.insert(Effect::Collect);
                CallKind::Builtin
            }
            "memory.move" | "memory.borrow_mut" | "memory.end_borrow" | "memory.drop"
            | "memory.close" => {
                self.effects.insert(Effect::Mutate);
                CallKind::Builtin
            }
            "memory.borrow" | "memory.use" => CallKind::Builtin,
            "ffi.call" | "python.call" | "python.import" | "torch.call" => {
                self.effects.insert(Effect::Foreign);
                self.effects.insert(Effect::Unknown);
                CallKind::Foreign
            }
            _ => {
                self.effects.insert(Effect::Unknown);
                CallKind::Unknown
            }
        }
    }
}

fn propagate_and_cycles(functions: &mut [Entry], top_level: &mut Entry) -> Vec<Vec<String>> {
    let n = functions.len();
    let positions: BTreeMap<String, usize> = functions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.clone(), i))
        .collect();
    let mut graph = vec![Vec::new(); n];
    let mut reverse = vec![Vec::new(); n + 1];
    for (caller, entry) in functions
        .iter()
        .chain(std::iter::once(&*top_level))
        .enumerate()
    {
        for call in &entry.calls {
            if call.kind == CallKind::Local {
                if let Some(&callee) = call.target.as_ref().and_then(|name| positions.get(name)) {
                    reverse[callee].push(caller);
                    if caller < n {
                        graph[caller].push(callee);
                    }
                }
            }
        }
    }
    for edges in &mut graph {
        edges.sort_unstable();
        edges.dedup();
    }
    let cycles = find_cycles(&graph, functions);
    let mut effects = functions
        .iter()
        .map(|f| f.direct_effects.clone())
        .chain(std::iter::once(top_level.direct_effects.clone()))
        .collect::<Vec<_>>();
    let mut queue: VecDeque<usize> = (0..=n).collect();
    let mut queued = vec![true; n + 1];
    while let Some(callee) = queue.pop_front() {
        queued[callee] = false;
        let inherited = effects[callee].clone();
        for &caller in &reverse[callee] {
            let old_len = effects[caller].len();
            effects[caller].extend(&inherited);
            if effects[caller].len() != old_len && !queued[caller] {
                queue.push_back(caller);
                queued[caller] = true;
            }
        }
    }
    for (entry, inherited) in functions.iter_mut().zip(effects.iter()) {
        entry.transitive_effects = inherited.clone();
    }
    top_level.transitive_effects = effects[n].clone();
    cycles
}

fn find_cycles(graph: &[Vec<usize>], functions: &mut [Entry]) -> Vec<Vec<String>> {
    let n = graph.len();
    let mut seen = vec![false; n];
    let mut order = Vec::with_capacity(n);
    for root in 0..n {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![(root, 0)];
        while let Some((node, next)) = stack.last_mut() {
            if *next < graph[*node].len() {
                let child = graph[*node][*next];
                *next += 1;
                if !seen[child] {
                    seen[child] = true;
                    stack.push((child, 0));
                }
            } else {
                order.push(*node);
                stack.pop();
            }
        }
    }
    let mut reversed = vec![Vec::new(); n];
    for (from, edges) in graph.iter().enumerate() {
        for &to in edges {
            reversed[to].push(from);
        }
    }
    seen.fill(false);
    let mut cycles = Vec::new();
    for &root in order.iter().rev() {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![root];
        let mut component = Vec::new();
        while let Some(node) = stack.pop() {
            component.push(node);
            for &child in &reversed[node] {
                if !seen[child] {
                    seen[child] = true;
                    stack.push(child);
                }
            }
        }
        if component.len() > 1 || graph[root].contains(&root) {
            let mut names = Vec::new();
            for node in component {
                functions[node].recursive = true;
                names.push(functions[node].name.clone());
            }
            names.sort();
            cycles.push(names);
        }
    }
    cycles.sort();
    cycles
}

fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch <= '\u{001f}' => {
                write!(out, "\\u{:04x}", ch as u32).expect("write to string");
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn span_json(span: Span) -> String {
    format!("{{\"start\":{},\"end\":{}}}", span.start, span.end)
}

fn strings_json<'a>(values: impl IntoIterator<Item = &'a str>) -> String {
    format!(
        "[{}]",
        values
            .into_iter()
            .map(json_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn entry_json(entry: &Entry) -> String {
    let parameters = entry
        .parameters
        .iter()
        .map(|p| {
            format!(
                "{{\"name\":{},\"type\":{}}}",
                json_string(&p.name),
                json_string(&p.ty)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let calls = entry
        .calls
        .iter()
        .map(|c| {
            format!(
                "{{\"target\":{},\"kind\":{},\"span\":{}}}",
                c.target.as_deref().map_or("null".to_owned(), json_string),
                json_string(c.kind.as_str()),
                span_json(c.span)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{\"name\":{},\"span\":{},\"public\":{},\"parameters\":[{}],\"return_type\":{},\"calls\":[{}],\"direct_effects\":{},\"transitive_effects\":{},\"recursive\":{}}}",
        json_string(&entry.name), span_json(entry.span), entry.public, parameters,
        entry.return_type.as_deref().map_or("null".to_owned(), json_string), calls,
        strings_json(entry.direct_effects.iter().map(|e| e.as_str())),
        strings_json(entry.transitive_effects.iter().map(|e| e.as_str())), entry.recursive)
}

impl Index {
    pub fn to_json(&self) -> String {
        let functions = self
            .functions
            .iter()
            .map(entry_json)
            .collect::<Vec<_>>()
            .join(",");
        let cycles = self
            .cycles
            .iter()
            .map(|cycle| strings_json(cycle.iter().map(String::as_str)))
            .collect::<Vec<_>>()
            .join(",");
        format!("{{\"schema_version\":{},\"span_unit\":\"utf8_byte_offset\",\"module\":{},\"check_mode\":{},\"functions\":[{}],\"top_level\":{},\"cycles\":[{}]}}",
            self.schema_version, self.module.as_deref().map_or("null".to_owned(), json_string),
            json_string(self.check_mode), functions, entry_json(&self.top_level), cycles)
    }

    pub fn to_text(&self) -> String {
        let mut out = format!(
            "module {} [{}; spans: zero-based half-open UTF-8 bytes]\n",
            self.module.as_deref().unwrap_or("<script>"),
            self.check_mode
        );
        for entry in self
            .functions
            .iter()
            .chain(std::iter::once(&self.top_level))
        {
            writeln!(
                out,
                "{} @{}..{}{}",
                entry.name,
                entry.span.start,
                entry.span.end,
                if entry.recursive { " (recursive)" } else { "" }
            )
            .expect("write to string");
            for call in &entry.calls {
                writeln!(
                    out,
                    "  call {} [{}] @{}..{}",
                    call.target.as_deref().unwrap_or("<dynamic>"),
                    call.kind.as_str(),
                    call.span.start,
                    call.span.end
                )
                .expect("write to string");
            }
            writeln!(out, "  direct: {}", effects_text(&entry.direct_effects))
                .expect("write to string");
            writeln!(
                out,
                "  transitive: {}",
                effects_text(&entry.transitive_effects)
            )
            .expect("write to string");
        }
        for cycle in &self.cycles {
            writeln!(out, "cycle: {}", cycle.join(", ")).expect("write to string");
        }
        out
    }
}

fn effects_text(effects: &BTreeSet<Effect>) -> String {
    if effects.is_empty() {
        "(none)".into()
    } else {
        effects
            .iter()
            .map(|e| e.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
mod imported_record_tests {
    use super::*;
    #[test]
    fn imported_constructor_keeps_argument_call_dependencies() {
        let definitions =
            parse_module("module demo.types\npub record Point:\n    x: i64\n").unwrap();
        let mut imported = cgx2_sema::ImportedSignatures::default();
        imported
            .modules
            .extend(["demo.types".into(), "demo.main".into()]);
        let mut record = definitions.records[0].clone();
        record.name = "demo.types.Point".into();
        imported.public_records.insert(record.name.clone(), record);
        let module = parse_module("module demo.main\nimport demo.types\nfn main() -> demo.types.Point:\n    demo.types.Point(x: arg_count())\n").unwrap();
        let typed = cgx2_sema::check_module_with_imports(&module, &imported).unwrap();
        let index = build_index(&module, Some(&typed));
        let function = &index.functions[0];
        assert!(function.direct_effects.contains(&Effect::ReadArgs));
        assert!(!function.direct_effects.contains(&Effect::Unknown));
        assert!(function
            .calls
            .iter()
            .any(|call| call.target.as_deref() == Some("demo.types.Point")
                && call.kind == CallKind::Builtin));
        assert!(function
            .calls
            .iter()
            .any(|call| call.target.as_deref() == Some("arg_count")));
    }
}
