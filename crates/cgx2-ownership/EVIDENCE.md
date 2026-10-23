# S1-03 ownership delivery evidence

Date: 2026-10-23. Reviewed starting commit:
`0f0b0b9c87d817f39830f3e2b311dbae2ae3e0f3`.
Branch: `riley/release-ownership`. Worktree:
`/home/john-riley/.codex/worktrees/730e/lang`.
The delivery commit is the commit containing this report; obtain its exact ID
with `git log -1 --format=%H -- crates/cgx2-ownership/EVIDENCE.md`.

## Implemented and checked

Added `v1::analyze_function` and versioned input/report types, preserving every
legacy `Op` variant and `check_ops` consumer. No dependency or root Cargo changes.
The function API models owned transfer, shared/exclusive parameters and result
loans, partial aggregate moves and structural overlap, bounded CFG joins and loop
fixed points, explicit region ownership and foreign owner retention. Unknown
lifetimes, local loan escapes and exhausted budgets reject. Byte spans are checked
against the original UTF-8 source. See [ADAPTER.md](ADAPTER.md) for exact contracts,
defaults and compiler/runtime obligations.

Owned files:

- `src/lib.rs`: additive public `v1` module only.
- `src/v1/model.rs`, `src/v1/analysis.rs`, `src/v1/mod.rs`: versioned contracts and
  analyzer; independent of the existing operation checker.
- `tests/function_analysis.rs`: 31 new positive/adversarial tests.
- `examples/function_analysis.rs`, `fixtures/call-spans.cg`: executable manual
  lowering example with real byte locations and a multibyte prefix.
- `README.md`, `ADAPTER.md`, `EVIDENCE.md`: entry point, S2 handoff and evidence.

## Actual commands and results

All Rust commands used the installed Rust 1.98.1 toolchain
(`rustc 1.98.1 (48a229cea 2026-09-01)`). Builds used:

```sh
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-ownership"
export TMPDIR="$PWD/target/tmp-ownership"
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
```

No additional library paths, installations, network access, shared target, real
benchmarks or external paid studies were needed.

| Command | Exit | Observed result |
| --- | ---: | --- |
| `cargo test -p cgx2-ownership --all-features --offline` | 0 | 54 passed: 31 new function tests and 23 unchanged legacy tests; no failures/ignored tests |
| `cargo clippy -p cgx2-ownership --all-targets --all-features --offline -- -D warnings` | 0 | No warnings; includes example |
| `cargo run -p cgx2-ownership --example function_analysis --all-features --offline` | 0 | Accepted `transfer`; `invalid_transfer` rejected with `CGO101 at 330..335 (owner)` |
| `cargo check -p cgx2-sema --all-features --offline` | 0 | Existing direct consumer compiles without changes |
| `cargo fmt --manifest-path crates/cgx2-ownership/Cargo.toml` | 0 | Formatted owned package only |
| `cargo fmt --manifest-path crates/cgx2-ownership/Cargo.toml --check` | 0 | Pass |
| `git diff --check` | 0 | Pass |

Tests cover valid owned/shared/exclusive calls, argument alias conflicts,
parameter/result provenance and loan kinds, use after move, returned local loans,
partial-move misuse and sibling cleanup, known/dynamic projection overlap,
zero-trip CFG joins, loop-local reinitialization, loop-carried invalid loans and
second-iteration moves, possible regional handles blocking close, region escape
and transfer rejection, unknown/missing foreign retention, explicit retention
release, UTF-8 locations, malformed CFG/contracts/spans, unsupported operations,
input/state/step exhaustion, deterministic reports and related loan spans.

## Boundaries, blocked acceptance and executable handoffs

**Passed: library-only fixture semantics.** The example manually supplies a unique
ownership policy for `Buffer`; no inference from current `.cg` syntax is claimed.
No current parser/type/semantic/compiler/runtime API was changed. Legacy source
memory-script support stays at the reviewed base's scope.

**Blocked: ordinary `.cg` ownership/borrowing.** S2 must lower typed functions,
layouts, cleanup and imported/foreign summaries under the recipe in ADAPTER.md.
The blockers are outside this track's exclusive paths: `cgx2-sema`, compiler and
emitters need the adapter and source/runtime tests. Run the existing handoff now:

```sh
cargo run -p cgx2-ownership --example function_analysis --all-features --offline
cargo test -p cgx2-ownership --test function_analysis --all-features --offline
```

After reviewed compiler integration, add corresponding `.cg` accepted/rejected
fixtures to the owning compiler/conformance suite, then run (these were **not run
as part of this delivery**):

```sh
cargo test -p cgx2-sema -p cgx2c -p cgx2-memory-conformance --all-features --offline
```

That command alone does not prove source coverage: S2 must first add and verify
the v1 lowering cases. In particular compare source diagnostic spans, runtime
cleanup and interpreter/native results against the library matrix.

**Outside v1, requires explicit rejection/future contracts:** raw pointers,
reference-containing or overlapping aggregate layouts, aliased managed graphs,
foreign owner escape, callbacks, token forwarding/reborrowing through exclusive
loans, implicit cross-region aliases/copies, region conversion, layout/index/type
checking, `no_gc` and allocation accounting. Advisory type/identity assertions in
an unauthenticated summary cannot establish foreign memory safety.

**Unrun external acceptance:** real foreign runtimes/FFI leak-enabled assurance,
independent safety audit, full integrated source corpus and release performance
measurements. This crate is safe-Rust static analysis; its fixture tests do not
exercise allocators/FFI or close the global memory release gate. After S2/S3
integration, the memory/evidence owners must provide their integrated sanitizer
runner and supported leak-enabled host (no `detect_leaks=0` substitution), source
and runtime revision/artifact pins, raw command/exit artifacts and independent
review. No such external runner was executed here. Quiet-host calibration is
required before any release timing; this delivery makes no performance claim.

MegaMemory is recorded with the implemented module/API, defaults, checked scope
and S2 dependency. No merge, follow-on stage, chat delegation or other-track edit
is included. The pre-existing untracked `.serena/` directory is excluded from this
commit.
