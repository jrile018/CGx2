# S2a — shared language and type representation

Status: ready after reviewed S1 repairs. Owner: one worktree chat. Requirements: R01 and the representation prerequisites of R02/R10/R11. Read ../README.md, ../contracts.md, ../worker-rules.md and project design/validation documents before coding.

## Bounded delivery

Implement nominal payload variants, exhaustive matching, usable Option<T>/Result<T,E>, and stable qualified type identities across checked packages. Establish the AST/type/value representation and documented syntax needed by S2b and S3. This is one coordinated shared-interface task; other consumers remain paused until coordinator review.

Existing nominal records, scalar functions, package effects/contracts, memory-script and foreign routes must keep working. Preserve source spans, checked arithmetic and runtime contract behavior. Existing Type::Record(String), TypeSyntax.arguments and package canonical qualification must be considered explicitly during migration; no unqualified name equivalence across modules.

## Exclusive write ownership

Full implementation ownership:
- crates/cgx2-syntax/**
- crates/cgx2-sema/**
- crates/cgx2-package/**
- crates/cgx2-api-index/**
- crates/cgx2-cli/**
- crates/cgx2c/**
- crates/cgx2-emit-rust/**
- crates/cgx2-emit-python/**

Mechanical shared-interface compatibility only:
- crates/cgx2-notebook-{format,kernel,session,cli,ui}/**
- crates/cgx2-verify-{core,cli}/**
- crates/cgx2-effect-index/**
- crates/cgx2-security/**

The brace lists above mean exactly the named crates, not additional directories. Consumer edits adapt exhaustive matches, source traversal, and explicit unsupported-profile diagnostics/tests. They do not add notebook variant execution, new proof semantics, or new advisory analysis features. Do not mask unsupported syntax with unchecked wildcard arms or accidentally omit effects/contract calls.

Owned contract/evidence: crates/cgx2-syntax/S2A-CONTRACT.md and crates/cgx2-syntax/s2a-evidence/**. Put all new fixtures/tests under owned crates.

Read-only: all other crates (including runtime, ownership, foreign/ffi/bind/LibTorch/Python bridge, interop-worker, execution-policy and memory-conformance), root Cargo.toml/Cargo.lock, bench, tools, shared docs and plans/ledgers. No new dependency installation. Report any required changes outside ownership with exact signatures/paths.

## Implementation and contract

1. Specify canonical indentation-based variant declarations, qualified constructors, match arms and patterns, binding/shadowing and result typing in S2A-CONTRACT.md before implementation. Follow existing language conventions. Include public/private visibility, duplicate declaration/constructor/pattern diagnostics, payload arity/types and recursive-type policy. Grammar remains an implementation candidate pending the genuine R14 syntax study.
2. Represent nominal identity independently of spelling at a call site; support imported public variants and records without collisions or private-type leakage. API output must contain public variant cases/payloads, type arguments and stable qualified identity with deterministic ordering and correct spans.
3. Implement payload-free/payload constructors, checked match scrutinees and pattern scopes, exhaustive coverage, unreachable/duplicate arms and consistent branch result typing. State nested-pattern and guard support exactly; reject unsupported forms explicitly. Evaluate scrutinee once and only the selected arm; preserve checked errors/effects/contracts.
4. Implement Option<T> (Some/None) and Result<T,E> (Ok/Err) as well-defined typed variants, including construction, match, argument/return use and nested concrete instantiation. No runtime string-tag guessing or untyped payload escape hatch. Support concrete user variants containing scalar/record/variant payloads and imported identities. Recursive/infinite-size types must either have safe explicit representation or a source diagnostic; never recurse unboundedly during resolution.
5. Publish stable type parameter/application/constraint representation and proposed canonical syntax for user generic declarations with named constraints, collections/arrays, checked indexing, and loop invariant/decreases clauses. Parser/formatter/API coverage and explicit unsupported semantic diagnostics are required where full execution is deferred. Full generic inference/substitution/constraint solving and standard data/loop proof semantics remain S2b/S3 work; record their exact boundaries. Do not claim R02/R11 closure from syntax.
6. Complete interpreter and native Rust lowering for the admitted variant/match/Option/Result profile, including entry/package paths. Portable Python must either preserve semantics for the admitted profile or give a precise target-specific unsupported diagnostic, with the missing R10 work carried into S2b. Never silently approximate.
7. Update every exhaustive AST/type/value consumer mechanically so the workspace builds. Existing verifier soundness limits (4096 nodes/parameters, depth128, output1MiB and fail-closed emission) must remain intact. Unsupported new forms produce structured source-located errors/unknown outcomes, never proved. Analysis must traverse newly representable children conservatively or explicitly reject admission; no skipped effectful subtree.
8. Publish compatibility details: changed public symbols/fields/enums; old/new representations; constructor/type identity mapping; consumer rejection matrix; source and target feature matrix; exact S2b and S3 follow-ups. The coordinator reviews this commit before further consumer work.

## Required evidence

Positive executable fixtures:
- local payload-free/payload variants and matches, Option/Result success/error branches, records containing variants and variants containing records;
- imported public variants/functions in two modules with colliding local type/case names, preserving nominal identity;
- payload binding scopes/shadowing, one evaluation of scrutinee, only selected arm executing, contract success/failure and checked arithmetic failure;
- parse → format → parse equivalence, idempotent formatting and deterministic API metadata.

Negative fixtures:
- nonexhaustive and duplicate/unreachable arms; wrong constructor/payload count/type; inaccessible/private and unknown variants; unrelated nominal types; inconsistent arm results;
- rejected recursive/unsupported forms, generic arity/unsupported constraint semantics, invalid pattern scope and invalid syntax, all with stable UTF-8 spans;
- notebook/verifier/analyzer/target unsupported-profile behavior, ensuring no silent acceptance or proof claim.

Run private offline workspace all-feature tests and workspace all-target/all-feature Clippy -D warnings because this task changes shared enums, cargo fmt --all --check and git diff --check. Run concrete interpreter/native parity and applicable portable Python fixtures, plus existing package/record/conditional/contract regressions. Record counts and exact commands, not just done. Tests must validate semantics rather than mirror code.

Set RUSTUP_TOOLCHAIN=1.98.1, CARGO_BUILD_JOBS=2, CARGO_TARGET_DIR=$PWD/target/release-s2a and TMPDIR=$PWD/target/tmp-s2a; create them first. If still needed use LIBRARY_PATH=/tmp/cgx2-wave22-lib. Do not run performance measurements while other work is active.

## Completion and stop

Commit the coherent implementation, contract and owned evidence in your attached worktree. Multiple local commits are acceptable for a cohesive delivery. Report commit IDs, files, checks, feature matrix, limitations and any coordinator-owned dependency changes. Stop for review. Do not start S2b, create chats, delegate, modify shared plans, merge main or publish. R01 closes only after coordinator acceptance of its required source/target evidence; R02–R15 remain independently tracked.
