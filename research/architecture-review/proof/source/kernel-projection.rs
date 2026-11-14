// Context projection only. Kernel source is included unchanged from the pinned worktree.
#[derive(Clone, Copy)]
pub enum AllocationError { BudgetExceeded }
pub struct Session {
    budgets: Vec<budget::Budget>,
    charged_bytes_v2: u128,
}
#[path = "/tmp/cgx2-arch-review-proof/kernel-source.rs"]
mod budget;
