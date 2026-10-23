# Wave 25 conditional contract evidence

Implemented from `7f4c4525c61da165a4b628f34eb095521a14b349` in the attached
`/home/john-riley/.codex/worktrees/e84a/lang` worktree. Only the two verifier
crates are changed. The committed `Function`, scalar `ExprKind`, and
`StmtKind::If { condition, then_body, else_body }` interfaces are consumed
without changes to syntax, sema, runtime, shared plans, or root Cargo files.

## Supported and rejected profiles

Existing single scalar expressions/returns retain their semantics and original
expression-node accounting. Public explicit-pure `i64`/`Bool` functions may
add one complete mandatory-else conditional tree: each branch is exactly one
explicit scalar return or another complete conditional. Every guard is Bool
and both branches have equal result sorts. Source checking still enforces the
declared return type, effects, parameters and contracts. Both branches must be
admissible, including under constant guards.

`SOURCE_CONDITIONAL_DEPTH_LIMIT = 32` bounds nested conditional statements;
`SOURCE_NODE_LIMIT = 1024` includes every new conditional/branch statement,
every guard and both branches' expression nodes, and requires/ensures nodes.
The existing 65,536-byte input cap is unchanged. Tests accept 32 levels and an
exact 1,024-node balanced tree, independently audit both, and reject level 33
and a two-node increase over the node cap. Rejections are CGV001 with UTF-8
byte spans before solver launch. Parser-invalid input retains CGS diagnostics.

Rejected profiles include implicit branch tails, Unit returns, mismatched
sorts, non-Bool guards, local bindings, mutation, loops, ordinary/foreign calls,
records, memory calls, multiple statements, missing/empty else branches,
partial paths, multiplication/division and source Boolean logical syntax.
Finite domain, public-function, explicit-effects and requires/ensures
restrictions are preserved. Existing source operators have not been expanded.

## Semantics and oracle experiments

`Expr::If(Box<Expr>, Box<Expr>, Box<Expr>)` validates a Boolean guard and
equal branch sorts; `eval` evaluates its guard once, then just the chosen
branch. Walkers still inspect every child for uninterpreted calls. JSON uses
`{"op":"if","condition":EXPR,"then":EXPR,"else":EXPR}` and retains existing
field, sort, depth and node validation. Prior eager Boolean operators retain
their behavior.

SMT values use `ite`. Definedness is
`(and guard_defined (ite guard_term then_defined else_defined))`.
The four obligations remain requires-defined, requires-satisfiable,
ensures-false, ensures-overflow, in that order. i64 bounds and inconsistent
precondition handling are unchanged.

The independent source oracle executes the original parsed statements and
original requires/ensures; it does not call core evaluation or use lowered
results as expected values. Finite assignments cover nested decisions, both
result sorts/outcomes, true/false contracts, conflicting return values,
guard overflow and active/inactive leaf overflow. The following deliberate
faults are rejected by `source::tests`:

1. Replacing lowered ensures with `true`, then generating internally consistent
   SMT: core-vs-SMT audit passes but original-source audit returns SourceMismatch.
2. Swapping emitted `ite` branch terms: audit returns Mismatch.
3. Requiring both branches to be defined unconditionally: audit returns Mismatch
   on the inactive-overflow assignment.

Witness tests reject a claimed overflow with `flag=true, x=MAX` when the
overflowing leaf is inactive. They validate `flag=false, x=MAX` against both
core and original source, including when the witness lies outside the finite
audit domain. The transport script is an unpinned **mock**, used solely to
exercise replay and rejection; it never supplies trusted proof.

## Exact verification commands and results

Commands run from the attached worktree, using Rust 1.98.1. These private
paths prevent artifact collisions with concurrent chats:

