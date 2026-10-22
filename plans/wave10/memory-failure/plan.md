# Memory failure-path hardening

> Work in the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, create another chat, or automate follow-up work.

**Goal:** Tighten and test `Session::alloc_bytes` failure ordering and region invariants, with memory correctness as the highest-priority release gate.

**Architecture:** Specify which automatic collection is allowed to make an allocation succeed, and which impossible/guarded requests must fail without collection. Preflight arithmetic and fixed limits before any optional automatic collection when they make success impossible. Reuse the existing state-machine audit; do not change the public `Session` API or introduce a second accounting model.

**Tech stack:** Rust 1.98, `cgx2-runtime`, `cgx2-memory-conformance`; offline Cargo.

**Spec:** `docs/language-design.md` memory section; `docs/validation-plan.md` hard memory gates.

## Scope and contract

- Own only `crates/cgx2-runtime/**` and `crates/cgx2-memory-conformance/**`. Read but do not edit source/compiler code, root manifests/lockfile, shared docs/plans, or benchmark files. Preserve public method signatures and error variants so the native-emission lane can use the current API.
- An invalid region, `len + metadata` overflow, or request intrinsically larger than its region limit must not trigger automatic collection first. A request that can fit after reclaiming unreachable memory in an auto region may collect and then succeed or fail according to real capacity. In a manual region, only explicit `collect` changes its collection count. Inside a `no_gc` guard, every success/failure/nested path performs zero CGx2 collections and an exhausted budget returns `AllocationError`.
- On every failure, no new usable handle exists; live/committed bytes, roots, generations, budgets, and unrelated regions remain consistent. The failure counter may increase exactly once. Test collector side effects separately from allocation side effects where auto collection is permitted.
- Include closing/collecting regions, stale handles, and owner conversion failure where the current API permits. Keep tests deterministic and bounded; use fault injection or a seam only if it exposes a real failure path. A finite corpus improves evidence but is not a proof of all memory safety.

## Tasks

1. Add failing targeted tests for invalid region, arithmetic overflow, request larger than region limit, `no_gc` budget exhaustion, nested guards, manual allocation failure, and auto reclaimable success. Capture before/after reports and handle validity.
2. Repair only demonstrated runtime defects. Preserve public signatures, document collection ordering in the crate README, and add state-machine regression seeds for each real bug. Distinguish genuinely unreproducible cases from fixed failures.
3. Run `RUSTUP_TOOLCHAIN=1.98.1 cargo test -p cgx2-runtime -p cgx2-memory-conformance --offline`, both repeatable v4 audit runs plus the extended corpus when time permits, fmt and targeted Clippy `-D warnings`. Record exact seed counts and any omitted dynamic sanitizer or formal proof.
4. Commit only owned paths and report changed invariant, before/after evidence, checks, limitations, and commit.

**Acceptance:** Impossible or guarded allocation failures do not cause an unrequested collection, and the checked accounting/handle invariants survive all exercised failure paths.
