# Managed source authentication and nominal compatibility repair
Status: ready after coordinator review of bc4ba95. This is a bounded repair of the existing managed delivery; it does not accept that delivery or open consumer fan-out.

Read AGENTS.md, docs/language-design.md, docs/validation-plan.md, plans/roadmap.md, release-completion/{README,contracts,worker-rules,dispatch}.md, s2b-default-managed/plan.md and s2b-default-managed-review/README.md. Follow MegaMemory overview/query/record.

## Exclusive ownership
- crates/cgx2-sema/src/nominal.rs and crates/cgx2-sema/src/default_managed/**
- crates/cgx2-sema/tests/managed_source_auth.rs and managed_nominal_compat.rs (new files)
- crates/cgx2-cli/src/** (source/provenance pipeline and entry-diagnostic changes only)
- crates/cgx2-emit-rust/src/** (source/provenance pipeline only)
- crates/cgx2-package/src/** (checked provenance only), crates/cgx2-api-index/src/memory_v1.rs, crates/cgx2c/src/lib.rs
- New crates/cgx2-syntax/managed-core-repair-evidence/** and S2B-DEFAULT-MANAGED-AUTH-REPAIR.md.
All other paths are read-only, including root manifests/lock, coordinator plans, existing evidence, runtime graph/scope, and the evidence repair runner. Preserve frozen old APIs; additions to the new managed envelope must be explicit and versioned. Do not edit another repair lane.

## Required result
1. Restore local and imported user-defined AllocationError variant identity, including qualified paths, payloads, generics where previously supported, return/parameter/match annotations and targets. Bare nominal names must retain schema1 meaning. True recovery AllocationError and Result error argument validation must remain correct; the earlier nominal-record rejection regression must not return. See review/repros/core-allocationerror.cg: previous accepted binary prints42, delivery rejects.
2. Close the source-authentication seam: compile_module currently accepts an AST returning42 with unrelated invalid source text, authenticate succeeds and source_sha256 hashes that text. Source-based CLI, API/index, emitter and package routes must bind checked execution, bodies, layouts/root maps and both effect dimensions to their actual source inputs. Reject mismatched/stale source or provenance before execution/emission and before artifact replacement. Do not substitute Debug AST text for original source or silently drop import/callee authentication.
3. Preserve legitimate AST-only APIs with explicit provenance semantics; an AST-only artifact must not advertise a verified original-source digest. Keep public source-authenticated and package-authenticated construction authoritative, with a checked provenance design rather than a caller-set trusted boolean. Do not naively require a merged package AST to equal parsing concatenated module text, or disable supported package/native routes. Document the exact source-vs-AST/package distinction and additive contracts.
4. Ensure uncaught Session-entry failure diagnostics retain the original source span consistently in interpreter/native/standalone and mixed managed bridge. Existing CLI test-injected entry errors currently omit span; keep entry recovery out of scope.

## Verification
Use private target/release-managed-core-repair and target/tmp-managed-core-repair, CARGO_BUILD_JOBS=2, RUSTUP_TOOLCHAIN=1.98.1, LIBRARY_PATH=/tmp/cgx2-wave22-lib, offline/locked dependencies.
Add focused negative source/AST/provenance and stale/import tests; valid single-source/package and AST-only routes must work. Cover source/emitter hashes and artifact preservation, both behavioral and memory summaries, user variant compatibility and genuine recovery types, entry spans including UTF-8.
Run affected sema/package/CLI/API-index/emitter/cgx2c tests and all-feature/all-target Clippy -D warnings for affected packages; formatting/diff checks. Run unchanged195symbol suite and coordinator owned-result reconciliation593+17, managed product cases shapes/package/namespace/hybrid plus a bounded repair runner with native debug/release/standalone positives and negatives. Existing historical runner/fixtures remain unchanged. Record commands, bounds, exact exit/signal/output, binary/source/runtime hashes and limitations.
No installations, external spend, security changes, release measurements, main merge or publication. Finish this one repair, commit, report SHA/evidence, and stop for coordinator review.
