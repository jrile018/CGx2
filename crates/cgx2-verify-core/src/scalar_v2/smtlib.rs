use std::collections::{BTreeMap, BTreeSet};

use super::{Contract, Expr, Failure, Sort, ValidationError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmtError {
    InvalidContract(ValidationError),
    UnmodeledLocalBinding,
}

struct Encoded {
    term: String,
    defined: String,
}

fn context(contract: &Contract) -> Result<(String, Encoded, Encoded), SmtError> {
    contract.validate().map_err(SmtError::InvalidContract)?;
    // External-result range assumptions are global in this encoding. Reject
    // mixing them with lexical binders rather than emitting a free local name.
    if (contract.requires.contains_binding() || contract.ensures.contains_binding())
        && (contract.requires.contains_uninterpreted() || contract.ensures.contains_uninterpreted())
    {
        return Err(SmtError::UnmodeledLocalBinding);
    }
    let mut functions = BTreeMap::new();
    let variables = contract
        .parameters
        .iter()
        .map(|p| (p.name.as_str(), p.sort))
        .collect::<BTreeMap<_, _>>();
    let mut foreign_bounds = BTreeSet::new();
    let requires = encode(
        &contract.requires,
        &variables,
        &mut functions,
        &mut foreign_bounds,
    );
    let ensures = encode(
        &contract.ensures,
        &variables,
        &mut functions,
        &mut foreign_bounds,
    );
    let mut text = String::from("(set-option :produce-models true)\n(set-logic QF_UFNIA)\n; i64 values are mathematical Ints constrained to the signed 64-bit range.\n; Uninterpreted functions, if declared, represent pure total external results.\n");
    for parameter in &contract.parameters {
        let symbol = symbol("v", &parameter.name);
        text.push_str(&format!(
            "(declare-const {symbol} {})\n",
            sort_name(parameter.sort)
        ));
        if parameter.sort == Sort::I64 {
            text.push_str(&format!("(assert (>= {symbol} (- 9223372036854775808)))\n(assert (<= {symbol} 9223372036854775807))\n"));
        }
    }
    for (symbol, signature) in functions {
        text.push_str(&format!(
            "(declare-fun {symbol} ({}) {})\n",
            signature
                .0
                .iter()
                .map(|s| sort_name(*s))
                .collect::<Vec<_>>()
                .join(" "),
            sort_name(signature.1)
        ));
    }
    for bound in foreign_bounds {
        text.push_str(&format!("(assert {bound})\n"));
    }
    Ok((text, requires, ensures))
}

/// Four independent checks in fixed order: undefined requires, satisfiable
/// requires, false ensures, and undefined (overflowing) ensures.
pub fn emit_smtlib(contract: &Contract) -> Result<String, SmtError> {
    let (mut text, requires, ensures) = context(contract)?;
    text.push_str(&format!(
        "; vc:requires_defined\n(push 1)\n(assert (not {}))\n(check-sat)\n(pop 1)\n",
        requires.defined
    ));
    text.push_str(&format!(
        "; vc:requires_satisfiable\n(push 1)\n(assert (and {} {}))\n(check-sat)\n(pop 1)\n",
        requires.defined, requires.term
    ));
    text.push_str(&format!(
        "; vc:ensures_false\n(push 1)\n(assert (and {} {} {} (not {})))\n(check-sat)\n(pop 1)\n",
        requires.defined, requires.term, ensures.defined, ensures.term
    ));
    text.push_str(&format!(
        "; vc:ensures_overflow\n(push 1)\n(assert (and {} {} (not {})))\n(check-sat)\n(pop 1)\n",
        requires.defined, requires.term, ensures.defined
    ));
    Ok(text)
}

/// Query one reported violation again and request all parameter values.
pub fn emit_witness_smtlib(contract: &Contract, failure: Failure) -> Result<String, SmtError> {
    let (mut text, requires, ensures) = context(contract)?;
    let violation = match failure {
        Failure::EnsuresFalse => format!("(and {} (not {}))", ensures.defined, ensures.term),
        Failure::Overflow => format!("(not {})", ensures.defined),
    };
    text.push_str(&format!(
        "(assert (and {} {} {violation}))\n(check-sat)\n",
        requires.defined, requires.term
    ));
    if !contract.parameters.is_empty() {
        let names = contract
            .parameters
            .iter()
            .map(|p| symbol("v", &p.name))
            .collect::<Vec<_>>();
        text.push_str(&format!("(get-value ({}))\n", names.join(" ")));
    }
    Ok(text)
}

fn sort_name(sort: Sort) -> &'static str {
    match sort {
        Sort::Bool => "Bool",
        Sort::I64 => "Int",
    }
}

pub(crate) fn symbol(prefix: &str, name: &str) -> String {
    let mut result = String::from(prefix);
    result.push('_');
    for byte in name.bytes() {
        result.push_str(&format!("{byte:02x}"));
    }
    result
}

