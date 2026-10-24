# Ordinary-memory delivery review: repair base only

Delivery a3b98d7374de8aea90a610a73c99b04deb8857a1 is clean and stopped. It is staged as 32032fd3c34d1a1f71e5229ec3e72dcbf0c462a6 on the coordinator branch solely for repairs. **Not accepted; no S3 or new language consumers.** Main remains unmerged.

Independent source/runtime/consumer reviews and coordinator frozen-binary reproductions confirm:

| ID | Priority | Defect | Repair owner |
| --- | --- | --- | --- |
| OM01 | P1 | Shadowed managed.ref/managed.collect allow real region/collection under effects() caps() | core-admission |
| OM02 | P1 | Legacy no_gc budget traversed as expression index panics; assignment tags likewise misread | core-admission |
| OM03 | P1 | User variant named Err erases heap payload region provenance, accepted source escapes at runtime | core-admission |
| OM04 | P2 | Legitimate user borrow function intercepted as region capability intrinsic | core-admission |
| OM05 | P2 | Region arm assignment to outer managed binding releases registration through wrong root table: NotRooted | runtime-roots |
| OM06 | P2 | Safe runtime copy_graph_to_v1 releases temporary source root before same-region allocation; Auto trigger1 invalidates source | runtime-roots |
| OM07 | P2 | Copy publication root absent from evidence registrations: copy-cycle 25 registered / 26 released | runtime-roots |
| OM08 | P2 | Ordinary api routes to memory-effects view, exposing private functions and losing public records | api-visibility |

Reproductions under repros/ retain exact source/commands/exits/hashes, including unsuccessful coordinator package argv followed by correct positional package invocation. OM06 is direct safe-runtime API exposure; source evaluator currently roots its expression arguments. Do not claim a demonstrated source-level OM06 failure.

## Evidence and remaining scope

Worker evidence independently verified: 209 exported hashes, 205 compiler inputs, frozen binaries/libraries, 1327 Rust tests/0 failed/6 ignored, 117 new product assertions plus 19 reused regression assertions =136, and22 independent standalone checks. Parent product attempt retains6 package-argv failures; final run fixes invocation and reuses only rows against identical frozen artifacts. Initial exploratory interface-sema raw diagnostics were overwritten; first scratch cleanup audit failed; final artifacts do not recover those missing histories.

Coordinator rebuild independently passes1327 workspace tests/0 failed/6 ignored, Clippy, fmt/diff and8 harness selftests. Full product/standalone replay disposition is recorded separately when complete. These successes do not negate the defects or missing acceptance rows.

Delivered support includes scoped named regions/guards, contextual allocation, managed graph copy, scalar pre-Session fallback and source bundles. Required general aggregate ownership (projections/partial moves/reborrows/recursion/generics/packages), source copy_owned/promote with allocation-free same-owner failure, complete v3 summary/body authentication, precise static dependency-file attribution and full fault/cleanup/parity remain incomplete. v3 currently adapts to v2. R01–R15 stay required without narrowing.

## Exclusive repair frontier

Four disjoint plans share this reviewed repair base: core-admission, runtime-roots, api-visibility, evidence-parity. They repair existing reviewed surfaces; none adopts unfinished ownership interfaces. The evidence lane records known implementation failures accurately and can be integrated before the combined post-repair rerun. Do not cherry-pick across workers or edit another lane. Coordinator reconciles and runs full integration gates before declaring a new accepted checkpoint or next sole producer.

Independent plan review confirmed disjoint feasible lanes after assigning sema/src/effects.rs to core and freezing authenticated original-provenance API projection. Package dependency-file diagnostics remain explicitly deferred/failed; no impossible acceptance promise was moved into API repair.

Coordinator full product replay passed136/0, including unchanged current regressions and explicit historical reconciliation. Initial standalone attempt failed on /tmp quota before final report persistence; traceback preserved, exact own inactive scratch archived and hash-verified before removal (26,017,239 bytes), no other directory removed. Partial command records were in memory and are unavailable. A retry changes only scratch-directory placement to a private location outside every repository; actual outcome follows in retained evidence. This motivates incremental harness reporting in evidence-parity.

Standalone directory-only retry passed22/0; frozen binaries/libraries remain unchanged. Independent baseline:1327Rust/0failed/6ignored,8harness selftests,136product assertions and22standalone assertions. See evidence/review-status.json and compressed raw reports. This establishes a buildable reviewed repair base, not accepted ordinary memory.