```sh
export TMPDIR="$PWD/target/wave25-conditional-tmp"
export CARGO_TARGET_DIR="$PWD/target/wave25-conditional"
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
mkdir -p "$TMPDIR"
cargo +1.98.1 test -p cgx2-verify-core -p cgx2-verify-cli --offline -- --quiet
cargo +1.98.1 fmt -p cgx2-verify-core -p cgx2-verify-cli -- --check
cargo +1.98.1 clippy --offline -p cgx2-verify-core -p cgx2-verify-cli --all-targets -- -D warnings
cargo +1.98.1 test --offline -p cgx2-syntax -p cgx2-sema -p cgx2-cli --test conditionals
cargo +1.98.1 run --offline -p cgx2-cli --bin cgx2 -- check crates/cgx2-verify-cli/examples/conditional-run.cg
target/wave25-conditional/debug/cgx2 run crates/cgx2-verify-cli/examples/conditional-run.cg
target/wave25-conditional/debug/cgx2 check crates/cgx2-verify-cli/examples/conditional-bool.cg
target/wave25-conditional/debug/cgx2 check crates/cgx2-verify-cli/examples/conditional-overflow.cg
target/wave25-conditional/debug/cgx2 check crates/cgx2-verify-cli/examples/conditional-guard-overflow.cg
git diff --check
```

All final commands pass: **54 verifier tests**, **8 existing cross-component
conditional tests**, owned formatting, Clippy with warnings denied and diff
whitespace validation. The native CLI checks pass; the runnable nested example
prints `0` and exits zero. No opted-in Python bridge tests or linker workaround
were required. Logs are in
[`examples/conditional-evidence`](examples/conditional-evidence).

The initial build using `/tmp/cgx2-wave25-conditional-target` failed because
the worker's `/tmp` disk quota was exhausted. Retrying without debug info there
also failed. Removing only this task's failed target and placing target and
temporary artifacts under this worktree resolved the environmental failure.
An initial CLI build command without `--bin cgx2` was rejected as ambiguous;
the explicit binary command above passed. An old test asserting all
conditionals were unsupported was replaced with the false-branch witness test.

## Reproducible finite observations

After the verifier test build, each example was run with:

```sh
target/wave25-conditional/debug/cgx2-verify crates/cgx2-verify-cli/examples/conditional.cg --domain x:-1:1 --format json
target/wave25-conditional/debug/cgx2-verify crates/cgx2-verify-cli/examples/conditional-bool.cg --domain x:-1:1 --format json
target/wave25-conditional/debug/cgx2-verify crates/cgx2-verify-cli/examples/conditional-false.cg --format json
target/wave25-conditional/debug/cgx2-verify crates/cgx2-verify-cli/examples/conditional-inactive-overflow.cg --domain x:9223372036854775807:9223372036854775807 --format json
target/wave25-conditional/debug/cgx2-verify crates/cgx2-verify-cli/examples/conditional-overflow.cg --domain x:9223372036854775807:9223372036854775807 --format json
target/wave25-conditional/debug/cgx2-verify crates/cgx2-verify-cli/examples/conditional-guard-overflow.cg --domain x:9223372036854775807:9223372036854775807 --format json
```

| Example | Audited assignments | False postconditions | Overflow paths | Finite verdict | Separate proof |
| --- | ---: | ---: | ---: | --- | --- |
| conditional | 6 | 0 | 0 | proved | unknown |
| conditional-bool | 6 | 0 | 0 | proved | unknown |
| conditional-false | 2 | 1 | 0 | disproved | disproved |
| conditional-inactive-overflow | 1 | 0 | 0 | proved | unknown |
| conditional-overflow | 2 | 0 | 1 | disproved | disproved |
| conditional-guard-overflow | 2 | 0 | 2 | disproved | disproved |

Every row has independent source parity. Source hashes, emitted SMT hashes,
exact arguments and structured results are retained in
[`manifest.json`](examples/conditional-evidence/manifest.json); neighboring
`.smt2` files retain all four generated obligations. False/overflow proof
states here mean replayed finite counterexamples, not solver proofs.

## Live solver gate and limitations

`command -v z3 cvc5` found neither executable; `shutil.which` also records
null for both in the manifest. No solver was installed and no trusted digest
was selected. The genuine Z3/CVC5 valid/false/overflow proof gate is **unrun**.
The existing optional live-solver test returns early without configuration;
its test-run success is not live solver evidence. Executable identity/hash
and digest selection, exact four-line transcripts, precondition checks,
timeouts, stdout/stderr caps, memory limits and adversarial rejection tests
are preserved. Mocks exercise transport/rejection only.

This is finite evidence for the admitted pure scalar source contract. It is
not a general conditional verifier, compiler proof, source-memory proof,
all-i64 solver proof, or whole-language release gate. General calls, local
state, loops, records, branch tail values and richer source operators remain
outside the profile. Existing source/JSON/report compatibility is covered by
the prior regression suites; full workspace/release checks remain coordinator
work after review.
