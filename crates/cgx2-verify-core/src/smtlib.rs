use std::collections::{BTreeMap, BTreeSet};

use crate::{Contract, Expr, Sort, ValidationError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SmtError {
    InvalidContract(ValidationError),
}

struct Encoded {
    term: String,
    defined: String,
}

/// Emit three queries: precondition overflow, precondition satisfiability,
/// then the negation of the postcondition (including checked overflow).
pub fn emit_smtlib(contract: &Contract) -> Result<String, SmtError> {
    contract.validate().map_err(SmtError::InvalidContract)?;
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
    let mut text = String::from("(set-logic QF_UFLIA)\n; i64 values are mathematical Ints constrained to the signed 64-bit range.\n; Uninterpreted functions, if declared, represent pure total external results.\n");
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
    text.push_str(&format!(
        "(push 1)\n(assert (not {}))\n(check-sat)\n(pop 1)\n",
        requires.defined
    ));
    text.push_str(&format!(
        "(push 1)\n(assert (and {} {}))\n(check-sat)\n",
        requires.defined, requires.term
    ));
    text.push_str(&format!(
        "(push 1)\n(assert (or (not {}) (not {})))\n(check-sat)\n(pop 1)\n(pop 1)\n",
        ensures.defined, ensures.term
    ));
    Ok(text)
}

fn sort_name(sort: Sort) -> &'static str {
    match sort {
        Sort::Bool => "Bool",
        Sort::I64 => "Int",
    }
}

fn symbol(prefix: &str, name: &str) -> String {
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
        Not(a) => {
            let a = encode(a, variables, functions, foreign_bounds);
            Encoded {
                term: format!("(not {})", a.term),
                defined: a.defined,
            }
        }
        Add(a, b) | Sub(a, b) => {
            let a = encode(a, variables, functions, foreign_bounds);
            let b = encode(b, variables, functions, foreign_bounds);
            let op = if matches!(expr, Add(_, _)) { "+" } else { "-" };
            let term = format!("({op} {} {})", a.term, b.term);
            let defined = format!(
                "(and {} {} (>= {term} (- 9223372036854775808)) (<= {term} 9223372036854775807))",
                a.defined, b.defined
            );
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
