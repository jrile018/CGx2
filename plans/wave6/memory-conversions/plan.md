# CGx2 Source Owned/Managed Conversion Plan

> Implement this plan in your attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, or launch another chat.

**Goal:** Let checked `.cg` memory scripts transfer owned bytes into a managed region and explicitly copy managed bytes back to a unique owned value, preserving ownership and `no_gc` limits.

**Architecture:** Extend the Wave 5 memory source lowerer and ownership operation checker with direct `memory.promote_owned(region, owned)` and `memory.copy_to_owned(managed)` intrinsics. Extend the CLI memory evaluator to call existing `Session::promote_owned` and `Session::copy_managed_to_owned`. Root a newly managed result until explicit drop. Promotion consumes its source on success; a copy preserves its managed source. Keep parser/AST unchanged and reuse source byte spans.

**Tech stack:** Rust 1.98; existing sema, ownership, CLI, and runtime APIs; no new dependencies or `unsafe`.

**Specification:** `docs/language-design.md` owned/managed transitions and `no_gc`; `crates/cgx2-runtime/README.md`; `plans/wave6/README.md`.

## Owned paths and fixed boundaries

- Own only `crates/cgx2-sema/**`, `crates/cgx2-ownership/**`, `crates/cgx2-cli/**`, and this plan. Do not edit syntax/AST, runtime, emitter, notebook, package, benchmark, or root files.
- Direct source forms: `let m = memory.promote_owned(r, o)` returns managed `m` in open region `r` and consumes owned `o`; `let o2 = memory.copy_to_owned(m)` returns a fresh owned byte buffer while `m` remains live. Both require a direct named handle, not dynamic calls or aliases. Existing `memory.move`, `drop`, borrow, region close, and `no_gc` rules stay intact.
- Extend `cgx2_ownership::Op` conservatively for the copy; `Promote` already exists. Preserve `CGO001`–`CGO005` meanings and source spans. An unsupported path must never be omitted from analysis and then accepted.
- `Session::promote_owned` returns the original owned value on allocation failure. The script currently exits on runtime error; document that no success report is emitted and no second owner is created. In a `no_gc` block, only the documented metadata or copied storage is charged, exactly as the runtime API defines.
- A promoted managed handle is rooted by the CLI until `memory.drop`; a copied owned value has distinct storage. No implicit cross-region transfer or zero-copy managed-to-owned claim.

## Review focus

1. Promote then use original is a static `CGO001`; use promoted result survives explicit collection while rooted.
2. Copy then drop managed leaves owned copy live; after copying, managed and owned values are distinct and can be dropped independently.
3. Promotion into closed/wrong region, while borrowed, or after move fails statically. Region limit and `no_gc` budget failures remain defined runtime errors with zero collections during guards.
4. Failure does not print a success report or fabricated result; counters and ownership behavior match the existing runtime unit tests.
5. Scalar CLI, Python/Rust emitters' target rejection, and Wave 5 memory scripts still pass.

## Tasks

### 1. Static conversion rules

- [ ] Add red ownership and sema tests for valid promotion/copy, use-after-promotion, borrow conflict, wrong handle kind, closed region, and duplicate binding.
- [ ] Add precise source lowering to `Op::Promote` and a new explicit copy op. Extend checker states without treating copy as a move.
- [ ] Verify diagnostics retain `CGO`/`CGM` codes and UTF-8 byte spans.

### 2. Runtime integration

- [ ] Add red CLI/API tests for successful promotion and copy across collection and drop, region close, two-region isolation, and no_gc budget failure.
- [ ] Implement direct evaluator operations, root/unroot lifetime, and runtime error mapping. Keep all mutations after the static gate.
- [ ] Extend the memory report fixture to show promotion/copy accounting where appropriate; no foreign allocation is charged.

### 3. Verification and handoff

- [ ] Run `cargo test --workspace --exclude cgx2-python-bridge --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy --workspace --exclude cgx2-python-bridge --all-targets --offline -- -D warnings`, and six-case benchmark parity.
- [ ] Update owned crate READMEs with exact source operations and limits. Commit only owned paths.

**Acceptance:** One `.cg` program moves owned bytes into managed storage and copies a separate owned value back; static errors and dynamic budgets preserve the bounded memory guarantees.