fn encode(
    expr: &Expr,
    variables: &BTreeMap<&str, Sort>,
    functions: &mut BTreeMap<String, (Vec<Sort>, Sort)>,
    foreign_bounds: &mut BTreeSet<String>,
) -> Encoded {
    use Expr::*;
    match expr {
        Bool(value) => Encoded {
            term: value.to_string(),
            defined: "true".into(),
        },
        I64(value) => Encoded {
            term: int(*value),
            defined: "true".into(),
        },
        Var(name) => Encoded {
            term: symbol("v", name),
            defined: "true".into(),
        },
        If(guard, yes, no) => {
            let guard = encode(guard, variables, functions, foreign_bounds);
            let yes = encode(yes, variables, functions, foreign_bounds);
            let no = encode(no, variables, functions, foreign_bounds);
            Encoded {
                term: format!("(ite {} {} {})", guard.term, yes.term, no.term),
                defined: format!(
                    "(and {} (ite {} {} {}))",
                    guard.defined, guard.term, yes.defined, no.defined
                ),
            }
        }
        Not(a) => {
            let a = encode(a, variables, functions, foreign_bounds);
            Encoded {
                term: format!("(not {})", a.term),
                defined: a.defined,
            }
        }
        Let(name, value, body) => {
            let binding = encode(value, variables, functions, foreign_bounds);
            let mut scope = variables.clone();
            scope.insert(
                name.as_str(),
                value.sort(variables).expect("validated binding"),
            );
            let body = encode(body, &scope, functions, foreign_bounds);
            let symbol = symbol("v", name);
            Encoded {
                term: format!("(let (({symbol} {})) {})", binding.term, body.term),
                defined: format!(
                    "(and {} (let (({symbol} {})) {}))",
                    binding.defined, binding.term, body.defined
                ),
            }
        }
        Add(a, b) | Sub(a, b) | Mul(a, b) | Div(a, b) => {
            let a = encode(a, variables, functions, foreign_bounds);
            let b = encode(b, variables, functions, foreign_bounds);
            let term = if matches!(expr, Div(_, _)) {
                // SMT div rounds down; CGx2 signed division truncates toward zero.
                let abs_a = format!("(ite (< {} 0) (- {}) {})", a.term, a.term, a.term);
                let abs_b = format!("(ite (< {} 0) (- {}) {})", b.term, b.term, b.term);
                let magnitude = format!("(div {abs_a} {abs_b})");
                format!(
                    "(ite (= {} 0) 0 (ite (= (< {} 0) (< {} 0)) {magnitude} (- {magnitude})))",
                    b.term, a.term, b.term
                )
            } else {
                let op = match expr {
                    Add(_, _) => "+",
                    Sub(_, _) => "-",
                    _ => "*",
                };
                format!("({op} {} {})", a.term, b.term)
            };
            let defined = format!(
                "(and {} {} (>= {term} (- 9223372036854775808)) (<= {term} 9223372036854775807))",
                a.defined, b.defined
            );
            let defined = if matches!(expr, Div(_, _)) {
                format!("(and (not (= {} 0)) {defined})", b.term)
            } else {
                defined
            };
            Encoded { term, defined }
        }
        Eq(a, b) | Lt(a, b) | Le(a, b) | Gt(a, b) | Ge(a, b) | And(a, b) | Or(a, b) => {
            let a = encode(a, variables, functions, foreign_bounds);
            let b = encode(b, variables, functions, foreign_bounds);
            let op = match expr {
                Eq(_, _) => "=",
                Lt(_, _) => "<",
                Le(_, _) => "<=",
                Gt(_, _) => ">",
                Ge(_, _) => ">=",
                And(_, _) => "and",
                Or(_, _) => "or",
                _ => unreachable!(),
            };
            Encoded {
                term: format!("({op} {} {})", a.term, b.term),
                defined: format!("(and {} {})", a.defined, b.defined),
            }
        }
        Uninterpreted { name, sort, args } => {
            // Validation established every referenced variable and argument sort.
            let signatures = args
                .iter()
                .map(|arg| arg.sort(variables).expect("validated expression"))
                .collect::<Vec<_>>();
            let symbol = symbol("u", &format!("{name}:{signatures:?}:{sort:?}"));
            functions.insert(symbol.clone(), (signatures, *sort));
            let args = args
                .iter()
                .map(|arg| encode(arg, variables, functions, foreign_bounds))
                .collect::<Vec<_>>();
            let term = if args.is_empty() {
                symbol
            } else {
                format!(
                    "({symbol} {})",
                    args.iter()
                        .map(|arg| arg.term.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            };
            if *sort == Sort::I64 {
                foreign_bounds.insert(format!(
                    "(and (>= {term} (- 9223372036854775808)) (<= {term} 9223372036854775807))"
                ));
            }
            let defined = and(args.iter().map(|arg| arg.defined.as_str()));
            Encoded { term, defined }
        }
    }
}

fn int(value: i64) -> String {
    if value < 0 {
        format!("(- {})", -(value as i128))
    } else {
        value.to_string()
    }
}

fn and<'a>(items: impl Iterator<Item = &'a str>) -> String {
    let items = items.collect::<Vec<_>>();
    if items.is_empty() {
        "true".into()
    } else {
        format!("(and {})", items.join(" "))
    }
}
