use super::*;

pub(super) fn syntax_type(ty: &TypeSyntax) -> Type {
    match ty.path.segments.join(".").as_str() {
        "i64" => Type::I64,
        "Bool" => Type::Bool,
        "String" => Type::String,
        "Unit" => Type::Unit,
        "List" => Type::List(Box::new(syntax_type(&ty.arguments[0]))),
        "Array" => Type::Array(Box::new(syntax_type(&ty.arguments[0]))),
        "Option" | "Result" => Type::Variant {
            name: format!("cgx2.core.{}", ty.path.segments[0]),
            arguments: ty.arguments.iter().map(syntax_type).collect(),
        },
        name if !ty.arguments.is_empty() => Type::Variant {
            name: name.to_owned(),
            arguments: ty.arguments.iter().map(syntax_type).collect(),
        },
        // Concrete user variants and records have the same generated name.
        name => Type::Record(name.to_owned()),
    }
}

pub(super) fn emit_variants(modules: &[(&Module, &TypedModule)], out: &mut String) {
    fn collect(ty: &Type, found: &mut BTreeMap<String, Type>) {
        if let Type::List(t) | Type::Array(t) = ty {
            collect(t, found);
        }
        if let Type::Variant { arguments, .. } = ty {
            found.insert(ty.to_string(), ty.clone());
            for ty in arguments {
                collect(ty, found);
            }
        }
    }
    let mut found = BTreeMap::new();
    for (module, typed) in modules {
        for (_, ty) in typed.expressions.iter() {
            collect(ty, &mut found);
        }
        for signature in typed.functions.values() {
            collect(&signature.return_type, &mut found);
            for ty in &signature.parameters {
                collect(ty, &mut found);
            }
        }
        for record in &module.records {
            for field in &record.fields {
                collect(&syntax_type(&field.ty), &mut found);
            }
        }
        for variant in &module.variants {
            for case in &variant.cases {
                for ty in &case.payloads {
                    collect(&syntax_type(ty), &mut found);
                }
            }
        }
    }
    let mut definitions = BTreeMap::new();
    for (module, _) in modules {
        for variant in &module.variants {
            let name = module
                .name
                .as_ref()
                .filter(|_| !variant.name.contains('.'))
                .map_or_else(
                    || variant.name.clone(),
                    |p| format!("{}.{}", p.segments.join("."), variant.name),
                );
            definitions.insert(
                name,
                variant
                    .cases
                    .iter()
                    .map(|c| {
                        (
                            c.name.clone(),
                            c.payloads.iter().map(syntax_type).collect::<Vec<_>>(),
                        )
                    })
                    .collect::<Vec<_>>(),
            );
        }
    }
    for (key, ty) in found {
        if let Type::Variant { name, arguments } = ty {
            if name == "cgx2.core.Option" {
                definitions.insert(
                    key,
                    vec![
                        ("Some".into(), vec![arguments[0].clone()]),
                        ("None".into(), vec![]),
                    ],
                );
            } else if name == "cgx2.core.Result" {
                definitions.insert(
                    key,
                    vec![
                        ("Ok".into(), vec![arguments[0].clone()]),
                        ("Err".into(), vec![arguments[1].clone()]),
                    ],
                );
            }
        }
    }
    for (identity, cases) in definitions {
        let name = rust_name(&identity);
        out.push_str(&format!(
            "#[derive(Clone, PartialEq, Eq)]\nenum {name} {{\n"
        ));
        for (case, payloads) in &cases {
            out.push_str(&format!("    {}", rust_name(case)));
            if !payloads.is_empty() {
                out.push('(');
                out.push_str(
                    &payloads
                        .iter()
                        .map(rust_type)
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                out.push(')');
            }
            out.push_str(",\n");
        }
        out.push_str("}\n");
        out.push_str(&format!("impl std::fmt::Display for {name} {{\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{\n        match self {{\n"));
        for (case, payloads) in &cases {
            out.push_str(&format!("            Self::{}", rust_name(case)));
            if !payloads.is_empty() {
                out.push('(');
                out.push_str(
                    &(0..payloads.len())
                        .map(|i| format!("v{i}"))
                        .collect::<Vec<_>>()
                        .join(", "),
                );
                out.push(')');
            }
            out.push_str(&format!(
                " => {{ f.write_str({})?;",
                quote_rust(&format!("{identity}.{case}("))
            ));
            for (i, ty) in payloads.iter().enumerate() {
                if i > 0 {
                    out.push_str(" f.write_str(\", \")?;");
                }
                if *ty != Type::Unit {
                    out.push_str(&format!(" write!(f, \"{{}}\", v{i})?;"));
                }
            }
            out.push_str(" f.write_str(\")\") },\n");
        }
        out.push_str("        }\n    }\n}\n\n");
        out.push_str(&format!(
            "impl Cgx2Show for {name} {{ fn cg_show(&self)->String {{format!(\"{{}}\",self)}}\n"
        ));
        for method in ["cg_nodes", "cg_bytes"] {
            out.push_str(&format!("fn {method}(&self)->usize {{ match self {{\n"));
            for (case, payloads) in &cases {
                let bindings = (0..payloads.len())
                    .map(|i| format!("v{i}"))
                    .collect::<Vec<_>>();
                out.push_str(&format!(
                    "Self::{}{} => {}usize",
                    rust_name(case),
                    if bindings.is_empty() {
                        String::new()
                    } else {
                        format!("({})", bindings.join(", "))
                    },
                    if method == "cg_nodes" { 1 } else { 0 }
                ));
                for binding in bindings {
                    out.push_str(&format!(".saturating_add({binding}.{method}())"));
                }
                out.push_str(",\n");
            }
            out.push_str("} }\n");
        }
        out.push_str("}\n");
    }
}

pub(super) fn constructor(
    ty: &TypeSyntax,
    case: &str,
    arguments: &[cgx2_syntax::CallArg],
    module: Option<&str>,
    typed: &TypedModule,
) -> String {
    let ty = syntax_type(ty);
    let name = match &ty {
        Type::Record(name) => rust_name(name),
        _ => rust_type(&ty),
    };
    let mut prefix = String::new();
    for (i, arg) in arguments.iter().enumerate() {
        prefix.push_str(&format!(
            "let cgx2_payload_{i} = {}?; ",
            emit_expr_context(&arg.value, module, typed)
        ));
    }
    let payloads = if arguments.is_empty() {
        String::new()
    } else {
        format!(
            "({})",
            (0..arguments.len())
                .map(|i| format!("cgx2_payload_{i}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    format!(
        "{{ {prefix}Ok::<_, String>({name}::{}{payloads}) }}",
        rust_name(case)
    )
}
