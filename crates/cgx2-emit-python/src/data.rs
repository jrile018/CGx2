//! Portable copied-value target. All source legality precedes lowering.
use crate::{names::python_name, EmitDiagnostic};
use cgx2_sema::{Type, TypedModule};
use cgx2_syntax::{AssignOp, BinaryOp, Expr, ExprKind, Module, Pattern, Span, Stmt, StmtKind};
use std::collections::BTreeMap;
fn quote(s: &str) -> String {
    crate::emit::quote_python(s)
}
fn errors(es: Vec<cgx2_sema::TypeDiagnostic>) -> Vec<EmitDiagnostic> {
    es.into_iter()
        .map(|d| EmitDiagnostic::new(d.code, d.message, d.span))
        .collect()
}
pub(crate) fn validate_strings(module: &Module) -> Result<(), Vec<EmitDiagnostic>> {
    // The finite target bypasses scalar admission. Never let Python interpret
    // raw CGx2 literal text using its larger set of escape sequences.
    let mut literal_errors = Vec::new();
    let mut literal = |expr: &Expr| {
        if matches!(&expr.kind, ExprKind::StringLiteral(raw) if crate::emit::decode_string(raw).is_none())
        {
            literal_errors.push(EmitDiagnostic::new(
                "CGE003",
                "string literal contains unsupported escape",
                expr.span,
            ));
        }
    };
    cgx2_syntax::visit_stmts(&module.statements, &mut |_| {}, &mut literal);
    for function in &module.functions {
        cgx2_syntax::visit_stmts(&function.body, &mut |_| {}, &mut literal);
        for expr in function.requires.iter().chain(&function.ensures) {
            cgx2_syntax::visit_expr(expr, &mut literal);
        }
    }
    if !literal_errors.is_empty() {
        return Err(literal_errors);
    }
    Ok(())
}

