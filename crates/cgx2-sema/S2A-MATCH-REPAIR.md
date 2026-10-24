# S2a match control-flow repair evidence

Bounded match-flow delivery on `riley/s2a-match-flow-repair`, based on
`8902cbd92533b81e801bf2c5bea725f5508fe3e2`. This is repair evidence for review,
not S2b/S3 interface acceptance or a release gate result.

## Change and rationale

`src/check.rs::Checker::check_match` passes the enclosing function's expected
result through arm checking for explicit `Return` statements, but joins only
arms that can fall through. Those arm values must agree with each other; only a
function's final fallthrough value is checked against its result annotation.
A terminating arm contributes explicit returns, not a match fallthrough value.
An empty arm contributes `Unit` rather than silently bypassing agreement.

`Checker::check_inferred_result` now collects explicit returns recursively from
`Match` arms and existing `For` bodies as well as `If` branches. It joins these
with the function's fallthrough value when `block_returns` permits fallthrough.
All-return private helpers therefore infer their actual result, even when
unused. Existing conservative checking of explicit returns (including unreachable
ones), `If` statement tails as `Unit`, exhaustiveness/unreachable-arm diagnostics,
and the rejection of returns in range loops remain intact. List-loop returns
are checked under the existing semantic profile; this adds no execution support.

There are no public signature/type/value changes, dependency changes, emitter
edits, root Cargo changes, or writes to shared plans/ledgers/coordinator evidence.

## Regression evidence

`tests/match_control_flow.rs` has six tests covering non-final mutation and
Boolean-valued matches in an `i64` function, inferred and annotated partial
returns, nested match/if all-return inference, unused helpers, bare `return`
(`StmtKind::Return(None)`), no-value arms, list-loop inference/fallthrough,
range-loop rejection, annotated/inferred mismatches, coverage/unreachable arms,
and byte-valid diagnostic spans after UTF-8 comments. Empty suites are not source
syntax; an explicit AST regression checks that empty-arm values cannot bypass
semantic agreement.

Owned fixtures are under `tests/match_flow/`. `nontail.cg` and `inferred.cg` copy
the coordinator reproductions unchanged. Five valid fixtures each interpret as
`42` and execute as `42` after native builds in **both debug and release** (ten
new native executions). The all-return fixture also compiles unused inferred
`i64` and `Unit` helpers. The invalid inferred fixture receives source `CGT003`
from interpretation, product `check`/`run`/Rust emission, and compiler build/Rust
emission. No requested backend output is published, and native failure is not
`CGXC005`.

## Environment and commands

All commands ran from the attached worktree with existing offline dependencies:

```sh
export RUSTUP_TOOLCHAIN=1.98.1
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-s2a-match-flow"
export TMPDIR="$PWD/target/tmp-s2a-match-flow"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
```

Tool versions: `rustc 1.98.1 (48a229cea 2026-09-01)`;
`cargo 1.98.1 (797e8a9bc 2026-08-05)`.

| Command | Final result |
| --- | --- |
| `cargo test --offline --locked -p cgx2-sema` | 59 passed, 0 failed, 0 ignored, including six new tests and existing S2a tests |
| `cargo test --offline --locked -p cgx2-cli --test s2a_match_control_flow --test s2a` | 9 passed, 0 failed, 0 ignored |
| `cargo test --offline --locked -p cgx2c --test s2a_match_control_flow --test s2a_parity` | 5 passed, 0 failed, 0 ignored; native debug/release parity |
| `cargo clippy --offline --locked -p cgx2-sema --all-targets --all-features -- -D warnings` | passed |
| `cargo clippy --offline --locked -p cgx2-cli -p cgx2c --test s2a_match_control_flow --all-features -- -D warnings` | passed |
| `rustfmt --check --edition 2021 crates/cgx2-sema/src/check.rs crates/cgx2-sema/tests/match_control_flow.rs` | passed |
| `rustfmt --check --edition 2024 crates/cgx2-cli/tests/s2a_match_control_flow.rs crates/cgx2c/tests/s2a_match_control_flow.rs` | passed |
| `git diff --check` | passed |

Local logs are `target/match-{sema,cli,native,clippy,product-clippy}.log` (ignored,
not part of the delivery). Initial regression authoring exposed mandatory `else`
syntax and a fixture replacement mistake; both were corrected. Initial native
test setup reused output paths without `--force`, then placed that flag before
the required input argument; the final harness uses the supported flag order.
Those attempts failed and are not counted as passing verification.

## Limits

Verification is the specified bounded semantic/product/native regression work.
Full workspace integration and the concurrent native-record/API-text repairs
remain coordinator-owned. No external solver, leak/isolation/performance, model,
human study, or release acceptance claim follows from these fixture tests.
The pre-existing untracked `.serena/` directory is excluded from this commit.
