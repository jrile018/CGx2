# Stage 1 review repair: bounded scalar v2 SMT emission

Repair candidate for coordinator review, 2026-10-23. Base commit:
`2b9855c48713794d4cf819bc391813c1260f287b`; branch `riley/repair-verifier-size`.
Only `crates/cgx2-verify-core/**` and `crates/cgx2-verify-cli/**` are changed.
No root Cargo files, shared plans, parser/checker, legacy root verifier APIs,
other lanes, merge, installation, or Stage 2 work are included.

## Change and limits

The scalar v2 emitter now writes fresh dependency-ordered, zero-argument
`define-fun` bindings for values and definedness. Child terms are referenced
rather than copied recursively. Lexical environments map locals to initializer
references; shadowing, eager unused initializers, selected branch definedness,
checked arithmetic and truncating signed division are preserved. Parameter,
internal, and UF namespaces cannot capture each other. Global UF range
assumptions and conservative rejection of locals combined with UF remain.

Main and witness APIs preserve their signatures. Additive APIs are
`SmtLimits`, `check_smt_limits`, `emit_smtlib_with_limits`, and
`emit_witness_smtlib_with_limits`. Defaults/hard maxima are **4,096 combined
expression nodes and parameters, depth 128 (root one), 1,048,576 emitted bytes**.
Callers may choose smaller positive budgets only. Aggregate identifier UTF-8
bytes across parameter and expression occurrences must fit `output_bytes / 2`
before hex encoding. Shape is checked iteratively before recursive validation.
A bounded `fmt::Write` checks each fragment before allocation/copy, including
identifier encoding, definitions, declarations, assertions and witness names.
No partial script is returned; errors identify resource and limit.

Arithmetic parents of undefined children use a dummy zero term and false
propagated definedness. This prevents inactive chains from producing unbounded
mathematical intermediates in the finite interpreter. No undefined source
operation becomes valid. Division by zero and MIN / -1 still fail; signed
quotients truncate toward zero. The complete boundary grid and eager/lazy
regressions retain actual SMT interpretation.

The independent audit parses the emitted definitions and four assertions; it
does not re-emit the contract or skip formulas. Definitions are evaluated once
per tuple in dependency order, with sort/duplicate/forward-reference checks.
Source callbacks still evaluate the original checked AST separately. Whole
script cap is 1 MiB; per-term caps remain 256 KiB / 32,768 tokens / depth 256.
Audit input limit errors are `InvalidInput`; malformed scripts fail closed.
CLI emission now precedes finite verification and all solver launches. Errors
produce CGV003 with unknown/unsupported proof, unrun audit and
`solver.launched=false`. Probe/catalog/witness fallible emission no longer uses
`expect`; query failure records invalid/unknown without launching that query.

## Verification

Commands run from this attached worktree with:

```sh
export RUSTUP_TOOLCHAIN=1.98.1
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR=$PWD/target/repair-verifier-size
export TMPDIR=$PWD/target/tmp-verifier-size
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo test -p cgx2-verify-core -p cgx2-verify-cli --all-features --offline
cargo clippy -p cgx2-verify-core -p cgx2-verify-cli --all-targets --all-features --offline -- -D warnings
cargo fmt -p cgx2-verify-core -p cgx2-verify-cli --check
git diff --check
python3 -m unittest discover -s crates/cgx2-verify-cli/scripts -p test_accept_scalar_v2.py
python3 crates/cgx2-verify-cli/verifier-size-evidence/reproduce.py "$CARGO_TARGET_DIR/debug/cgx2-verify"
cargo test -p cgx2-verify-core --test scalar_v2_size nested_division --offline -- --nocapture
python3 crates/cgx2-verify-cli/scripts/accept-scalar-v2.py --verifier "$CARGO_TARGET_DIR/debug/cgx2-verify" --output /NEW/OUTPUT/DIRECTORY
```

- Rust: **73 passed, 0 failed, 0 ignored**, including **9 new regressions**
  (8 core + 1 CLI). Existing corruption/protocol fixtures were adapted to
  shared-definition text while retaining independent source/core/SMT checks.
- Clippy all targets/all features, warnings denied: **pass**.
- Owned formatting and `git diff --check`: **pass**.
- Acceptance runner Python tests: **2 passed** (fixture/preflight evidence).
- Near/exact/over tests cover default node/depth ceilings, smaller node/depth
  limits, exact output lengths and one-byte-over main/witness output, large
  identifiers, malformed definitions/obligations, shadowing/collision,
  inactive overflow chains, eager Boolean/local failures, integer edge grid,
  and conservative UF behavior. Default 4,096-node and depth-128 boundary
  emission/audits pass. An exploratory depth-256 boundary overflowed the debug
  test thread stack; the final hard ceiling was reduced to 128 and verified.
- Source CLI reproduction: each process capped at **512 MiB address space,
  15 CPU seconds and 15 wall seconds**. Every admitted row cross-checks all
  four obligations against original source, core and SMT for `x=-1,0,1`.
  Depth 140 returns CGV003 with no solver launch or partial SMT.

| Divisions | Source bytes | SMT bytes |
| ---: | ---: | ---: |
| 1 | 101 | 2,892 |
| 2 | 105 | 4,783 |
| 3 | 109 | 6,675 |
| 4 | 113 | 8,574 |
| 8 | 129 | 16,170 |
| 16 | 161 | 31,815 |
| 32 | 225 | 63,583 |
| 64 | 353 | 127,128 |
| 140 | 657 | rejected before SMT/solver |

These fixtures are persisted by the reproducible generator; their prefix is
three source bytes shorter than the coordinator's historical reproduction, so
this is a measured growth regression rather than a same-input timing comparison.
Core-only division measurements extend to 120 divisions: **79,234 SMT bytes**,
**78,923 witness bytes**, with independent SMT parity. Symbol digit widths add
a logarithmic factor; there is no repeated expansion of nested child strings.

## Evidence and remaining boundaries

`tests.txt`, `clippy.txt`, `checks.txt`, `acceptance-runner-tests.txt`,
`core-growth.txt`, `growth.json`, `live-solver-preflight.txt`, and
`live-solver/acceptance.json` retain commands/results/measurements. The live
runner returned **exit 3, blocked/unrun: trusted Z3/CVC5 executable unavailable**.
Neither executable is on PATH; no solver was installed or trusted on the
basis of a mock. Finite parity, mock transport, and fixture probes are not
trusted solver acceptance. An operator must supply an already trusted solver
and its previously trusted digest to rerun the existing acceptance script.

Verification is scoped to both owned packages, not a new full-workspace or
release acceptance run. Source accumulator admission, arrays/invariants/new
source types and all external release gates remain unchanged. The byte budget
bounds emitter buffers; caller-created AST allocation/destruction and global
allocator exhaustion are not claimed to be solved.

Host `/tmp` quota prevented sandbox startup. Scoped commands used normal
approval and private disk-backed TMPDIR. Zsh heredocs also required
`TMPPREFIX=$PWD/target/tmp-verifier-size/zsh`; no unrelated temporary data was
deleted. Required Serena initialization generated untracked `.serena/`
metadata outside owned source paths; it is excluded from the repair commit.
