use std::ops::{Deref, DerefMut};

use crate::{AllocationError, Session};

#[derive(Clone, Copy)]
pub(crate) struct Budget {
    pub limit: usize,
    pub spent: usize,
}

/// Suppresses collection for the whole session until dropped.
/// Nested guards charge each successful allocation to every active budget.
pub struct NoGcGuard<'a> {
    pub(crate) session: &'a mut Session,
}

impl Deref for NoGcGuard<'_> {
    type Target = Session;

    fn deref(&self) -> &Self::Target {
        self.session
    }
}

impl DerefMut for NoGcGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.session
    }
}

impl Drop for NoGcGuard<'_> {
    fn drop(&mut self) {
        self.session.budgets.pop();
    }
}

impl Session {
    pub(crate) fn check_budgets(&self, bytes: usize) -> Result<(), AllocationError> {
        for budget in &self.budgets {
            if bytes > budget.limit.saturating_sub(budget.spent) {
                return Err(AllocationError::BudgetExceeded);
            }
        }
        Ok(())
    }

    pub(crate) fn charge_budgets(&mut self, bytes: usize) {
        for budget in &mut self.budgets {
            budget.spent += bytes;
        }
    }
}
