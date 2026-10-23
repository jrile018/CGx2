# `cgx2-verify`

The command checks a JSON contract or one selected `.cg` function. Finite
enumeration reports `verdict.kind: proved` only for the requested integer
domains and both values of each Boolean parameter. `proof` reports a separate
solver backed result for the selected contract obligation. It never certifies
every function or behavior in a source file.

```text
cgx2-verify CONTRACT.json [--format text|json] [--max-cases N] [--solver PATH] [--trusted-solver-sha256 HEX] [--timeout-ms N]
cgx2-verify FILE.cg [--function NAME] [--domain NAME:MIN:MAX]... [--format text|json] [--max-cases N] [--solver PATH] [--trusted-solver-sha256 HEX] [--timeout-ms N]
```

`--format` defaults to `text`. `--max-cases` defaults to 100,000 and accepts
1–1,000,000. Each `i64` parameter needs one inclusive `--domain`; Boolean
parameters need none. `--timeout-ms` defaults to 2,000 and accepts 1–10,000.
This is a deadline **per solver process** (version, obligation, each fixture,
and each witness query). Input is capped at 65,536 bytes. Invalid input exits
2; a valid report, including `unknown`, exits 0.

## Result states and scope

`proof.state` is `proven`, `counterexample`, `unknown`, `unsupported`, or
`solver_failure`. The older `proof.kind` is retained: `proved`, `disproved`,
or `unknown`. `proven` requires a supported selected `.cg` function, source and
SMT term parity for the checked finite audit, four complete solver answers,
passing true/false/overflow probes, a recognized Z3/CVC5 version, a stable
executable digest, and a digest matching `--trusted-solver-sha256`. The four
answers must be `unsat/sat/unsat/unsat`. The caller is responsible for trusting
the pinned executable; a filename, version banner, hash, and three probes do
not independently establish solver soundness. `proof.scope` names the selected
function and source span, input and obligation SHA-256 hashes, and finite audit
bounds. The result assumes the emitted SMT semantics for this pure scalar
fragment. It says nothing about unselected functions, arbitrary control flow,
memory, foreign calls, concurrency, or the whole language.

`counterexample` requires a violation re-evaluated by the core evaluator and,
for `.cg`, the source evaluator. Solver SAT or a model alone is only a
candidate. `unknown` covers an unrun solver, solver `unknown`, timeout,
inconsistent preconditions, and uncompleted obligations. `unsupported`
covers rejected source constructs and uninterpreted calls. Malformed or
contradictory output, nonzero exit, unavailable executable, excessive output,
invalid version, failed fixtures, and a wrong digest produce `solver_failure`
when no independent counterexample already settles the obligation. An
unpinned but otherwise valid solver observation remains `unknown`.

The selected source function must be public and semantically checked, declare
`effects()`, use `i64`/`Bool` parameters and return type, have exactly one
`requires` and one `ensures`, and have one pure expression body, return, or the bounded conditional tree described below.
Supported source expressions are literals, parameters, `result` in `ensures`,
checked `+`/`-`/`*`/`/`, and `==`, `!=`, `<`, `<=`, `>`, `>=`. The source parser does
not expose Boolean `and`/`or`/`not` syntax, though JSON/core contracts do.
Calls, `assert`, source mutation/loops, arrays/invariants, variant/generic types, foreign declarations, and
effects report unsupported source with a zero based, half open byte span.
Selecting one supported function from a larger file never checks the other
functions. Source reports use schema version 3; JSON reports use version 2.

## Solver process and reproduction

`--solver PATH` accepts an executable named `z3` or `cvc5` for a proof
candidate. The result records its canonical command, arguments, version,
executable hash, obligation hash, exit, stdout/stderr, fixture and witness
observations, and per process timeout. Stdout and stderr each have a 16,384
byte cap. On Linux each child has a 536,870,912 byte address space limit.
The result does not promote a truncated, extra, or inconsistent transcript.

