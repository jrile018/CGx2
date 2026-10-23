# Allocation-free checked scalar range loops

**Goal.** Make existing readable `for` syntax useful for bounded scalar programs without introducing hidden managed allocations or a new collection rule. Implement one simple loop form through checker, interpreter, and native output.

**Ownership.** Edit only `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, `crates/cgx2-cli/**`, `crates/cgx2-emit-rust/**`, and `crates/cgx2c/**`. Do not edit runtime/ownership internals, Python emitter/package, notebook, foreign adapters, execution-policy crate, root Cargo files, or shared docs.

## Deliverable

1. Admit `for item in range(start, end):` in the scalar source profile. `start` and `end` are checked `i64` expressions, evaluated once; iteration is increasing by one, starts at `start`, and stops before `end`. `end <= start` is zero iterations. The iterator is not a first-class value and `range` outside this exact `for` form is rejected with a source-located diagnostic. No list or heap object is created by the loop mechanism.
2. Preserve lexical scope and mutation/effect rules. Support checked scalar expressions, `let`, `+=`, and direct pure/local calls in the body as the existing checker permits; reject memory-script loops, unsafe foreign calls, unsupported control flow, and ownership crossings rather than silently changing semantics. Apply one documented deterministic execution-step/iteration budget in interpreter and generated native output so a huge range cannot hang a preview program indefinitely. Preserve the first overflow and source span.
3. Add `cgx2 check`, `cgx2 run`, and `cgx2c build` tests for zero/one/many iterations, negative endpoints, `i64` boundaries, nested local calls, body overflow, incorrect range arity/type, and budget exhaustion. Compare interpreter and native debug/release stdout, exit status, diagnostic class/span, and relevant effect metadata. Formatter round-trip must retain the same AST. Existing scalar and memory fixtures must remain unchanged.
4. Update only owned README/examples with precise preview limits. The Python output target may reject loops until separately implemented; do not claim that this slice enables generic list iteration, automatic parallelism, or GPU execution.

## Verification and handoff

Run targeted offline suites for all owned crates, source/native parity fixtures in debug and release where feasible, formatter, and Clippy with `-D warnings`. Commit only owned paths. Report SHA, changed files, exact commands/results, accepted form, budget behavior, and unsupported forms; stop for review.
