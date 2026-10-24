//! Concrete finite-value emission after checked symbolic templates specialize.
use super::*;

fn failure(errors: Vec<cgx2_sema::TypeDiagnostic>) -> Vec<EmitDiagnostic> {
    errors
        .into_iter()
        .map(|d| EmitDiagnostic::new(d.code, d.message, d.span))
        .collect()
}
pub(crate) fn validate_strings(module: &Module) -> Result<(), Vec<EmitDiagnostic>> {
    // Finite lowering bypasses the scalar validator. Check the complete source
    // before specialization, including literals in generic contracts and data.
    let mut errors = Vec::new();
    let mut literal = |expr: &Expr| {
        if matches!(&expr.kind, ExprKind::StringLiteral(raw) if decode_string(raw).is_none()) {
            errors.push(EmitDiagnostic::new(
                "CGRS003",
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
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(())
}

pub(super) fn package(package: &CheckedPackage) -> Result<String, Vec<PackageEmitDiagnostic>> {
    package.verify_unchanged().map_err(|ds| {
        ds.into_iter()
            .map(|d| PackageEmitDiagnostic {
                file: d.file,
                diagnostic: EmitDiagnostic::new(
                    d.code,
                    d.message,
                    d.span.unwrap_or(Span { start: 0, end: 0 }),
                ),
            })
            .collect::<Vec<_>>()
    })?;
    for source in package.modules() {
        validate_strings(source.syntax()).map_err(|errors| {
            errors
                .into_iter()
                .map(|diagnostic| package_error(source, diagnostic))
                .collect::<Vec<_>>()
        })?;
        for f in &source.syntax().functions {
            if let Some(binding) = &f.foreign {
                return Err(vec![package_error(
                    source,
                    EmitDiagnostic::new(
                        "CGRS001",
                        "foreign declarations are unsupported by the finite native value target",
                        binding.target.span,
                    ),
                )]);
            }
        }
    }
    let entry = package
        .modules()
        .iter()
        .find(|m| m.source() == package.index().entry)
        .expect("checked entry");
    let name = entry
        .syntax()
        .name
        .as_ref()
        .map(|p| p.segments.join("."))
        .unwrap_or_default();
    let merged = cgx2_sema::merge_modules(
        &package
            .modules()
            .iter()
            .map(|m| m.syntax())
            .collect::<Vec<_>>(),
        &name,
    );
    let typed = cgx2_sema::check_module(&merged).map_err(|ds| {
        failure(ds)
            .into_iter()
            .map(|diagnostic| PackageEmitDiagnostic {
                file: entry.source().to_path_buf(),
                diagnostic,
            })
            .collect::<Vec<_>>()
    })?;
    emit(&merged, &typed).map_err(|ds| {
        ds.into_iter()
            .map(|diagnostic| PackageEmitDiagnostic {
                file: entry.source().to_path_buf(),
                diagnostic,
            })
            .collect()
    })
}
pub(super) fn emit(module: &Module, typed: &TypedModule) -> Result<String, Vec<EmitDiagnostic>> {
    let original = cgx2_sema::check_module(module).map_err(failure)?;
    if &original != typed {
        return Err(vec![EmitDiagnostic::new(
            "CGRS001",
            "typed module does not match source",
            module.span,
        )]);
    }
    validate_strings(module)?;
    for f in &module.functions {
        if f.foreign.is_some() || f.body.iter().any(memory_stmt) {
            return Err(vec![EmitDiagnostic::new(
                "CGRS001",
                "finite data target does not admit foreign or memory operations",
                f.span,
            )]);
        }
    }
    if module.statements.iter().any(memory_stmt) {
        return Err(vec![EmitDiagnostic::new(
            "CGRS001",
            "finite data target does not admit memory operations",
            module.span,
        )]);
    }
    let module = cgx2_sema::lower_module(module, typed).map_err(failure)?;
    let typed = cgx2_sema::check_module(&module).map_err(failure)?;
    let mut out = String::from(PRELUDE);
    out.push_str(include_str!("data_runtime.rs.txt"));
    emit_variants(&[(&module, &typed)], &mut out);
    emit_records(&module.records, &mut out);
    for f in &module.functions {
        let signature = &typed.functions[&f.name];
        let params = f
            .parameters
            .iter()
            .zip(&signature.parameters)
            .map(|(p, t)| format!(", {}: {}", rust_name(&p.name), rust_type(t)))
            .collect::<String>();
        out.push_str(&format!("fn {}(ctx:&mut Cgx2Context,args:&[String]{params})->Result<{},String>{{\nctx.enter()?;\nlet value=(||->Result<_,String>{{\n",rust_name(&f.name),rust_type(&signature.return_type)));
        let mut view = typed.clone();
        if let Some(body) = typed.bodies.get(&f.name) {
            view.expressions = body.clone();
        }
        emit_function_body(f, 1, &mut out, None, &view);
        out.push_str("})();\nctx.leave();\nvalue\n}\n");
    }
    let result = if module.functions.is_empty() {
        let t = tail_type(&module.statements, &typed);
        out.push_str(&format!(
            "fn cgx2_script(ctx:&mut Cgx2Context,args:&[String])->Result<{},String>{{\n",
            rust_type(&t)
        ));
        let mut view = typed.clone();
        if let Some(body) = typed.bodies.get("") {
            view.expressions = body.clone();
        }
        emit_block(&module.statements, 1, &mut out, &view);
        out.push_str("}\n");
        "cgx2_script(&mut ctx,&args)".to_owned()
    } else {
        let Some(main) = typed.functions.get("main") else {
            return Err(vec![EmitDiagnostic::new(
                "CGRS001",
                "function module requires main",
                module.span,
            )]);
        };
        if !main.parameters.is_empty() {
            return Err(vec![EmitDiagnostic::new(
                "CGRS001",
                "main requires zero parameters",
                main.span,
            )]);
        }
        format!("{}(&mut ctx,&args)", rust_name("main"))
    };
    let print_value = if module.functions.is_empty() {
        tail_type(&module.statements, &typed) != Type::Unit
    } else {
        typed.functions["main"].return_type != Type::Unit
    };
    out.push_str(&format!("fn main(){{let args=std::env::args().skip(1).collect::<Vec<_>>();let mut ctx=Cgx2Context::default();match {result}{{Ok(value)=>{{let shown=cgx2_show(&value);if shown.len()>1048576{{eprintln!(\"CGR010: output limit exceeded\");std::process::exit(1);}}if {print_value}{{println!(\"{{shown}}\");}}}},Err(e)=>{{eprintln!(\"{{e}}\");std::process::exit(1);}}}}}}\n"));
    Ok(out)
}
fn memory_stmt(s: &Stmt) -> bool {
    match &s.kind {
        StmtKind::NoGc { .. } | StmtKind::LoopContract { .. } => true,
        StmtKind::If {
            then_body,
            else_body,
            ..
        } => then_body.iter().chain(else_body).any(memory_stmt),
        StmtKind::Match { arms, .. } => arms.iter().any(|a| a.body.iter().any(memory_stmt)),
        StmtKind::For { body, .. } => body.iter().any(memory_stmt),
        _ => false,
    }
}
fn option(value: &str, ty: &Type) -> String {
    format!(
        "match {value}{{Some(v)=>{}::{}(v),None=>{}::{}}}",
        rust_type(ty),
        rust_name("Some"),
        rust_type(ty),
        rust_name("None")
    )
}
pub(super) fn expression(e: &Expr, module: Option<&str>, typed: &TypedModule) -> Option<String> {
    let t = typed.expressions.get(e.span);
    match &e.kind {
        ExprKind::Collection(v) => {
            let mut prefix = String::new();
            for (i, e) in v.iter().enumerate() {
                prefix.push_str(&format!(
                    "let v{i}={}?;",
                    emit_expr_context(e, module, typed)
                ));
            }
            let wrapper = if matches!(t, Some(Type::Array(_))) {
                "Cgx2Array"
            } else {
                "Cgx2List"
            };
            Some(format!(
                "{{{prefix}Ok::<_,String>({wrapper}(vec![{}]))}}",
                (0..v.len())
                    .map(|i| format!("v{i}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ))
        }
        ExprKind::Index { collection, index } => {
            let ty = t?;
            Some(format!(
                "{{let xs={}?;let i={}?;Ok::<_,String>({})}}",
                emit_expr_context(collection, module, typed),
                emit_expr_context(index, module, typed),
                option("cgx2_get(&xs,i)", ty)
            ))
        }
        ExprKind::Call { callee, arguments } => {
            let ExprKind::Name(p) = &callee.kind else {
                return None;
            };
            let n = p.segments.join(".");
            if typed.functions.contains_key(&n)
                // Lowering has replaced constructor paths and generic records
                // with their checked concrete nominal identity. Let record
                // initialization handle these before spelling-based intrinsics.
                || matches!(t, Some(Type::Record(name)) if name == &n)
                || !matches!(
                    n.as_str(),
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
                return None;
            }
            let mut prefix = String::new();
            for (i, a) in arguments.iter().enumerate() {
                prefix.push_str(&format!(
                    "let v{i}={}?;",
                    emit_expr_context(&a.value, module, typed)
                ));
            }
            let value = match n.as_str() {
                "length" => "cgx2_length(&v0)".into(),
                "show" => "cgx2_checked_string(cgx2_show(&v0))".into(),
                "get" => format!("Ok::<_,String>({})", option("cgx2_get(&v0,v1)", t?)),
                "updated" => format!("Ok::<_,String>({})", option("cgx2_updated(v0,v1,v2)", t?)),
                "append" => "cgx2_append(v0,v1)".into(),
                "array" => "Ok::<_,String>(Cgx2Array(v0.0))".into(),
                "list" => "Ok::<_,String>(Cgx2List(v0.0))".into(),
                "concat" => "cgx2_concat(v0,v1)".into(),
                "slice" => format!("Ok::<_,String>({})", option("cgx2_slice(&v0,v1,v2)", t?)),
                "parse_i64" => format!(
                    "Ok::<_,String>(match cgx2_parse(&v0){{Ok(v)=>{}::{}(v),Err(e)=>{}::{}(e)}})",
                    rust_type(t?),
                    rust_name("Ok"),
                    rust_type(t?),
                    rust_name("Err")
                ),
                _ => unreachable!(),
            };
            Some(format!("{{{prefix}{value}}}"))
        }
        _ => None,
    }
}

pub(super) fn account(e: &Expr, value: String, finite: bool) -> String {
    if !finite {
        return format!("{{ctx.step({}, {})?;{value}}}", e.span.start, e.span.end);
    }
    format!(
        "{{ctx.step({}, {})?;let cgx2_value=(|| -> Result<_,String> {{ {value} }})()?;cgx2_charge(ctx,&cgx2_value,{}, {})?;Ok::<_,String>(cgx2_value)}}",
        e.span.start, e.span.end, e.span.start, e.span.end
    )
}
