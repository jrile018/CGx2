//! Independent finite-domain interpreter for the SMT terms emitted by this crate.
//! This audits encoding parity; finite samples are never a global proof.
use std::collections::BTreeMap;

use super::smtlib::symbol;
use super::{check_smt_limits, SmtLimits, SMT_NODE_LIMIT, SMT_OUTPUT_LIMIT};
use super::{eval, Contract, Domain, EvalError, Expr, Sort, Value};

pub const OBLIGATION_NAMES: [&str; 4] = [
    "requires_defined",
    "requires_satisfiable",
    "ensures_false",
    "ensures_overflow",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodingAudit {
    pub cases: usize,
    pub obligations: [usize; 4],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuditError {
    InvalidInput(String),
    Unsupported(String),
    MalformedFormula(String),
    Mismatch {
        obligation: &'static str,
        values: BTreeMap<String, Value>,
    },
    SourceMismatch {
        obligation: &'static str,
        values: BTreeMap<String, Value>,
    },
}
pub type SourceEvaluator<'a> =
    dyn Fn(&BTreeMap<String, Value>) -> Result<[bool; 4], AuditError> + 'a;

#[derive(Clone, Debug)]
enum Term {
    Atom(String),
    List(Vec<Term>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SmtValue {
    Bool(bool),
    Int(i128),
}

fn tokenize(input: &str) -> Result<Vec<&str>, AuditError> {
    if input.len() > 256 * 1024 || !input.is_ascii() {
        return Err(AuditError::MalformedFormula(
            "formula size or encoding".into(),
        ));
    }
    let bytes = input.as_bytes();
    let mut result = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        let start = at;
        if matches!(bytes[at], b'(' | b')') {
            at += 1;
        } else {
            while at < bytes.len()
                && !bytes[at].is_ascii_whitespace()
                && !matches!(bytes[at], b'(' | b')')
            {
                at += 1;
            }
        }
        result.push(&input[start..at]);
        if result.len() > 32768 {
            return Err(AuditError::MalformedFormula("too many tokens".into()));
        }
    }
    Ok(result)
}

fn parse(tokens: &[&str], at: &mut usize, depth: usize) -> Result<Term, AuditError> {
    if depth > 256 {
        return Err(AuditError::MalformedFormula("term nesting".into()));
    }
    let token = *tokens
        .get(*at)
        .ok_or_else(|| AuditError::MalformedFormula("truncated term".into()))?;
    *at += 1;
    match token {
        "(" => {
            let mut items = Vec::new();
            while tokens.get(*at).copied() != Some(")") {
                if *at == tokens.len() {
                    return Err(AuditError::MalformedFormula("unclosed term".into()));
                }
                items.push(parse(tokens, at, depth + 1)?);
            }
            *at += 1;
            Ok(Term::List(items))
        }
        ")" => Err(AuditError::MalformedFormula("unexpected close".into())),
        _ => Ok(Term::Atom(token.into())),
    }
}

fn obligations(script: &str) -> Result<[Term; 4], AuditError> {
    let mut found: [Option<Term>; 4] = std::array::from_fn(|_| None);
    let mut lines = script.lines();
    while let Some(line) = lines.next() {
        let Some(name) = line.strip_prefix("; vc:") else {
            continue;
        };
        let index = OBLIGATION_NAMES
            .iter()
            .position(|item| *item == name)
            .ok_or_else(|| AuditError::MalformedFormula(format!("unknown obligation {name}")))?;
        if found[index].is_some() || lines.next() != Some("(push 1)") {
            return Err(AuditError::MalformedFormula(format!(
                "duplicate or incomplete {name}"
            )));
        }
        let assertion = lines
            .next()
            .ok_or_else(|| AuditError::MalformedFormula(format!("missing {name}")))?;
        let tokens = tokenize(assertion)?;
        let mut at = 0;
        let term = parse(&tokens, &mut at, 0)?;
        if at != tokens.len()
            || lines.next() != Some("(check-sat)")
            || lines.next() != Some("(pop 1)")
        {
            return Err(AuditError::MalformedFormula(format!("incomplete {name}")));
        }
        let Term::List(items) = term else {
            return Err(AuditError::MalformedFormula("assertion shape".into()));
        };
        if items.len() != 2 || !matches!(&items[0], Term::Atom(op) if op == "assert") {
            return Err(AuditError::MalformedFormula("assertion shape".into()));
        }
        found[index] = Some(items[1].clone());
    }
    let mut terms = Vec::new();
    for (index, term) in found.into_iter().enumerate() {
        terms.push(term.ok_or_else(|| {
            AuditError::MalformedFormula(format!("missing {}", OBLIGATION_NAMES[index]))
        })?);
    }
    terms
        .try_into()
        .map_err(|_| AuditError::MalformedFormula("obligation count".into()))
}

// Parse actual emitted definitions, independently of the source/core encoder.
// Definitions are evaluated once, in order, for each finite-domain tuple.
fn definitions(script: &str) -> Result<Vec<(String, Sort, Term)>, AuditError> {
    if script.len() > SMT_OUTPUT_LIMIT || !script.is_ascii() {
        return Err(AuditError::MalformedFormula(
            "script size or encoding".into(),
        ));
    }
    let mut result = Vec::new();
    let mut names = std::collections::BTreeSet::new();
    let mut obligations_started = false;
    for line in script.lines() {
        obligations_started |= line.starts_with("; vc:");
        if !line.starts_with("(define-fun") {
            continue;
        }
        if obligations_started || result.len() >= 4 * SMT_NODE_LIMIT {
            return Err(AuditError::MalformedFormula(
                "definition position/count".into(),
            ));
        }
        let tokens = tokenize(line)?;
        let mut at = 0;
        let parsed = parse(&tokens, &mut at, 0)?;
        if at != tokens.len() {
            return Err(AuditError::MalformedFormula("definition tokens".into()));
        }
        let Term::List(items) = parsed else {
            return Err(AuditError::MalformedFormula("definition shape".into()));
        };
        let [Term::Atom(op), Term::Atom(name), Term::List(args), Term::Atom(sort), body] =
            items.as_slice()
        else {
            return Err(AuditError::MalformedFormula("definition shape".into()));
        };
        if op != "define-fun"
            || !name.starts_with("cgx2_")
            || !args.is_empty()
            || !names.insert(name.clone())
        {
            return Err(AuditError::MalformedFormula(
                "definition name/arguments".into(),
            ));
        }
        let sort = match sort.as_str() {
            "Bool" => Sort::Bool,
            "Int" => Sort::I64,
            _ => return Err(AuditError::MalformedFormula("definition sort".into())),
        };
        result.push((name.clone(), sort, body.clone()));
    }
    Ok(result)
}

fn bool_value(value: SmtValue) -> Result<bool, AuditError> {
    if let SmtValue::Bool(value) = value {
        Ok(value)
    } else {
        Err(AuditError::MalformedFormula("expected Bool".into()))
    }
}

fn int_value(value: SmtValue) -> Result<i128, AuditError> {
    if let SmtValue::Int(value) = value {
        Ok(value)
    } else {
        Err(AuditError::MalformedFormula("expected Int".into()))
    }
}

fn evaluate(
    term: &Term,
    variables: &BTreeMap<String, SmtValue>,
    depth: usize,
) -> Result<SmtValue, AuditError> {
    if depth > 256 {
        return Err(AuditError::MalformedFormula("evaluation nesting".into()));
    }
    match term {
        Term::Atom(atom) => match atom.as_str() {
            "true" => Ok(SmtValue::Bool(true)),
            "false" => Ok(SmtValue::Bool(false)),
            _ => variables
                .get(atom)
                .copied()
                .or_else(|| atom.parse::<i128>().ok().map(SmtValue::Int))
                .ok_or_else(|| AuditError::MalformedFormula(format!("unknown term {atom}"))),
        },
        Term::List(items) => {
            let Some(Term::Atom(op)) = items.first() else {
                return Err(AuditError::MalformedFormula("operator".into()));
            };
            if op == "ite" && items.len() == 4 {
                let guard = bool_value(evaluate(&items[1], variables, depth + 1)?)?;
                return evaluate(&items[if guard { 2 } else { 3 }], variables, depth + 1);
            }
            if op == "let" && items.len() == 3 {
                let Term::List(bindings) = &items[1] else {
                    return Err(AuditError::MalformedFormula("let bindings".into()));
                };
                let mut scope = variables.clone();
                for binding in bindings {
                    let Term::List(pair) = binding else {
                        return Err(AuditError::MalformedFormula("let binding".into()));
                    };
                    let [Term::Atom(name), value] = pair.as_slice() else {
                        return Err(AuditError::MalformedFormula("let pair".into()));
                    };
                    scope.insert(name.clone(), evaluate(value, variables, depth + 1)?);
                }
                return evaluate(&items[2], &scope, depth + 1);
            }
            let args = items[1..]
                .iter()
                .map(|item| evaluate(item, variables, depth + 1))
                .collect::<Result<Vec<_>, _>>()?;
            match (op.as_str(), args.as_slice()) {
                ("not", [a]) => Ok(SmtValue::Bool(!bool_value(*a)?)),
                ("and", _) if !args.is_empty() => Ok(SmtValue::Bool(
                    args.into_iter()
                        .map(bool_value)
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        .all(|v| v),
                )),
                ("or", _) if !args.is_empty() => Ok(SmtValue::Bool(
                    args.into_iter()
                        .map(bool_value)
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        .any(|v| v),
                )),
                ("*", [a, b]) => int_value(*a)?
                    .checked_mul(int_value(*b)?)
                    .map(SmtValue::Int)
                    .ok_or_else(|| AuditError::Unsupported("SMT Int exceeds i128".into())),
                ("div", [a, b]) => {
                    let (a, b) = (int_value(*a)?, int_value(*b)?);
                    if a < 0 || b <= 0 {
                        return Err(AuditError::Unsupported(
                            "audit requires nonnegative div operands".into(),
                        ));
                    }
                    Ok(SmtValue::Int(a / b))
                }
                ("+", [a, b]) => int_value(*a)?
                    .checked_add(int_value(*b)?)
                    .map(SmtValue::Int)
                    .ok_or_else(|| AuditError::Unsupported("SMT Int exceeds i128".into())),
                ("-", [a]) => int_value(*a)?
                    .checked_neg()
                    .map(SmtValue::Int)
                    .ok_or_else(|| AuditError::Unsupported("SMT Int exceeds i128".into())),
                ("-", [a, b]) => int_value(*a)?
                    .checked_sub(int_value(*b)?)
                    .map(SmtValue::Int)
                    .ok_or_else(|| AuditError::Unsupported("SMT Int exceeds i128".into())),
                ("=", [a, b]) => Ok(SmtValue::Bool(a == b)),
                ("<", [a, b]) => Ok(SmtValue::Bool(int_value(*a)? < int_value(*b)?)),
                ("<=", [a, b]) => Ok(SmtValue::Bool(int_value(*a)? <= int_value(*b)?)),
                (">", [a, b]) => Ok(SmtValue::Bool(int_value(*a)? > int_value(*b)?)),
                (">=", [a, b]) => Ok(SmtValue::Bool(int_value(*a)? >= int_value(*b)?)),
                _ => Err(AuditError::MalformedFormula(format!(
                    "unsupported SMT operator {op}"
                ))),
            }
        }
    }
}

fn expectation(
    requires: &Expr,
    ensures: &Expr,
    values: &BTreeMap<String, Value>,
) -> Result<[bool; 4], AuditError> {
    let pre = eval(requires, values);
    let undefined = matches!(pre, Err(EvalError::Overflow));
    let admitted = pre == Ok(Value::Bool(true));
    if let Err(error) = &pre {
        if *error != EvalError::Overflow {
            return Err(AuditError::Unsupported(format!(
                "requires evaluation: {error:?}"
            )));
        }
    }
    let post = if admitted {
        Some(eval(ensures, values))
    } else {
        None
    };
    if let Some(Err(error)) = &post {
        if *error != EvalError::Overflow {
            return Err(AuditError::Unsupported(format!(
                "ensures evaluation: {error:?}"
            )));
        }
    }
    Ok([
        undefined,
        admitted,
        post == Some(Ok(Value::Bool(false))),
        post == Some(Err(EvalError::Overflow)),
    ])
}

/// Exhaustively compares every tuple in the supplied finite domain with all four
/// emitted SMT assertions. `max_cases` bounds work; `cases` counts every tuple,
/// including those rejected by `requires`.
pub fn audit_encoding(
    contract: &Contract,
    domains: &[Domain],
    max_cases: usize,
    script: &str,
) -> Result<EncodingAudit, AuditError> {
    audit_inner(contract, domains, max_cases, script, None)
}

/// Also compares the lowered contract against a separate source evaluator on
/// every tuple. The callback must evaluate the original checked source AST.
pub fn audit_encoding_with_source(
    contract: &Contract,
    domains: &[Domain],
    max_cases: usize,
    script: &str,
    source: &SourceEvaluator<'_>,
) -> Result<EncodingAudit, AuditError> {
    audit_inner(contract, domains, max_cases, script, Some(source))
}

fn audit_inner(
    contract: &Contract,
    domains: &[Domain],
    max_cases: usize,
    script: &str,
    source: Option<&SourceEvaluator<'_>>,
) -> Result<EncodingAudit, AuditError> {
    check_smt_limits(contract, SmtLimits::default())
        .map_err(|e| AuditError::InvalidInput(format!("{e:?}")))?;
    let definitions = definitions(script)?;
    contract
        .validate()
        .map_err(|error| AuditError::InvalidInput(format!("{error:?}")))?;
    if contract.requires.contains_uninterpreted() || contract.ensures.contains_uninterpreted() {
        return Err(AuditError::Unsupported("uninterpreted call".into()));
    }
    let formulas = obligations(script)?;
    let by_name = domains
        .iter()
        .map(|domain| (domain.name.as_str(), domain))
        .collect::<BTreeMap<_, _>>();
    if by_name.len() != domains.len() {
        return Err(AuditError::InvalidInput("duplicate domain".into()));
    }
    let mut choices = Vec::new();
    let mut total = 1usize;
    for parameter in &contract.parameters {
        let values = match parameter.sort {
            Sort::Bool => {
                if by_name.contains_key(parameter.name.as_str()) {
                    return Err(AuditError::InvalidInput("domain for Bool".into()));
                }
                vec![Value::Bool(false), Value::Bool(true)]
            }
            Sort::I64 => {
                let domain = by_name.get(parameter.name.as_str()).ok_or_else(|| {
                    AuditError::InvalidInput(format!("missing domain {}", parameter.name))
                })?;
                if domain.min > domain.max {
                    return Err(AuditError::InvalidInput("empty domain".into()));
                }
                let width = (i128::from(domain.max) - i128::from(domain.min) + 1)
                    .try_into()
                    .map_err(|_| AuditError::InvalidInput("domain too wide".into()))?;
                total = total
                    .checked_mul(width)
                    .ok_or_else(|| AuditError::InvalidInput("case count overflow".into()))?;
                if total > max_cases {
                    return Err(AuditError::InvalidInput("case cap".into()));
                }
                (domain.min..=domain.max).map(Value::I64).collect()
            }
        };
        if parameter.sort == Sort::Bool {
            total = total
                .checked_mul(2)
                .ok_or_else(|| AuditError::InvalidInput("case count overflow".into()))?;
            if total > max_cases {
                return Err(AuditError::InvalidInput("case cap".into()));
            }
        }
        choices.push(values);
    }
    if total == 0 || total > max_cases {
        return Err(AuditError::InvalidInput("case cap".into()));
    }
    if by_name
        .keys()
        .any(|name| !contract.parameters.iter().any(|p| p.name == *name))
    {
        return Err(AuditError::InvalidInput("unused domain".into()));
    }
    let mut audit = EncodingAudit {
        cases: 0,
        obligations: [0; 4],
    };
    let mut indices = vec![0; choices.len()];
    loop {
        let values = contract
            .parameters
            .iter()
            .enumerate()
            .map(|(i, p)| (p.name.clone(), choices[i][indices[i]].clone()))
            .collect::<BTreeMap<_, _>>();
        let mut variables = values
            .iter()
            .map(|(name, value)| {
                (
                    symbol("v", name),
                    match value {
                        Value::Bool(v) => SmtValue::Bool(*v),
                        Value::I64(v) => SmtValue::Int(i128::from(*v)),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        for (name, sort, body) in &definitions {
            let value = evaluate(body, &variables, 0)?;
            if !matches!(
                (sort, value),
                (Sort::Bool, SmtValue::Bool(_)) | (Sort::I64, SmtValue::Int(_))
            ) {
                return Err(AuditError::MalformedFormula("definition value sort".into()));
            }
            variables.insert(name.clone(), value);
        }
        let expected = expectation(&contract.requires, &contract.ensures, &values)?;
        if let Some(source) = source {
            let observed = source(&values)?;
            if let Some(index) = observed
                .iter()
                .zip(expected)
                .position(|(actual, expected)| *actual != expected)
            {
                return Err(AuditError::SourceMismatch {
                    obligation: OBLIGATION_NAMES[index],
                    values,
                });
            }
        }
        for (index, formula) in formulas.iter().enumerate() {
            let actual = bool_value(evaluate(formula, &variables, 0)?)?;
            if actual != expected[index] {
                return Err(AuditError::Mismatch {
                    obligation: OBLIGATION_NAMES[index],
                    values,
                });
            }
            audit.obligations[index] += usize::from(actual);
        }
        audit.cases += 1;
        if indices.is_empty() {
            break;
        }
        let mut position = indices.len();
        loop {
            position -= 1;
            indices[position] += 1;
            if indices[position] < choices[position].len() {
                break;
            }
            indices[position] = 0;
            if position == 0 {
                return Ok(audit);
            }
        }
    }
    Ok(audit)
}
