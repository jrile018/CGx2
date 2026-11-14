use crate::{AllocationError, Session};

#[derive(Clone, Copy)]
pub(crate) struct Budget {
    pub limit: usize,
    pub spent: usize,
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
        self.charged_bytes_v2 = self.charged_bytes_v2.saturating_add(bytes as u128);
        for budget in &mut self.budgets {
            budget.spent += bytes;
        }
    }
}
