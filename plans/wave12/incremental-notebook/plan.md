# Retained-state pure scalar notebook execution

> Work in the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, start another chat, or automate follow-up.

**Goal:** A native `.cgnb` session should execute a later pure scalar cell from retained evaluator state without running earlier cells again. This prepares the architecture for side effects; this wave must continue rejecting side-effecting cells.

**Architecture:** Make the notebook `CellExecutor` retain checked scalar bindings and their defining cell IDs after each successful cell. `run_cell` evaluates only that cell. A selected rerun should invalidate and recompute affected definitions under a documented strategy without executing unaffected earlier cells; if the dependency graph cannot prove this safely, return a restart-required diagnostic rather than replaying hidden work. Keep the existing notebook dependency facts, saved-output provenance, stale statuses, and clean restart semantics. The accepted scalar language and arithmetic errors must agree with the frozen Wave 11 CLI. Do not duplicate mutable/foreign/memory execution semantics.

**Ownership:** Only `crates/cgx2-notebook-format/**`, `crates/cgx2-notebook-session/**`, `crates/cgx2-notebook-kernel/**`, `crates/cgx2-notebook-cli/**`, and `crates/cgx2-notebook-ui/**`. Read but do not edit `cgx2-cli`, source grammar/checker, runtime, root manifests/lock, docs or plans. No new API from the parallel source-native lane.

## Required behavior

1. Instrument the executor in tests: executing a later cell after two earlier successful cells must increment only that cell's execution count. An edit must mark direct/transitive readers stale, preserve cached old outputs as noncurrent, and never label an old result current. A selected rerun and Restart and Run All must have distinct, documented behavior. Save/reopen retains `.cgnb` text and provenance, but reopened outputs start noncurrent.
2. Preserve failed-cell atomicity: definitions from a failing cell do not commit, output remains cached/noncurrent, and retry is possible. Enforce the existing cell/source/session limits and account for new retained-state memory. Calls, I/O, Python, native bindings, memory operations, mutable values and unknown effects remain rejected before execution.
3. Exercise the notebook CLI and loopback UI: Run next, edit, selected rerun or explicit restart, save/reopen, and visible staleness. Keep existing host/origin/token and inert-output protections. Do not present this pure-scalar state as full Jupyter parity or persistent side-effect support.
4. Update owned READMEs with evaluator state lifetime, invalidation rules, limits and unsupported forms.

## Verification and handoff

Run `RUSTUP_TOOLCHAIN=1.98.1 LIBRARY_PATH=/tmp/cgx2-python-lib cargo test -p cgx2-notebook-format -p cgx2-notebook-session -p cgx2-notebook-kernel -p cgx2-notebook-cli --offline`; run `cgx2-notebook-ui` tests where loopback sockets are available; run format and targeted Clippy `-D warnings`. Commit owned paths, report SHA, execution-count evidence, UI checks and limits.

**Acceptance:** Ordinary later cell execution does not replay prior cells, and edits/reruns keep dependency and provenance state honest while effects stay outside the accepted subset.
