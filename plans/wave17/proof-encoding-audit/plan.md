# Proof encoding audit

**Goal.** The Wave 16 verifier records solver identity and bounded observations but keeps global results `unknown`. Build confidence that the admitted checked `.cg` fragment is encoded faithfully before trusting any solver result.

**Ownership.** Only `crates/cgx2-verify-core/**` and `crates/cgx2-verify-cli/**`. Consume frozen source-contract extraction APIs. Do not edit syntax/sema/CLI, root Cargo files, shared docs, or other lanes.

## Deliverable

1. Audit the actual Wave 16 source subset: one pure expression body, one `requires`, one `ensures`, `i64`/`Bool`, checked `+`/`-`, comparisons and Boolean operators. Calls, `assert` statements, multiplication/division, loops and effects remain unsupported with source spans. For small bounded domains, evaluate every input tuple independently and compare the four named emitted SMT obligations: `requires_defined`, `requires_satisfiable`, `ensures_false`, and `ensures_overflow` (including substituted body overflow). Implement an independent evaluator for this emitted SMT-term subset; if it cannot be completed, mark semantic cross-check unrun rather than claiming it from string or stub tests. Include `i64::MIN/MAX` overflow boundaries and preserve source spans/assumptions.
2. Separate finite-domain `verdict.kind` from global `proof.kind`, `bounded_solver_result`, validated `disproved`, and `unknown` with explicit trust conditions. A stub solver tests only transport/protocol rejection. A `proved` result for the exact checked pure fragment requires all four complete obligations, source/evaluator parity, and a live exact Z3/CVC5 result with identity recorded; otherwise keep global proof `unknown` with reason. Reject malformed, truncated or inconsistent solver output and enforce time/memory/output limits.
3. If a live solver binary exists, record version/hash and run known true/false/overflow fixtures plus independent finite cross-check. If absent, publish generated formulas/fixtures and mark live proof unrun; do not make a trusted proof claim from mock output.

## Verification

Run offline tests for both owned crates, formatter, targeted Clippy with warnings denied, exhaustive small-domain fixtures and optional live solver command. Report exact fragment, trust boundary and commit ID. Stop for review.
