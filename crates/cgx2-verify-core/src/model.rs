use std::collections::BTreeMap;

use crate::smtlib::symbol;
use crate::{eval, Contract, Counterexample, EvalError, Failure, Sort, UnknownReason, Value};

fn tokens(input: &str) -> Result<Vec<&str>, UnknownReason> {
    if input.len() > crate::SOLVER_OUTPUT_LIMIT || !input.is_ascii() {
        return Err(UnknownReason::MalformedResponse);
    }
    let bytes = input.as_bytes();
    let mut result = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        if bytes[pos].is_ascii_whitespace() {
            pos += 1;
            continue;
        }
        let start = pos;
        if matches!(bytes[pos], b'(' | b')') {
            pos += 1;
        } else {
            while pos < bytes.len()
                && !bytes[pos].is_ascii_whitespace()
                && !matches!(bytes[pos], b'(' | b')')
            {
                pos += 1;
            }
        }
        result.push(&input[start..pos]);
        if result.len() > 512 {
            return Err(UnknownReason::MalformedResponse);
        }
    }
    Ok(result)
}

fn next<'a>(items: &'a [&str], pos: &mut usize) -> Result<&'a str, UnknownReason> {
    let value = items
        .get(*pos)
        .copied()
        .ok_or(UnknownReason::MalformedResponse)?;
    *pos += 1;
    Ok(value)
}

fn expect(items: &[&str], pos: &mut usize, expected: &str) -> Result<(), UnknownReason> {
    if next(items, pos)? == expected {
        Ok(())
    } else {
        Err(UnknownReason::MalformedResponse)
    }
}

fn parse_value(items: &[&str], pos: &mut usize, sort: Sort) -> Result<Value, UnknownReason> {
    let token = next(items, pos)?;
    match sort {
        Sort::Bool => match token {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(UnknownReason::MalformedResponse),
        },
        Sort::I64 => {
            let value = if token == "(" {
                expect(items, pos, "-")?;
                let magnitude = next(items, pos)?;
                if magnitude.is_empty() || !magnitude.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(UnknownReason::MalformedResponse);
                }
                let magnitude = magnitude
                    .parse::<i128>()
                    .map_err(|_| UnknownReason::MalformedResponse)?;
                expect(items, pos, ")")?;
                -magnitude
            } else {
                token
                    .parse::<i128>()
                    .map_err(|_| UnknownReason::MalformedResponse)?
            };
            Ok(Value::I64(
                i64::try_from(value).map_err(|_| UnknownReason::MalformedResponse)?,
            ))
        }
    }
}

/// Accept only a complete `get-value` answer, then check the original contract
/// with checked i64 evaluation. A fake solver can supply a witness, but it cannot
/// turn a nonviolating or out-of-range model into a counterexample.
pub fn validate_solver_witness(
    contract: &Contract,
    output: &str,
    failure: Failure,
) -> Result<Counterexample, UnknownReason> {
    if output.len() > crate::SOLVER_OUTPUT_LIMIT {
        return Err(UnknownReason::MalformedResponse);
    }
    contract
        .validate()
        .map_err(|error| UnknownReason::InvalidContract(format!("{error:?}")))?;
    if contract.requires.contains_uninterpreted() || contract.ensures.contains_uninterpreted() {
        return Err(UnknownReason::Unsupported("uninterpreted call".into()));
    }
    let (status, body) = output.split_once('\n').unwrap_or((output, ""));
    if status.trim() != "sat" {
        return Err(UnknownReason::MalformedResponse);
    }
    let items = tokens(body)?;
    let mut pos = 0;
    let mut values = BTreeMap::new();
    let names = contract
        .parameters
        .iter()
        .map(|p| (symbol("v", &p.name), p))
        .collect::<BTreeMap<_, _>>();
    if !contract.parameters.is_empty() {
        expect(&items, &mut pos, "(")?;
        for _ in &contract.parameters {
            expect(&items, &mut pos, "(")?;
            let encoded = next(&items, &mut pos)?;
            let parameter = names.get(encoded).ok_or(UnknownReason::MalformedResponse)?;
            let value = parse_value(&items, &mut pos, parameter.sort)?;
            expect(&items, &mut pos, ")")?;
            if values.insert(parameter.name.clone(), value).is_some() {
                return Err(UnknownReason::MalformedResponse);
            }
        }
        expect(&items, &mut pos, ")")?;
    }
    if pos != items.len() || values.len() != contract.parameters.len() {
        return Err(UnknownReason::MalformedResponse);
    }
    if eval(&contract.requires, &values) != Ok(Value::Bool(true)) {
        return Err(UnknownReason::Evaluation(
            "candidate does not satisfy requires".into(),
        ));
    }
    let expected = match failure {
        Failure::EnsuresFalse => Ok(Value::Bool(false)),
        Failure::Overflow => Err(EvalError::Overflow),
    };
    if eval(&contract.ensures, &values) != expected {
        return Err(UnknownReason::Evaluation(
            "candidate does not violate ensures".into(),
        ));
    }
    Ok(Counterexample { values, failure })
}
