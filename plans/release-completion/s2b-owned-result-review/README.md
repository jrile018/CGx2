# Owned Result review: repair required

Worker d88b57acd14cd299b029a46b197259bb2ec32079 staged as d8253431a9ab5ab571b9e7e8841242f7667b0a28. All paths comply; no dependencies/root-lock changes. Three independent reviews completed. Runtime/backend semantics and consumer rejection receive bounded acceptance; source compatibility fails. New consumer work remains gated.

Coordinator integrated tests:1229 passed,0 failed,6 ignored in265 suites. Build, all-target/all-feature Clippy -D warnings,fmt/diff and all12 product suites pass (2707 checks). These existing suites miss four reproduced schema1 regressions. Exact commands, compressed logs, hashes and results are in evidence/review.json and verification.json. Coordinator verifier adapts only checkout/target/temp paths from worker verify.py; saved coordinator-verify.py records it. No release performance measurement.

## Linked failures and independent repairs
- symbol-compatibility/plan.md: nominal Result/AllocationError, user function try_owned and local try_owned all previously check/run42, now reject CGT082. Scope covers declaration/type/intrinsic lookup and profile classification, preserving real new-profile builtins and frozen APIs.
- runner-output/plan.md: output files grow until the120s child wait ends; the1MiB assertion runs afterward. Enforce live bounded capture and process-group cleanup, with safe harness self-tests.

Plans have exclusive files and were independently reviewed before dispatch. Repair chats inherit this committed review base. They stop for review; no consumer uses unaccepted new interfaces. Historical evidence remains unchanged.

## Release ledger
Result/no_gc source implementation exists but acceptance is failed pending these repairs. No R01-R15 row closes. Full ordinary managed memory, generic/package/recursive/partial-move ownership, regions/transfers/recovery, cancellation/unwind, typed foreign/execution and command closure remain mandatory, followed by S3/S4. External solver/leak/isolation/library/GPU/performance/human-model gates retain prior blocked/unrun states.

## Runner repair accepted

Worker cc69d8d integrated as3cdb63c after independent source/evidence review and coordinator10 harness/584 unchanged product checks. Live pipe capture bounds both streams, preserves failure evidence and kills the process group. Evidence: runner-accepted/acceptance.json. Only direct child is waitable; escaped sessions and aggregate cgroups are outside its documented guarantee. Rust unchanged; no full workspace rerun claimed. Symbol compatibility remains running and full allocation acceptance stays repair-required.

## Both repairs accepted; default managed checkpoint ready

Symbol03e35bf integrated938c911 after independent core/evidence reviews. Combined1237Rust/0failed/6ignored266suites; build/Clippy/fmt/diff and2928 current product checks pass. Full evidence: evidence/accepted-repairs/acceptance.json. Original historical runner retains one obsolete shadow-builtin failure (actual check success); separate coordinator reconcile.py stages inputs without editing historical files, replaces9 obsolete rejections with18 genuine reserved-owned/ambiguous-error checks, and executes17 restored-source checks. Reconciled Result593 + restored17 + symbol195 + prior2123 =2928. Neither historical failure nor skipped checks are called passes. Automatic review declined modifying historical runner paths; the separate harness is its approved safer alternative.

Both repairs are reviewed bounded deliveries. The next single shared producer plan is ../s2b-default-managed/plan.md: default ordinary heap allfive shapes, roots, explicit reference aliases/cycles, checked allocation boundary and honest resource effects. Full ordinary memory, regions/transfers and all R01-R15/S3/S4/external acceptance remain open.
