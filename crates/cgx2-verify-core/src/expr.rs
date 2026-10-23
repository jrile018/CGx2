use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Sort {
    Bool,
    I64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Expr {
    Bool(bool),
    I64(i64),
    Var(String),
    Add(Box<Self>, Box<Self>),
    Sub(Box<Self>, Box<Self>),
    Eq(Box<Self>, Box<Self>),
    Lt(Box<Self>, Box<Self>),
    Le(Box<Self>, Box<Self>),
    Gt(Box<Self>, Box<Self>),
    Ge(Box<Self>, Box<Self>),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
    Not(Box<Self>),
    /// A Boolean guard and two equal-sort branches; only the selected branch evaluates.
    If(Box<Self>, Box<Self>, Box<Self>),
    /// A foreign/unmodeled result. It is never evaluated or proved by the finite engine.
    Uninterpreted {
        name: String,
        sort: Sort,
        args: Vec<Self>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub sort: Sort,
}

impl Parameter {
    pub fn new(name: impl Into<String>, sort: Sort) -> Self {
        Self {
            name: name.into(),
            sort,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Contract {
    pub parameters: Vec<Parameter>,
    pub requires: Expr,
    pub ensures: Expr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidationError {
    DuplicateParameter(String),
    UnknownVariable(String),
    SortMismatch { expected: Sort, actual: Sort },
    ExpectedEqualSorts { left: Sort, right: Sort },
    InvalidName(String),
}

impl Contract {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut sorts = BTreeMap::new();
        for parameter in &self.parameters {
            if parameter.name.is_empty() {
                return Err(ValidationError::InvalidName(parameter.name.clone()));
            }
            if sorts
                .insert(parameter.name.as_str(), parameter.sort)
                .is_some()
            {
                return Err(ValidationError::DuplicateParameter(parameter.name.clone()));
            }
        }
        expect(self.requires.sort(&sorts)?, Sort::Bool)?;
        expect(self.ensures.sort(&sorts)?, Sort::Bool)?;
        Ok(())
    }
}

fn expect(actual: Sort, expected: Sort) -> Result<(), ValidationError> {
    if actual == expected {
        Ok(())
    } else {
        Err(ValidationError::SortMismatch { expected, actual })
    }
}

impl Expr {
    pub fn sort(&self, variables: &BTreeMap<&str, Sort>) -> Result<Sort, ValidationError> {
        match self {
            Self::If(guard, yes, no) => {
                expect(guard.sort(variables)?, Sort::Bool)?;
                let left = yes.sort(variables)?;
                let right = no.sort(variables)?;
                if left != right {
                    return Err(ValidationError::ExpectedEqualSorts { left, right });
                }
                Ok(left)
            }
            Self::Bool(_) => Ok(Sort::Bool),
            Self::I64(_) => Ok(Sort::I64),
            Self::Var(name) => variables
                .get(name.as_str())
                .copied()
                .ok_or_else(|| ValidationError::UnknownVariable(name.clone())),
            Self::Add(a, b) | Self::Sub(a, b) => {
                expect(a.sort(variables)?, Sort::I64)?;
                expect(b.sort(variables)?, Sort::I64)?;
                Ok(Sort::I64)
            }
            Self::Lt(a, b) | Self::Le(a, b) | Self::Gt(a, b) | Self::Ge(a, b) => {
                expect(a.sort(variables)?, Sort::I64)?;
                expect(b.sort(variables)?, Sort::I64)?;
                Ok(Sort::Bool)
            }
            Self::Eq(a, b) => {
                let left = a.sort(variables)?;
                let right = b.sort(variables)?;
                if left != right {
                    return Err(ValidationError::ExpectedEqualSorts { left, right });
                }
                Ok(Sort::Bool)
            }
            Self::And(a, b) | Self::Or(a, b) => {
                expect(a.sort(variables)?, Sort::Bool)?;
                expect(b.sort(variables)?, Sort::Bool)?;
                Ok(Sort::Bool)
            }
            Self::Not(a) => {
                expect(a.sort(variables)?, Sort::Bool)?;
                Ok(Sort::Bool)
            }
            Self::Uninterpreted { name, sort, args } => {
                if name.is_empty() {
                    return Err(ValidationError::InvalidName(name.clone()));
                }
                for arg in args {
                    arg.sort(variables)?;
                }
                Ok(*sort)
            }
        }
    }

    pub fn contains_uninterpreted(&self) -> bool {
        match self {
            Self::If(guard, yes, no) => {
                guard.contains_uninterpreted()
                    || yes.contains_uninterpreted()
                    || no.contains_uninterpreted()
            }
            Self::Uninterpreted { .. } => true,
            Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Eq(a, b)
            | Self::Lt(a, b)
            | Self::Le(a, b)
            | Self::Gt(a, b)
            | Self::Ge(a, b)
            | Self::And(a, b)
            | Self::Or(a, b) => a.contains_uninterpreted() || b.contains_uninterpreted(),
            Self::Not(a) => a.contains_uninterpreted(),
            _ => false,
        }
    }
}
