use std::collections::{BTreeMap, BTreeSet};

use super::{eval, Contract, EvalError, Sort, Value};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Domain {
    pub name: String,
    pub min: i64,
    pub max: i64,
}

impl Domain {
    pub fn new(name: impl Into<String>, min: i64, max: i64) -> Self {
        Self {
            name: name.into(),
            min,
            max,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Scope {
    pub domains: Vec<Domain>,
    pub boolean_parameters: Vec<String>,
    pub cases: usize,
    pub assumptions: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    EnsuresFalse,
    Overflow,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Counterexample {
    pub values: BTreeMap<String, Value>,
    pub failure: Failure,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UnknownReason {
    InvalidContract(String),
    EmptyDomain(String),
    CaseCap { max_cases: usize },
    InconsistentPrecondition,
    Unsupported(String),
    Evaluation(String),
    Timeout,
    SolverUnavailable,
    MalformedResponse,
    SolverUnknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Verdict {
    Proved { scope: Scope },
    Disproved { counterexample: Counterexample },
    Unknown { reason: UnknownReason },
}

struct Search<'a> {
    contract: &'a Contract,
    domains: &'a BTreeMap<&'a str, &'a Domain>,
    values: BTreeMap<String, Value>,
    visited: usize,
    admitted: usize,
    cap: usize,
}

pub fn verify_bounded(contract: &Contract, domains: &[Domain], max_cases: usize) -> Verdict {
    if let Err(error) = contract.validate() {
        return Verdict::Unknown {
            reason: UnknownReason::InvalidContract(format!("{error:?}")),
        };
    }
    if contract.requires.contains_uninterpreted() || contract.ensures.contains_uninterpreted() {
        return Verdict::Unknown {
            reason: UnknownReason::Unsupported("uninterpreted call".into()),
        };
    }
    let mut domain_map = BTreeMap::new();
    for domain in domains {
        if domain.min > domain.max {
            return Verdict::Unknown {
                reason: UnknownReason::EmptyDomain(domain.name.clone()),
            };
        }
        if domain_map.insert(domain.name.as_str(), domain).is_some() {
            return Verdict::Unknown {
                reason: UnknownReason::InvalidContract(format!(
                    "duplicate domain: {}",
                    domain.name
                )),
            };
        }
    }
    let mut product = 1_u128;
    let mut bools = Vec::new();
    let mut names = BTreeSet::new();
    for parameter in &contract.parameters {
        names.insert(parameter.name.as_str());
        let size = match parameter.sort {
            Sort::Bool => {
                if domain_map.contains_key(parameter.name.as_str()) {
                    return Verdict::Unknown {
                        reason: UnknownReason::InvalidContract(format!(
                            "Boolean domain must be implicit: {}",
                            parameter.name
                        )),
                    };
                }
                bools.push(parameter.name.clone());
                2_u128
            }
            Sort::I64 => match domain_map.get(parameter.name.as_str()) {
                Some(domain) => (i128::from(domain.max) - i128::from(domain.min) + 1) as u128,
                None => {
                    return Verdict::Unknown {
                        reason: UnknownReason::InvalidContract(format!(
                            "missing domain: {}",
                            parameter.name
                        )),
                    }
                }
            },
        };
        product = match product.checked_mul(size) {
            Some(value) => value,
            None => {
                return Verdict::Unknown {
                    reason: UnknownReason::CaseCap { max_cases },
                }
            }
        };
    }
    for name in domain_map.keys() {
        if !names.contains(name) {
            return Verdict::Unknown {
                reason: UnknownReason::InvalidContract(format!("unknown domain: {name}")),
            };
        }
    }
    let mut search = Search {
        contract,
        domains: &domain_map,
        values: BTreeMap::new(),
        visited: 0,
        admitted: 0,
        cap: max_cases,
    };
    if let Some(verdict) = search.walk(0) {
        return verdict;
    }
    if search.admitted == 0 {
        return Verdict::Unknown {
            reason: UnknownReason::InconsistentPrecondition,
        };
    }
    let mut ordered_domains = Vec::new();
    for parameter in &contract.parameters {
        if let Some(domain) = domain_map.get(parameter.name.as_str()) {
            ordered_domains.push((*domain).clone());
        }
    }
    Verdict::Proved {
        scope: Scope {
            domains: ordered_domains,
            boolean_parameters: bools,
            cases: search.visited,
            assumptions: vec![
                "all listed finite inputs enumerated".into(),
                super::ARITHMETIC_ASSUMPTION.into(),
            ],
        },
    }
}

impl Search<'_> {
    fn walk(&mut self, index: usize) -> Option<Verdict> {
        if index == self.contract.parameters.len() {
            return self.visit();
        }
        let parameter = &self.contract.parameters[index];
        match parameter.sort {
            Sort::Bool => {
                for value in [false, true] {
                    self.values
                        .insert(parameter.name.clone(), Value::Bool(value));
                    if let Some(result) = self.walk(index + 1) {
                        return Some(result);
                    }
                }
            }
            Sort::I64 => {
                let domain = self.domains[parameter.name.as_str()];
                let mut value = domain.min;
                loop {
                    self.values
                        .insert(parameter.name.clone(), Value::I64(value));
                    if let Some(result) = self.walk(index + 1) {
                        return Some(result);
                    }
                    if value == domain.max {
                        break;
                    }
                    value += 1; // value < max, so this cannot overflow
                }
            }
        }
        None
    }

    fn visit(&mut self) -> Option<Verdict> {
        if self.visited >= self.cap {
            return Some(Verdict::Unknown {
                reason: UnknownReason::CaseCap {
                    max_cases: self.cap,
                },
            });
        }
        self.visited += 1;
        match eval(&self.contract.requires, &self.values) {
            Ok(Value::Bool(false)) => return None,
            Ok(Value::Bool(true)) => self.admitted += 1,
            Ok(_) => {
                return Some(Verdict::Unknown {
                    reason: UnknownReason::InvalidContract("requires is not Boolean".into()),
                })
            }
            Err(error) => {
                return Some(Verdict::Unknown {
                    reason: UnknownReason::Evaluation(format!("requires: {error:?}")),
                })
            }
        }
        let failure = match eval(&self.contract.ensures, &self.values) {
            Ok(Value::Bool(true)) => return None,
            Ok(Value::Bool(false)) => Failure::EnsuresFalse,
            Err(EvalError::Overflow) => Failure::Overflow,
            other => {
                return Some(Verdict::Unknown {
                    reason: UnknownReason::Evaluation(format!("ensures: {other:?}")),
                })
            }
        };
        // Re-evaluate the witness before returning it; a solver candidate follows the same rule.
        if eval(&self.contract.requires, &self.values) != Ok(Value::Bool(true)) {
            return Some(Verdict::Unknown {
                reason: UnknownReason::Evaluation("counterexample failed validation".into()),
            });
        }
        let validated = match failure {
            Failure::EnsuresFalse => {
                eval(&self.contract.ensures, &self.values) == Ok(Value::Bool(false))
            }
            Failure::Overflow => {
                eval(&self.contract.ensures, &self.values) == Err(EvalError::Overflow)
            }
        };
        if !validated {
            return Some(Verdict::Unknown {
                reason: UnknownReason::Evaluation("counterexample failed validation".into()),
            });
        }
        Some(Verdict::Disproved {
            counterexample: Counterexample {
                values: self.values.clone(),
                failure,
            },
        })
    }
}
