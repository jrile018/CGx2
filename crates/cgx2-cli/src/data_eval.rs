fn prepare_data(module: &Module) -> Result<Option<Module>, RunError> {
    let required =
        cgx2_sema::foundation_profile(module) || cgx2_syntax::s2a_feature_span(module).is_some();
    let typed = match cgx2_sema::check_module(module) {
        Ok(t) => t,
        Err(es) if required => return Err(error("CGR009", es[0].message.clone(), es[0].span)),
        Err(_) => return Ok(None),
    };
    if !typed.finite_values {
        return Ok(None);
    }
    cgx2_sema::lower_module(module, &typed)
        .map(Some)
        .map_err(|es| error("CGR009", es[0].message.clone(), es[0].span))
}
fn check_value_bound(value: &Value, span: Span) -> Result<(), RunError> {
    fn cost(v: &Value, remaining: &mut usize, depth: usize) -> bool {
        if depth >= 64 || *remaining == 0 {
            return false;
        }
        *remaining -= 1;
        match v {
            Value::Sequence { values, .. } => {
                values.len() <= 65536 && values.iter().all(|v| cost(v, remaining, depth + 1))
            }
            Value::String(s) => s.len() <= 1048576,
            Value::Variant { payloads, .. } => {
                payloads.iter().all(|v| cost(v, remaining, depth + 1))
            }
            Value::Record { fields, .. } => {
                fields.iter().all(|(_, v)| cost(v, remaining, depth + 1))
            }
            _ => true,
        }
    }
    if cost(value, &mut 65537, 0) && value_size(value).1 <= 1048576 {
        Ok(())
    } else {
        Err(error(
            "CGR010",
            "finite value exceeds size/depth budget",
            span,
        ))
    }
}
fn is_data_call(n: &str) -> bool {
    matches!(
        n,
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
}
fn access(xs: &Value, i: i64) -> Option<Value> {
    let i = usize::try_from(i).ok()?;
    match xs {
        Value::Sequence { values, .. } => values.get(i).cloned(),
        Value::String(s) => s.chars().nth(i).map(|c| Value::String(c.to_string())),
        _ => None,
    }
}
impl Evaluator<'_> {
    fn expression_type(&self, span: Span) -> Option<&cgx2_sema::Type> {
        self.typed.as_ref().and_then(|t| {
            t.bodies
                .get(&self.owner)
                .and_then(|b| b.get(span))
                .or_else(|| t.expressions.get(span))
        })
    }
    fn option(&self, span: Span, value: Option<Value>) -> Result<Value, RunError> {
        let ty = self
            .expression_type(span)
            .cloned()
            .ok_or_else(|| error("CGR009", "checked access lacks type evidence", span))?;
        Ok(match value {
            Some(v) => Value::Variant {
                ty,
                case_index: 0,
                case_name: "Some".into(),
                payloads: vec![v],
            },
            None => Value::Variant {
                ty,
                case_index: 1,
                case_name: "None".into(),
                payloads: vec![],
            },
        })
    }
    fn data_call(&self, n: &str, mut args: Vec<Value>, span: Span) -> Result<Value, RunError> {
        let expected = match n {
            "length" | "show" | "parse_i64" | "array" | "list" => 1,
            "get" | "append" | "concat" => 2,
            _ => 3,
        };
        expect_arity(n, expected, args.len(), span)?;
        for v in &args {
            check_value_bound(v, span)?;
        }
        let first = args.remove(0);
        let value = match (n, first) {
            ("show", v) => {
                check_output_bound(&v, span)?;
                Value::String(v.to_string())
            }
            ("length", Value::Sequence { values, .. }) => Value::I64(values.len() as i64),
            ("length", Value::String(s)) => Value::I64(s.chars().count() as i64),
            ("get", xs) => {
                let Value::I64(i) = args[0] else {
                    return Err(error("CGR004", "index requires i64", span));
                };
                return self.option(span, access(&xs, i));
            }
            ("updated", Value::Sequence { ty, mut values }) => {
                let Value::I64(i) = args[0] else {
                    return Err(error("CGR004", "index requires i64", span));
                };
                let Some(i) = usize::try_from(i).ok().filter(|i| *i < values.len()) else {
                    return self.option(span, None);
                };
                values[i] = args.remove(1);
                return self.option(span, Some(Value::Sequence { ty, values }));
            }
            ("append", Value::Sequence { ty, mut values }) => {
                if values.len() >= 65536 {
                    return Err(error("CGR010", "collection length exceeds 65536", span));
                }
                values
                    .try_reserve(1)
                    .map_err(|_| error("CGR010", "allocation failed", span))?;
                values.push(args.remove(0));
                Value::Sequence { ty, values }
            }
            (
                "array",
                Value::Sequence {
                    ty: cgx2_sema::Type::List(t),
                    values,
                },
            ) => Value::Sequence {
                ty: cgx2_sema::Type::Array(t),
                values,
            },
            (
                "list",
                Value::Sequence {
                    ty: cgx2_sema::Type::Array(t),
                    values,
                },
            ) => Value::Sequence {
                ty: cgx2_sema::Type::List(t),
                values,
            },
            ("concat", Value::String(mut a)) => {
                let Value::String(b) = args.remove(0) else {
                    return Err(error("CGR004", "concat requires String", span));
                };
                if a.len().checked_add(b.len()).is_none_or(|n| n > 1048576) {
                    return Err(error("CGR010", "string exceeds 1048576 bytes", span));
                }
                a.try_reserve(b.len())
                    .map_err(|_| error("CGR010", "allocation failed", span))?;
                a.push_str(&b);
                Value::String(a)
            }
            ("slice", Value::String(s)) => {
                let (Value::I64(a), Value::I64(b)) = (&args[0], &args[1]) else {
                    return Err(error("CGR004", "slice positions require i64", span));
                };
                let positions = usize::try_from(*a).ok().zip(usize::try_from(*b).ok());
                let part = positions
                    .filter(|(a, b)| a <= b && *b <= s.chars().count())
                    .map(|(a, b)| Value::String(s.chars().skip(a).take(b - a).collect()));
                return self.option(span, part);
            }
            ("parse_i64", Value::String(s)) => {
                let digits = s
                    .strip_prefix('-')
                    .or_else(|| s.strip_prefix('+'))
                    .unwrap_or(&s);
                let parsed = if !digits.is_empty() && digits.bytes().all(|c| c.is_ascii_digit()) {
                    s.parse::<i64>().ok()
                } else {
                    None
                };
                let ty = self
                    .expression_type(span)
                    .cloned()
                    .ok_or_else(|| error("CGR009", "parse lacks checked result type", span))?;
                match parsed {
                    Some(v) => Value::Variant {
                        ty,
                        case_index: 0,
                        case_name: "Ok".into(),
                        payloads: vec![Value::I64(v)],
                    },
                    None => Value::Variant {
                        ty,
                        case_index: 1,
                        case_name: "Err".into(),
                        payloads: vec![Value::String("invalid integer".into())],
                    },
                }
            }
            _ => return Err(error("CGR004", "unsupported data operand", span)),
        };
        check_value_bound(&value, span)?;
        Ok(value)
    }
}

fn value_size(v: &Value) -> (usize, usize) {
    let children: Vec<&Value> = match v {
        Value::Sequence { values, .. } => values.iter().collect(),
        Value::Variant { payloads, .. } => payloads.iter().collect(),
        Value::Record { fields, .. } => fields.iter().map(|(_, v)| v).collect(),
        _ => vec![],
    };
    children.into_iter().fold(
        (1usize, if let Value::String(s) = v { s.len() } else { 0 }),
        |(n, b), v| {
            let (c, d) = value_size(v);
            (n.saturating_add(c), b.saturating_add(d))
        },
    )
}