pub(crate) fn emit(module: &Module, typed: &TypedModule) -> Result<String, Vec<EmitDiagnostic>> {
    let checked = cgx2_sema::check_module(module).map_err(errors)?;
    if &checked != typed {
        return Err(vec![EmitDiagnostic::new(
            "CGE001",
            "typed module does not match source",
            module.span,
        )]);
    }
    validate_strings(module)?;
    if let Some(f) = module.functions.iter().find(|f| f.foreign.is_some()) {
        return Err(vec![EmitDiagnostic::new(
            "CGE006",
            "portable value target does not admit foreign declarations",
            f.span,
        )]);
    }
    let mut rejected = None;
    cgx2_syntax::visit_stmts(
        &module.statements,
        &mut |s| {
            if matches!(
                s.kind,
                StmtKind::NoGc { .. } | StmtKind::LoopContract { .. }
            ) {
                rejected = Some(s.span)
            }
        },
        &mut |_| {},
    );
    for f in &module.functions {
        cgx2_syntax::visit_stmts(
            &f.body,
            &mut |s| {
                if matches!(
                    s.kind,
                    StmtKind::NoGc { .. } | StmtKind::LoopContract { .. }
                ) {
                    rejected = Some(s.span)
                }
            },
            &mut |_| {},
        );
    }
    if let Some(s) = rejected {
        return Err(vec![EmitDiagnostic::new(
            "CGE006",
            "portable value target does not admit memory or loop contracts",
            s,
        )]);
    }
    let module = cgx2_sema::lower_module(module, typed).map_err(errors)?;
    let typed = cgx2_sema::check_module(&module).map_err(errors)?;
    let mut emitter = Emitter {
        module: &module,
        typed: &typed,
        owner: String::new(),
        scopes: vec![BTreeMap::new()],
        next: 0,
        out: include_str!("data_runtime.py.txt").into(),
    };
    for f in &module.functions {
        emitter.owner = f.name.clone();
        emitter.scopes = vec![BTreeMap::new()];
        let params = f
            .parameters
            .iter()
            .map(|p| emitter.bind(&p.name))
            .collect::<Vec<_>>()
            .join(", ");
        emitter.line(0, &format!("def {}({params}):", python_name(&f.name)));
        emitter.line(1, "_cgx2_enter()");
        emitter.line(1, "try:");
        for e in &f.requires {
            let v = emitter.expr(e);
            emitter.line(
                2,
                &format!(
                    "_cgx2_contract({v}, 'CGR015', {}, {})",
                    e.span.start, e.span.end
                ),
            );
        }
        let contracted = !f.ensures.is_empty();
        if contracted {
            emitter.line(2, "def _cgx2_body():");
            emitter.block(&f.body, 3, true);
            let result = emitter.bind("result");
            emitter.line(2, &format!("{result} = _cgx2_body()"));
            for e in &f.ensures {
                let v = emitter.expr(e);
                emitter.line(
                    2,
                    &format!(
                        "_cgx2_contract({v}, 'CGR016', {}, {})",
                        e.span.start, e.span.end
                    ),
                );
            }
            emitter.line(2, &format!("return {result}"));
        } else {
            emitter.block(&f.body, 2, true);
        }
        emitter.line(1, "finally:");
        emitter.line(2, "_cgx2_leave()");
        emitter.line(0, "");
    }
    emitter.owner = String::new();
    emitter.scopes = vec![BTreeMap::new()];
    if module.functions.is_empty() {
        emitter.line(0, "def _cgx2_script():");
        emitter.block(&module.statements, 1, true);
    } else if typed
        .functions
        .get("main")
        .is_none_or(|s| !s.parameters.is_empty())
    {
        return Err(vec![EmitDiagnostic::new(
            "CGE001",
            "entry requires zero-argument main",
            module
                .functions
                .iter()
                .find(|f| f.name == "main")
                .and_then(|f| f.parameters.first())
                .map_or(module.span, |p| p.ty.span),
        )]);
    }
    let call = if module.functions.is_empty() {
        "_cgx2_script".into()
    } else {
        python_name("main")
    };
    emitter.line(0, "if __name__ == '__main__':");
    emitter.line(1, "try:");
    emitter.line(2, &format!("_cgx2_output({call}())"));
    emitter.line(1, "except Exception as _cgx2_error:");
    emitter.line(2, "_cgx2_sys.stderr.write(str(_cgx2_error) + '\\n')");
    emitter.line(2, "raise SystemExit(1)");
    Ok(emitter.out)
}
struct Emitter<'a> {
    module: &'a Module,
    typed: &'a TypedModule,
    owner: String,
    scopes: Vec<BTreeMap<String, String>>,
    next: usize,
    out: String,
}
impl Emitter<'_> {
    fn line(&mut self, n: usize, s: &str) {
        self.out.push_str(&"    ".repeat(n));
        self.out.push_str(s);
        self.out.push('\n');
    }
    fn fresh(&mut self) -> String {
        let n = self.next;
        self.next += 1;
        format!("_cgx2_local_{n}")
    }
    fn bind(&mut self, n: &str) -> String {
        let v = self.fresh();
        self.scopes.last_mut().unwrap().insert(n.into(), v.clone());
        v
    }
    fn lookup(&self, n: &str) -> String {
        self.scopes
            .iter()
            .rev()
            .find_map(|s| s.get(n).cloned())
            .unwrap_or_else(|| python_name(n))
    }
    fn ty(&self, s: Span) -> Option<&Type> {
        self.typed
            .bodies
            .get(&self.owner)
            .and_then(|b| b.get(s))
            .or_else(|| self.typed.expressions.get(s))
    }
    fn expr(&mut self, e: &Expr) -> String {
        let value = match &e.kind {
            ExprKind::Integer(s) => s.replace('_', ""),
            ExprKind::Boolean(b) => {
                if *b {
                    "True".into()
                } else {
                    "False".into()
                }
            }
            ExprKind::StringLiteral(s) => {
                quote(&crate::emit::decode_string(s).expect("validated finite string"))
            }
            ExprKind::Name(p) => {
                if p.segments.len() == 1 {
                    self.lookup(&p.segments[0])
                } else {
                    format!(
                        "_cgx2_field({}, {})",
                        self.lookup(&p.segments[0]),
                        quote(&p.segments[1])
                    )
                }
            }
            ExprKind::Binary { left, op, right } => {
                let a = self.expr(left);
                let b = self.expr(right);
                format!(
                    "_cgx2_math({}, {a}, {b}, {}, {})",
                    quote(match op {
                        BinaryOp::Add => "add",
                        BinaryOp::Subtract => "sub",
                        BinaryOp::Multiply => "mul",
                        BinaryOp::Divide => "div",
                    }),
                    e.span.start,
                    e.span.end
                )
            }
            ExprKind::Constructor {
                case, arguments, ..
            } => {
                let tag = self.ty(e.span).unwrap().to_string();
                let values = arguments
                    .iter()
                    .map(|a| self.expr(&a.value))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "_cgx2_Variant({}, {}, [{values}])",
                    quote(&tag),
                    quote(case)
                )
            }
            ExprKind::Collection(items) => {
                let array = matches!(self.ty(e.span), Some(Type::Array(_)));
                let values = items
                    .iter()
                    .map(|v| self.expr(v))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{}([{values}])",
                    if array { "_cgx2_Array" } else { "_cgx2_List" }
                )
            }
            ExprKind::Index { collection, index } => {
                let tag = self.ty(e.span).unwrap().to_string();
                let a = self.expr(collection);
                let b = self.expr(index);
                format!("_cgx2_get({a}, {b}, {})", quote(&tag))
            }
            ExprKind::Call { callee, arguments } => {
                let ExprKind::Name(p) = &callee.kind else {
                    unreachable!("checked callee")
                };
                let name = p.segments.join(".");
                if arguments.iter().any(|a| a.name.is_some()) {
                    let ty = self.ty(e.span).unwrap().to_string();
                    let fields = self
                        .module
                        .records
                        .iter()
                        .find(|r| r.name == ty)
                        .unwrap()
                        .fields
                        .iter()
                        .map(|f| quote(&f.name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let args = arguments
                        .iter()
                        .map(|a| {
                            format!(
                                "({}, {})",
                                quote(a.name.as_ref().unwrap()),
                                self.expr(&a.value)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("_cgx2_record({}, [{fields}], [{args}])", quote(&ty))
                } else {
                    let args = arguments
                        .iter()
                        .map(|a| self.expr(&a.value))
                        .collect::<Vec<_>>()
                        .join(", ");
                    if name.starts_with("$cgx2_cmp_") {
                        let op = match name.as_str() {
                            "$cgx2_cmp_eq" => "eq",
                            "$cgx2_cmp_ne" => "ne",
                            "$cgx2_cmp_lt" => "lt",
                            "$cgx2_cmp_le" => "le",
                            "$cgx2_cmp_gt" => "gt",
                            _ => "ge",
                        };
                        format!("_cgx2_compare({}, {args})", quote(op))
                    } else if !self.module.functions.iter().any(|f| f.name == name)
                        && matches!(
                            name.as_str(),
                            "length"
                                | "get"
                                | "updated"
                                | "append"
                                | "array"
                                | "list"
                                | "concat"
                                | "slice"
                                | "parse_i64"
                                | "show"
                        )
                    {
                        let tag = self.ty(e.span).unwrap().to_string();
                        format!("_cgx2_data({}, {}, [{args}])", quote(&name), quote(&tag))
                    } else if name == "arg_count" {
                        "len(_cgx2_sys.argv) - 1".into()
                    } else if name == "arg" {
                        format!("_cgx2_arg({args})")
                    } else {
                        format!("{}({args})", python_name(&name))
                    }
                }
            }
        };
        format!(
            "_cgx2_expr(lambda: {value}, {}, {})",
            e.span.start, e.span.end
        )
    }
    fn block(&mut self, body: &[Stmt], indent: usize, tail: bool) {
        if body.is_empty() {
            self.line(indent, if tail { "return None" } else { "pass" });
            return;
        }
        for (i, s) in body.iter().enumerate() {
            self.line(
                indent,
                &format!("_cgx2_step({}, {})", s.span.start, s.span.end),
            );
            let last = i + 1 == body.len();
            match &s.kind {
                StmtKind::Let { name, value, .. } => {
                    let v = self.expr(value);
                    let local = self.bind(name);
                    self.line(indent, &format!("{local} = {v}"));
                }
                StmtKind::Assign { target, op, value } => {
                    let v = self.expr(value);
                    let local = self.lookup(&target.segments[0]);
                    self.line(
                        indent,
                        &if *op == AssignOp::Set {
                            format!("{local} = {v}")
                        } else {
                            format!(
                                "{local} = _cgx2_math('add', {local}, {v}, {}, {})",
                                s.span.start, s.span.end
                            )
                        },
                    );
                }
                StmtKind::Expr(e) => {
                    let v = self.expr(e);
                    self.line(
                        indent,
                        &format!("{}{v}", if tail && last { "return " } else { "" }),
                    );
                }
                StmtKind::Return(v) => {
                    let v = v.as_ref().map(|e| self.expr(e)).unwrap_or("None".into());
                    self.line(indent, &format!("return {v}"));
                }
                StmtKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    let v = self.expr(condition);
                    self.line(indent, &format!("if {v}:"));
                    self.scopes.push(BTreeMap::new());
                    self.block(then_body, indent + 1, false);
                    self.scopes.pop();
                    self.line(indent, "else:");
                    self.scopes.push(BTreeMap::new());
                    self.block(else_body, indent + 1, false);
                    self.scopes.pop();
                }
                StmtKind::Match { scrutinee, arms } => {
                    let v = self.expr(scrutinee);
                    let local = self.fresh();
                    self.line(indent, &format!("{local} = {v}"));
                    for (j, a) in arms.iter().enumerate() {
                        let prefix = if j == 0 { "if" } else { "elif" };
                        let test = match &a.pattern {
                            Pattern::Wildcard(_) => "True".into(),
                            Pattern::Case { case, .. } => {
                                format!("{local}.case == {}", quote(case))
                            }
                        };
                        self.line(indent, &format!("{prefix} {test}:"));
                        self.scopes.push(BTreeMap::new());
                        if let Pattern::Case { bindings, .. } = &a.pattern {
                            for (k, b) in bindings.iter().enumerate() {
                                if b.name != "_" {
                                    let n = self.bind(&b.name);
                                    self.line(indent + 1, &format!("{n} = {local}.payloads[{k}]"));
                                }
                            }
                        }
                        self.block(&a.body, indent + 1, tail && last);
                        self.scopes.pop();
                    }
                }
                StmtKind::For {
                    binding,
                    iter,
                    body,
                } => {
                    let v = if let ExprKind::Call { callee, arguments } = &iter.kind {
                        if matches!(&callee.kind,ExprKind::Name(p) if p.segments.as_slice()==["range"])
                        {
                            let args = arguments
                                .iter()
                                .map(|a| self.expr(&a.value))
                                .collect::<Vec<_>>()
                                .join(", ");
                            format!(
                                "_cgx2_range(lambda: range({args}), {}, {})",
                                iter.span.start, iter.span.end
                            )
                        } else {
                            self.expr(iter)
                        }
                    } else {
                        self.expr(iter)
                    };
                    self.scopes.push(BTreeMap::new());
                    let n = self.bind(binding);
                    self.line(indent, &format!("for {n} in {v}:"));
                    self.line(
                        indent + 1,
                        &format!("_cgx2_step({}, {})", s.span.start, s.span.end),
                    );
                    self.scopes.push(BTreeMap::new());
                    self.block(body, indent + 1, false);
                    self.scopes.pop();
                    self.scopes.pop();
                }
                StmtKind::NoGc { .. } | StmtKind::LoopContract { .. } => {
                    unreachable!("profile gate")
                }
            }
        }
        if tail
            && !matches!(
                body.last().map(|s| &s.kind),
                Some(StmtKind::Expr(_) | StmtKind::Return(_) | StmtKind::Match { .. })
            )
        {
            self.line(indent, "return None");
        }
    }
}