```sh
cargo run --offline -p cgx2-verify-cli -- crates/cgx2-verify-cli/examples/finite.cg --domain x:0:2 --format json
SOLVER=$(command -v z3)
DIGEST=$(sha256sum "$SOLVER" | cut -d' ' -f1)
cargo run --offline -p cgx2-verify-cli -- crates/cgx2-verify-cli/examples/finite.cg --domain x:0:2 --format json --solver "$SOLVER" --trusted-solver-sha256 "$DIGEST"
cargo run --offline -p cgx2-verify-cli -- crates/cgx2-verify-cli/examples/false.cg --domain x:0:2 --format json --solver "$SOLVER" --trusted-solver-sha256 "$DIGEST"
```

Use `cvc5` in place of `z3` when available. The live solver gate was unrun
on the Wave 18 proof boundary host because neither Z3 nor CVC5 was installed.
Mock executables in the tests exercise process handling and rejection only.
The finite encoding audit fixture and its separate evidence remain in
[`examples/proof-encoding-audit/manifest.json`](examples/proof-encoding-audit/manifest.json).

```sh
cargo test --offline -p cgx2-verify-core -p cgx2-verify-cli
cargo fmt --all -- --check
cargo clippy --offline -p cgx2-verify-core -p cgx2-verify-cli --all-targets -- -D warnings
```

## Return-only conditional trees (Wave 25 historical baseline)

A body may contain exactly one mandatory-else `if`. Each branch contains
exactly one explicit scalar `return expression` or another such conditional.
Both branches are checked, even under a literal guard. Guards are Boolean;
leaves have the same result sort and must match the declared `i64` or `Bool`.
Wave 25 rejected locals and multiplication/division. The scalar v2 profile below
adds immutable locals and checked multiplication/division. Mutation, source loops,
calls, records, Unit returns, implicit branch tails, partial paths and source memory
remain unsupported. Source logical syntax is unchanged.

At most **32 nested conditionals** and **1,024 source nodes** are accepted.
The counter covers conditional/branch statements, every guard and every leaf
expression, plus requires/ensures expressions. Legacy single-expression/return
bodies retain their previous expression-only accounting. The source byte cap
remains 65,536. Unsupported or over-limit parsed trees return `CGV001` with
a zero-based half-open UTF-8 byte span before solver launch; malformed source
retains parser diagnostics. No branch pruning bypasses these checks.

Core `Expr::If(condition, then, else)` evaluates its guard once and executes
only the selected branch. SMT uses `ite` for the value and
`guard_defined AND ite(guard_term, then_defined, else_defined)` for
definedness. Guard/selected-leaf overflow is a violation; an inactive overflow
is not. The independent oracle executes the original `StmtKind::If` AST and
compares all four obligations against core and emitted SMT for each finite
assignment. It also replays solver witness candidates against original source.
Inconsistent preconditions remain unknown. Finite success never supplies a
trusted live proof.

JSON/core also accepts conditionals of either result sort:
`{"op":"if","condition":EXPR,"then":EXPR,"else":EXPR}`.
Unknown fields, non-Boolean guards and mixed branch sorts reject. Existing JSON
depth/node limits and Boolean eager `and`/`or` semantics are unchanged.

```sh
cargo +1.98.1 run --offline -p cgx2-verify-cli -- crates/cgx2-verify-cli/examples/conditional.cg --domain x:-1:1 --format json
cargo +1.98.1 run --offline -p cgx2-verify-cli -- crates/cgx2-verify-cli/examples/conditional-overflow.cg --domain x:9223372036854775807:9223372036854775807 --format json
```

Exact Wave 25 checks, corruption experiments, supported/rejected profiles and
the unrun live-solver gate are recorded in [wave25-evidence.md](wave25-evidence.md).


## Useful scalar profile v2 (release completion S1-09)

The CLI now uses the additive `cgx2_verify_core::scalar_v2` API. The original
root `Expr`/`Contract`/result types remain unchanged. Reports retain their schema
versions and include `scalar_profile.version: 2`; finite result scope and solver
assumptions name checked add/sub/mul/div.

