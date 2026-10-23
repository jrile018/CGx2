# S1-09 useful scalar proof profile

Reviewed base: `0f0b0b9c87d817f39830f3e2b311dbae2ae3e0f3`.
Branch: `riley/release-verification`. Host date: 2026-10-23.
All changes are inside `crates/cgx2-verify-core/**` and
`crates/cgx2-verify-cli/**`. No workspace manifests, dependency versions, root
lockfiles, shared checker rules, coordinator plans or other lanes were changed.

## Delivered scope

| Surface | Implemented and checked | Boundary |
| --- | --- | --- |
| Original core API | Existing root enums, types and signatures preserved; regression tests pass | Original checked add/sub profile remains unchanged |
| Additive core API | `scalar_v2::{Expr, Contract, eval, verify_bounded, emit_smtlib, audit_encoding_with_source, validate_solver_witness}`; `Expr::Mul`, `Div`, eager lexical `Let` | Frozen enums were not extended; protocol, finite search and witness implementations share source between API versions |
| `.cg` selected-function verification | Immutable locals before terminal returns/conditionals, local scopes and branch shadowing, checked multiplication and division | Public pure functions with annotated i64/Bool parameters/results, one requires and ensures; no calls, records, arrays, variants, generics, invariants, memory or general control flow |
| JSON | Additive `mul` and `div` binary operators; existing forms unchanged | No JSON `let` syntax; JSON alone still cannot receive a trusted source proof |
| Literal accumulator | `scalar_v2::LiteralAccumulator { name, initial, index, start, end, increment, result }.expand(max_expansion)`; half-open literal range, checked `+=`, eager initial value, scoped index | Library-only; mandatory explicit bound 1..=64; individual builders require caller accounting for cumulative expansion |
| Accumulator source preflight | `--max-expansion 1..64`, default 0; cumulative budget includes inactive branches, node budget checked before building expanded tree/starting solver | **Source admission blocked**, even within the bound; no shared checker bypass |
| Acceptance runner | Python standard library runner for previously trusted pinned Z3/CVC5, eight source cases, executable/version/SMT hashes, raw reports, probes, answers and validated witnesses | **Live solver blocked/unrun** on this host |

Multiplication uses checked i64 arithmetic. Division truncates toward zero;
division by zero and `MIN / -1` are definedness failures. For compatibility they
retain the existing `Overflow` error/failure and `ensures_overflow` obligation
labels. Initializers execute even when unused; failing initializers in inactive
branches do not execute. SMT models mathematical integers, range/nonzero guards,
lexical `let`, and division over absolute magnitudes with sign adjustment.
V2 rejects mixing lexical bindings with uninterpreted functions rather than
emitting free local names in global external-result assumptions.

Defaults remain 100000 finite cases, 2000 ms **per solver process**, and 65536
input bytes. Existing configurable caps remain 1000000 cases/10000 ms. Source
node limit is 1024, conditional nesting limit 32, explicit expansion cap 64.
Existing independent SMT audit size/token/depth limits and checked i128
interpreter arithmetic remain conservative limits: an unsupported audit or
intermediate i128 overflow does not grant proof. Complex nested overflowing
mathematical terms can therefore remain unknown rather than yield a checked
counterexample through this audit. Finite success is not a global proof.

## Verification

Environment: `rustc 1.98.1 (48a229cea 2026-09-01)`, Python 3.14.4,
`CARGO_BUILD_JOBS=2`, private target and temporary directories below. No
additional packages or host-policy changes were needed.

```sh
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-verification"
export TMPDIR="$PWD/target/tmp-verification"
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo test --offline -p cgx2-verify-core -p cgx2-verify-cli --all-features -- --test-threads=2
cargo clippy --offline -p cgx2-verify-core -p cgx2-verify-cli --all-targets --all-features -- -D warnings
cargo fmt -p cgx2-verify-core -p cgx2-verify-cli -- --check
python3 crates/cgx2-verify-cli/scripts/test_accept_scalar_v2.py
git diff --check
```

Results: **64 Rust tests pass**, two Python runner preflight tests pass, Clippy
with warnings denied passes, owned formatting and diff checks pass. The
pre-existing optional `CGX2_LIVE_SOLVER` test returns without running live
acceptance when the variable is absent; its test-harness success is not live
solver evidence. No timing benchmarks were run.

Coverage includes a 12×12×2 boundary grid (288 multiplication/division cases)
against Rust checked arithmetic and the independent SMT interpreter, signed
rounding, zero, MIN/-1, unused failing locals, inactive branches, lexical
shadowing, false/valid contracts, exact-cap 64-iteration expansion, empty/reversed
ranges, preallocation expansion rejection, cumulative inactive-branch budgets,
library-only expansion parity against original source AST replay, and source
oracle rejection of self-consistent corrupted lowering that deletes an unused
division failure. Operator/binding SMT corruption is detected. Existing
forged/partial/malformed output, stale witnesses, unpinned solver, timeout,
output-limit and process-handling tests now exercise the CLI through v2.

One earlier full run had an existing timeout test observe `unavailable` instead
of `timeout`. The same targeted test passed without a code change, then the
complete suite passed with two test threads. The failed observation and targeted
rerun are retained in `tests-transient-failure.txt` and
`timeout-targeted-rerun.txt`; no stronger host/process reliability claim follows.

## External and shared blockers

**Accumulator source admission:** the owned example
`examples/scalar-v2/accumulator.cg` with `--max-expansion 3` fails shared semantic
checking with `CGT012: effect Mutate is not permitted by explicit declaration`.
Changing to `effects(Mutate)` fails this verifier's pure-function admission.
With no bound or bound 2 it fails owned preflight with CGV001 before a solver
starts. A later shared core/effect contract must distinguish local accumulator
mutation from externally effectful behavior before this profile can be admitted.
The isolated AST parity test is explicitly library-only evidence and does not
represent a successful checked source proof.

**Array/invariant/new-type proofs:** shared S2 syntax/types and S3 obligation
design are required. These remain rejected. No array-bound or annotated-invariant
proof is claimed by the scalar profile.

**Trusted live solver:** neither Z3 nor CVC5 is installed on this host. Running
the acceptance runner with installed-solver discovery produced exit **3** and
[`live-solver/acceptance.json`](live-solver/acceptance.json): `blocked/unrun`, zero
cases, no executable/pin/version/results/witnesses. Mock process fixtures are
protocol tests only. No live proof or genuine solver acceptance is claimed.

Operator handoff: provide an independently trusted installed Z3/CVC5 and its
previously trusted SHA-256; build the verifier using the environment above, then
run the following with a **fresh** evidence directory:

```sh
cargo build --offline -p cgx2-verify-cli
python3 crates/cgx2-verify-cli/scripts/accept-scalar-v2.py \
  --verifier "$CARGO_TARGET_DIR/debug/cgx2-verify" \
  --solver /trusted/path/z3 --sha256 YOUR_PREVIOUSLY_TRUSTED_SHA256 \
  --output crates/cgx2-verify-cli/acceptance-run
```

The runner accepts `cvc5` as well. It exits 3 for unavailable executable/pin,
1 for acceptance failure and 0 only for full acceptance under operator trust.
It requires complete expected live answers, stable matching hashes, valid
version/probes, source parity, and genuine validated live witness attempts for
false/undefined cases. Finite counterexamples cannot satisfy the live gate.
Computing a digest or passing probes alone never attests solver soundness.

Raw successful test/Clippy logs and runner preflight logs accompany this report.
`manifest.json` records fixture hashes and the exact source/library distinction.
