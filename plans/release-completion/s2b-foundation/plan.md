# S2b foundation — executable generics and standard data

Status: ready only after reviewed S2a repair integration. This is the first bounded checkpoint inside S2b, **not completion of S2b**. Owner: one shared-core worktree chat. Requirement focus: R02 and R10, with checked interface prerequisites for R03–R08/R12. Read ../README.md, ../contracts.md, ../worker-rules.md, ../s2a-review/README.md, AGENTS.md and design/validation requirements.

## Deliverable

Implement generic functions, records and variants with named constraints, plus usable List/Array/String operations and checked indexing through source checking, package execution, native Rust and the stated portable Python profile. Publish the concrete compatibility contract. Document the remaining ordinary memory/foreign/execution/command interface envelopes for coordinator review; do not claim those source integrations are implemented by writing proposals.

The S2a TypeParameter, TypeSyntax.arguments, collection/index AST nodes and provisional syntax are starting points. Deferred CGT032 generic/data cases become admitted only when their full semantics and target behavior exist. Preserve all previously supported programs, genuine unsupported-target outcomes and the three S2a repair regressions.

## Exclusive ownership

Full implementation:
- crates/cgx2-syntax/**
- crates/cgx2-sema/**
- crates/cgx2-package/**
- crates/cgx2-api-index/**
- crates/cgx2-cli/**
- crates/cgx2c/**
- crates/cgx2-emit-rust/**
- crates/cgx2-emit-python/**

Mechanical exhaustive compatibility/admission only:
- crates/cgx2-notebook-format/**
- crates/cgx2-notebook-kernel/**
- crates/cgx2-notebook-session/**
- crates/cgx2-notebook-cli/**
- crates/cgx2-notebook-ui/**
- crates/cgx2-verify-core/**
- crates/cgx2-verify-cli/**
- crates/cgx2-effect-index/**
- crates/cgx2-security/**

Owned contract/evidence: crates/cgx2-syntax/S2B-FOUNDATION.md and crates/cgx2-syntax/s2b-evidence/**; put other tests/fixtures under owned crates.

All remaining runtime/ownership/foreign/bind/ffi/LibTorch/Python bridge/worker/execution/conformance crates, root Cargo.toml/Cargo.lock, bench/tools, shared docs/plans/ledgers are read-only. This is the sole core writer; do not delegate or create chats. Existing dependency edges may be proposed using owned manifests, but no new third-party package installation; report exact coordinator lock reconciliation. Temporary generated root-lock edits needed for local checks must not be committed.

## Concrete language contract and implementation

1. **Generic identity and substitution.** Define binder identity/scopes, concrete instantiation of record/variant fields, qualified module identity, substitution and deterministic specialization keys. Implement parameter/result inference where unambiguous, explicit application syntax and actionable ambiguity diagnostics. Support nested concrete applications, constructors, calls and matching. Reject wrong arity, unknown binders, duplicate parameters, escaping private types and incompatible nominal arguments. Keep existing Option/Result identities and semantics stable.
2. **Named constraints with actual operations.** Freeze exact supported named constraints, satisfaction rules and operations in the contract before implementation. At minimum make existing Eq + Show examples meaningful; add ordering or other constraints only with defined behavior. A declaration's constraint must govern use inside its body and be checked at each instantiation, including cross-package imports. Merely recognizing a constraint name is not constraint checking. Unknown, unsatisfied and ambiguous constraints reject with spans. Do not silently accept arbitrary traits or an unimplemented user-defined constraint ecosystem.
3. **Public package evidence.** Generic parameters, constraints, instantiated signatures and canonical identity must be represented in deterministic API text/JSON and checked package metadata. Imported checked-effect/signature evidence must cover every source declaration on which specialization depends. Retain verify_unchanged, source hashes, declared effects/capabilities and transitive trust checks. A generic body cannot smuggle hidden effects through specialization.
4. **Collections.** Define List<T> versus Array<T>, length and mutation policy, empty/nested literal inference, homogeneous element typing and minimum operations needed by real small applications: construction, length, iteration, safe access/update as supported, and list growth if lists are growable. No untyped heterogeneous escape hatch. Preserve the existing List behavior where previously admitted; explicitly document any compatibility correction. Size/length arithmetic and allocations are bounded and checked.
5. **Indexing and errors.** Freeze negative/out-of-range behavior and recoverable Result/Option error representation; expose an ergonomic checked-access operation. Index syntax must never produce unchecked host indexing or panic. Evaluate collection and index once. Define element copy/borrow behavior for this admitted finite value profile; reject unsupported loan/handle elements rather than bypass pending ownership. Define String length/index/slicing in terms of UTF-8 bytes, Unicode scalars or explicit separate operations, with boundary checks and tested semantics across targets. Do not let Rust/Python indexing conventions silently disagree.
6. **Execution and targets.** Interpreter and native debug/release execute admitted generic/data programs with identical values, checked arithmetic, contracts and error classes. Implement the portable Python value profile for S2a variants/match/Option/Result plus admitted generics/data and checked pure functions/loops; preserve single evaluation and nominal identity. Ordinary regions/no_gc/borrowed owners/foreign effects may remain explicit target-specific rejections, documented before artifact publication. Do not claim R10 as a whole closed while its real readability study remains unrun.
7. **Bounds and recursive behavior.** Set documented positive bounded defaults for type expansion, instantiation count/depth, collection sizes and relevant evaluator/output work. Reject cyclic/infinite-size instantiations, runaway polymorphic recursion and oversized inputs before unbounded expansion. Preserve existing source/SMT limits; no new proof semantics are added here.
8. **Consumers.** Adapt every exhaustive AST/type/value consumer. Notebook/verifier/advisory admission remains explicit until the later S3 work. New syntax/type children must be visited conservatively or rejected before old walkers. Never turn unsupported into a proved/clean report, drop an effectful child or rely on a reachable panic. Syntax-only API statuses must remain unchecked.
9. **Round trips and documentation.** Parser/formatter/API round trips cover applications, declarations/constraints, literals/indexing and nested matching. Text and JSON retain complete types and honest statuses. Grammar remains a candidate pending R14; no empirical grammar freeze.

## Required acceptance

Commit runnable source fixtures, expected outputs/diagnostic classes and an owned reproducible product runner.

- Cross-module generic functions/records/variants with colliding local names, nested Option/Result/List/Array types, inference plus explicit calls and pattern use.
- Wrong type arity, unavailable/private types, unsatisfied/unknown constraints, illegal constrained operations, ambiguity, conflicting substitutions and stale imported summaries reject before backend output; verify artifact preservation.
- A small application that combines parsing/processing strings, collection iteration, checked indexing, error handling and generic helper calls. Use actual CGx2 source, not a Rust library-only test.
- Empty/nested collections; zero/one/many lengths; lower/upper/negative boundary indices; non-ASCII strings; invalid UTF-8 boundaries if byte slicing exists; size limits and overflow.
- Selected-arm-only behavior, scrutinee/index single evaluation, partial/early returns and non-final matches; preserve all S2a fixes.
- Interpreter/native debug/release/portable Python parity for admitted programs and diagnostic/error parity for negative/runtime-error cases. Native and emitted Python cases must actually execute.
- Deterministic checked API output, syntax-only generic metadata and source-location regressions.
- Run workspace all-feature offline tests, all-target/all-feature Clippy -D warnings, cargo fmt --all -- --check and git diff --check. Shared representations require these integration checks. Run native output and Python emit/package examples outside repository-relative execution assumptions.

Use RUSTUP_TOOLCHAIN=1.98.1, CARGO_BUILD_JOBS=2, CARGO_TARGET_DIR=$PWD/target/release-s2b-foundation, TMPDIR=$PWD/target/tmp-s2b-foundation; create private directories first. Use LIBRARY_PATH=/tmp/cgx2-wave22-lib if needed. No performance measurements.

## Remaining S2b interface envelopes and fixed continuation

Alongside executable generic/data semantics, document precise proposed names/types/lifetimes and source/target admission for the following remaining integrations. Mark each proposal versus implemented API explicitly; a coordinator-reviewed proposal alone does not satisfy source acceptance.

- **Ordinary memory R03:** parameter/result move/borrow modes, region/loan provenance, aggregate projection/layout IDs, CFG spans, cleanup obligations and cross-package authenticated summaries. Existing API is cgx2-ownership::v1::analyze_function(function, summaries, source, limits), with mapping in crates/cgx2-ownership/ADAPTER.md. Its unique region-bound policy must not be treated as a proof for freely aliased managed graphs. Runtime Session::try_new/try_no_gc/scoped_roots and ScopedRoots::{root,cleanup,finish} preserve session storage identity and trace roots before allocation safepoints.
- **Foreign/package R04/R06/R07/R08:** typed copied values versus owned/session handles; dtype/shape/device; retention/release; effects/capabilities and pinned route identity. Existing ForeignValue/Declaration are in ffi-core, schema-3 binding locks in cgx2-bind/sdk_v2, native Sdk/OwnedHandle/SharedHandle in ffi-native/sdk_v2, PackageEnvironmentV1/PackageAuthorizationV1 in cgx2-foreign/package_v1, and Torch/MatrixF32 in cgx2-libtorch. Never map raw pointers to managed values or silently use Python for native tensors.
- **Execution R12:** source envelope for owned_v1::execute(Input::{Map,ZipMap}, policy, context, cancellation, budgets), ValidatedProfile identity, cancellation/overflow/no-partial-result semantics and structured report fields. Auto remains sequential absent valid evidence; managed sessions/foreign handles are not dispatch-safe inputs.
- **Commands R05:** remaining installed test/verify/notebook/bind routing, deterministic test/property discovery, seeds/failures and missing dependency diagnostics. Keep checked build/run/check/emit/fmt/api behavior intact.

After this bounded foundation is reviewed, the coordinator must schedule the remaining S2b source integrations above, full admitted Rust support and any remaining portable Python profile work. S3 notebook/verifier/analyzer/foreign adapters and R13–R15 external acceptance remain separate. No requirement is removed by this split; no S2b completion or release claim is permitted.

## Commit and stop

Complete the bounded foundation, contract, compatibility inventory and evidence; commit and stop for review. Report exact commit(s), source/target matrix, counts/commands, remaining S2b envelopes and any root-lock proposal. Do not edit master ledgers, begin another task, create chats, merge main or publish. No package installation, external paid calls, recruiting or host security changes.
