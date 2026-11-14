// Context projection only. Kernel source is included unchanged from the pinned worktree.
#[derive(Clone, Copy)]
pub enum AllocationError { BudgetExceeded }
pub struct Session {
    budgets: Vec<budget::Budget>,
    charged_bytes_v2: u128,
}
#[path = "/home/john-riley/.codex/worktrees/github-main-consolidation/lang/crates/cgx2-runtime/src/budget.rs"]
mod budget;