Immutable `let` bindings may precede the terminal expression/return/conditional,
including inside either branch. Initializers evaluate eagerly, even if the local
is unused. Branch locals have lexical scopes; shadowing an outer parameter in a
branch is supported. Same-scope duplicate names still fail shared type checking.
The existing 1,024-node source budget includes binding nodes and initializers;
conditional nesting remains capped at 32. Both branches are validated before a
solver starts. An unselected failing initializer is inactive at evaluation time.

Multiplication checks the signed i64 range. Division truncates toward zero;
zero divisors and `MIN / -1` are undefined arithmetic. For compatibility, reports
use the existing `Overflow` failure and `ensures_overflow` obligation for these
invalid divisions. The SMT model uses mathematical integer multiplication,
explicit range/nonzero guards, and sign-adjusted `div` over absolute values.
The independent SMT interpreter supports shared definitions, SMT `let`, `*` and `div`.
JSON adds `mul`/`div` with `left`/`right` operands; lexical binding is a v2 library
API and source feature, without a new JSON binding syntax.

`--max-expansion N` accepts an explicit 1..=64 budget (default 0, expansion
disabled). It preflights the narrow literal accumulator pattern before solver
startup and charges all branches against one budget. **Source accumulator
verification remains blocked**: `effects()` produces shared checker `CGT012`
for `+=` (`Mutate`), while this pure verifier rejects `effects(Mutate)`.
No checker rules are bypassed. `scalar_v2::LiteralAccumulator::expand(N)` is
library-only: one initial value, a literal half-open range, one checked additive
update, and a result. General mutation, nested/general loops, array bounds and
annotated invariants require later shared syntax/core work. Variants and generics
are rejected rather than assumed safe.

Defaults remain `--max-cases 100000`, `--timeout-ms 2000` **per solver process**,
and 65536 input bytes. Existing configurable caps remain 1000000 cases and 10000
milliseconds. A finite audit is required for a trusted proof; its unsupported
SMT operations, i128 interpreter overflow, size/depth bounds or mismatches leave
proof unknown/failing. These finite checks never grant a global proof alone.

### Pinned live solver acceptance

Python 3.11+ and an independently trusted installed Z3/CVC5 executable are required.
Supply a previously trusted digest; computing a digest does not establish trust.
No solver is installed or downloaded by this runner. Use a fresh evidence directory.

```sh
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-verification"
export TMPDIR="$PWD/target/tmp-verification"
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo build --offline -p cgx2-verify-cli
python3 crates/cgx2-verify-cli/scripts/accept-scalar-v2.py \
  --verifier "$CARGO_TARGET_DIR/debug/cgx2-verify" \
  --solver /trusted/path/z3 --sha256 YOUR_PREVIOUSLY_TRUSTED_SHA256 \
  --output crates/cgx2-verify-cli/acceptance-run
```

Replace `z3` with `cvc5` for that solver. The eight-case suite saves source and
SMT hashes, full JSON reports with executable/version/pin/probe observations,
raw stdout/stderr, solver answers and validated witness attempts. It requires
actual complete solver obligations for each case; finite counterexamples cannot
substitute for live witnesses. An unavailable solver/pin exits 3 and writes
`blocked/unrun`; failures exit 1; acceptance exits 0 only for the supplied trusted
executable. Accumulator source admission stays a separate blocker even when this
scalar gate succeeds. See [release evidence](release-completion-evidence/report.md).

### SMT construction budgets

Scalar v2 SMT scripts use shared value/definedness definitions rather than
expanding child strings. The fixed CLI budgets are 4,096 combined core expression
nodes and parameters, depth 128, and 1 MiB per complete SMT script; total
identifier bytes must fit half that byte budget before hex encoding. Existing
65,536-byte input, 1,024-source-node and 32-conditional-depth limits also apply.
The core library offers smaller explicit `SmtLimits` budgets. Main emission
runs before finite verification or any solver process. An emission error reports
`CGV003`, `proof.kind=unknown`, `proof.state=unsupported`, an unrun encoding audit,
and `solver.launched=false`; no partial SMT is sent. Fixture-probe or witness
emission errors are recorded as invalid/unknown and never panic or launch that
query. The independent SMT interpreter evaluates the emitted definitions and
assertions on every finite tuple, including original-source parity for `.cg`.
