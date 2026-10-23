use super::Expr;

pub const EXPANSION_CAP: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpansionError {
    InvalidBound,
    InvalidBinding,
    LimitExceeded { iterations: i128, limit: usize },
}

/// Library-only literal accumulator. Source admission depends on the shared
/// effect checker; expanding a tree never grants source verification.
pub struct LiteralAccumulator {
    pub name: String,
    pub initial: Expr,
    pub index: String,
    pub start: i64,
    pub end: i64,
    pub increment: Expr,
    pub result: Expr,
}

impl LiteralAccumulator {
    /// Bound is mandatory, cumulative expansion is the caller's responsibility.
    /// The iteration binding is scoped to each initializer, so it cannot capture
    /// an outer parameter used by `result`. Initializers evaluate eagerly.
    pub fn expand(self, max_expansion: usize) -> Result<Expr, ExpansionError> {
        if !(1..=EXPANSION_CAP).contains(&max_expansion) {
            return Err(ExpansionError::InvalidBound);
        }
        if self.name.is_empty() || self.index.is_empty() || self.name == self.index {
            return Err(ExpansionError::InvalidBinding);
        }
        let iterations = (i128::from(self.end) - i128::from(self.start)).max(0);
        if iterations > max_expansion as i128 {
            return Err(ExpansionError::LimitExceeded {
                iterations,
                limit: max_expansion,
            });
        }
        let mut result = self.result;
        for index in (self.start..self.end).rev() {
            let sum = Expr::Let(
                self.index.clone(),
                Box::new(Expr::I64(index)),
                Box::new(Expr::Add(
                    Box::new(Expr::Var(self.name.clone())),
                    Box::new(self.increment.clone()),
                )),
            );
            result = Expr::Let(self.name.clone(), Box::new(sum), Box::new(result));
        }
        Ok(Expr::Let(
            self.name,
            Box::new(self.initial),
            Box::new(result),
        ))
    }
}
