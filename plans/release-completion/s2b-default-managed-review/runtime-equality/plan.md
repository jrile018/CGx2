# Managed Result value equality repair
Status: ready after coordinator review of bc4ba95; bounded repair, not full managed acceptance.

Read AGENTS.md, design/validation/roadmap, release-completion common contracts/rules/ledger, s2b-default-managed/plan.md and s2b-default-managed-review/README.md. Use MegaMemory workflow.

## Exclusive ownership
crates/cgx2-runtime/src/graph.rs; new crates/cgx2-runtime/tests/managed_result_equality.rs; new crates/cgx2-sema/tests/managed_result_equality.rs; new crates/cgx2-syntax/managed-equality-repair-evidence/** and MANAGED-EQUALITY-REPAIR.md.
All other paths read-only, especially sema producer/auth source, CLI/emitter/package code, coordinator plans, root manifests/lock and existing evidence. No public AST/type/model shape or API changes are authorized; preserve current contracts and other lane ownership.

## Required result
Equal source Result values must compare equal regardless of allocation-free recovery carrier versus ordinary heap variant representation. Review repro result-equality.cg currently evaluates managed.try_alloc(1) == Result<i64,AllocationError>.Ok(1) as false. graph.rs equal currently rejects differing representation depth before logical comparison.
Normalize logical type, discriminant and payload comparison without allocating recovery carriers or weakening typed Ref rejection/cycle bounds. Cover Ok and Err, both operand orders, == and !=, nested records/variants/List/Array and inequality cases. Preserve persistent values, traversal/expansion bounds, actual graph payloads and roots. Real no_gc allocation/root bookkeeping and intrinsic allocation-free Ok/Err remain unchanged.
Do not turn arbitrary differently typed variants or different cases/payloads into equal values. Preserve failure propagation, root cleanup and exact deterministic memory behavior.

## Verification
Private target/release-managed-equality-repair and target/tmp-managed-equality-repair; pinned1.98.1, CARGO_BUILD_JOBS=2, existing LIBRARY_PATH, offline/locked.
Run runtime all-feature tests and relevant sema/default-managed/conformance tests. Add typed source regressions and a bounded runner proving interpreter/native debug/native release/standalone equal results and memory/root parity (no repo-relative standalone dependency). Include recovery under a real budget failure, nested payloads, reversed operands and non-equal controls; no host failure or signal counts as expected success. Run unchanged graph99/guard518/report265 and existing managed product; affected all-feature/all-target Clippy -D warnings, fmt/diff. Preserve commands/exits/signals/hashes and failure evidence.
No broad new runtime validation feature, installations, external spending/security changes or release benchmarking. Commit one bounded repair and stop for review; all broader release gates remain open.
