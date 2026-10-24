# Managed source authentication and nominal repair

Repair base: `3c1cfe2aeba2f7135e4069709bf633c082d2aab0`. This delivery is for coordinator review. It does not accept the original managed delivery, combine the other repair lanes, or close ordinary memory or release gates.

## Behavior and source contract

`sema/src/nominal.rs` gives a declared bare `AllocationError` record **or variant** its nominal meaning, including generic arguments. Declaration binders still resolve first. The fully qualified recovery type remains `cgx2.core.AllocationError`; a user's error variant cannot satisfy an annotation requiring that recovery type. Tests cover the reviewed print-42 reproducer, payloads, local and imported generic variants, qualified parameter/return/local/match types and constructors, preserved record behavior, actual recovery, and incompatible Result error arguments.

The new `default_managed/provenance.rs` adds `ManagedProvenanceV1` and `compile_source_bundle_v1(sources, entry)`. Its three construction modes are explicit:

| Construction | Checked input | Original-source digest |
| --- | --- | --- |
| `compile_source(text)` / `compile_module(module, text)` | Original text; `compile_module` also requires exact equality with parsing that text, including spans | SHA-256 of those original UTF-8 bytes |
| `compile_ast(module)` | Supplied AST, freshly type checked and lowered | Unavailable; `source_sha256()` returns an empty string |
| `compile_source_bundle_v1(&[(identity, text)], entry_identity)` | Every original module text, declarations, explicit imports, private/public visibility, dependency order and body-derived transitive effects; then deterministic package qualification | No single-source digest; versioned bundle identity covers every input |

One source is limited to 4 MiB. A bundle contains 1–128 distinct source identities, requires distinct declared module names in multi-source inputs, rejects missing/cyclic imports, and permits an unnamed module only for a singleton. `entry` identifies a source input, not a module name. The identity digest uses u64 little-endian length-prefixed fields: provenance kind, then original source for `source-v1`; or entry identity followed by each source identity and original text, in supplied order, for `source-bundle-v1`. `ast-only-v1` carries no source assertion.

These public descriptive provenance values are not certificates. `ManagedProgramV1` retains private immutable fields. Construction and `authenticate()` reconstruct the checked result using the corresponding input mode; authentication compares syntax, typed bodies/effects, the full runtime program (including layouts, root maps and memory effects), concrete types, and digests. No caller-set trusted boolean, caller-supplied callee summary, Debug-AST-as-source substitution, or parsing of concatenated module text is used.

`CheckedPackage::managed_program_v1()` first runs the existing `verify_unchanged()` checks for manifest/source bytes and canonical targets, then constructs the source bundle from retained original texts. Package loading, interpreter execution, memory-index output and Rust emission use this route. Package target/visibility checks and native/standalone routes remain supported. A sema certificate authenticates its immutable source **snapshot**; filesystem freshness is additionally the responsibility of the checked package/compiler route. It does not claim that an arbitrary source file remains unchanged forever after construction.

The existing AST-based Rust API remains usable through `compile_ast`. Its emitted header explicitly says `ast-only-v1` and `original source sha256: unavailable`. Source-based Rust output reports the actual original hash; package output reports its bundle input hash. `ApiMemoryIndexV1` adds `provenance_schema_version: 1`, `provenance_kind`, and `inputs_sha256`. Its `source_sha256` is a string only for a verified single source and JSON `null` for AST-only/bundle inputs. Behavioral and `MemoryEffectsV1` summaries retain their existing fields and meanings. Other frozen API/index schemas are unchanged.

The mixed owned/managed producer labels its managed compiler projection AST-only. Its outer authenticated envelope still reconstructs that projection from the complete original source and compares its analysis identity before execution or emission; it never advertises the projection as a parse of the original source.

Single-file `cgx2c` publication now also rechecks the original canonical source target and bytes after target compilation and before artifact installation, or before emitted-source installation. Source drift returns `CGXC009` and preserves the previous artifact. Package publication retains its existing `CGP018` freshness checks. Interpreter package diagnostics retain the existing invalid-string and missing-main codes.

## Entry diagnostics

Injected Session-entry failures authenticate before allocating the Session and retain the original entry span. Managed interpreter/native/standalone errors use `CGR090`; the mixed bridge uses `CGR080`. In the committed UTF-8 fixtures their byte spans agree exactly across targets: managed `40..99`, mixed `126..200`. These are synthetic reserve-1 failures through existing test hooks, not real host-OOM observations. Entry recovery remains out of scope.

## Verification and retained failures

All commands used Rust 1.98.1, offline/locked existing dependencies, `CARGO_BUILD_JOBS=2`, `LIBRARY_PATH=/tmp/cgx2-wave22-lib`, and private `target/release-managed-core-repair` / `target/tmp-managed-core-repair` directories. No root manifest/lock or historical runner/fixture changes are included. Exact commands, exits, complete compressed logs, code/binary/runtime/source hashes and earlier attempts are in `managed-core-repair-evidence/verification-final/verification.json` and the referenced files.

* Affected sema/package/CLI/API-index/emitter/compiler tests with `--all-features --no-fail-fast`: **433 passed, 1 failed**, exit 101. The sole failure is the unchanged emitter layout test at `cgx2-emit-rust/tests/s2a_record_identities.rs:49`: it calls the now-strict single-source constructor with a merged AST and concatenated source. This is the prohibited authentication seam, not a package route rejection. Coordinator explicitly owns its mechanical call-site migration. A separately retained run excluding this one call site passes 433 tests; final library, focused and post-compatibility CLI checks also pass.
* The bounded repair runner's final attempt passes **116 checks**, including native debug/release and standalone positives, original/bundle/emitter hashes, exact wrong-recovery code/span checks and artifact preservation, matching UTF-8 entry diagnostics, and source/import drift during actual successful rustc invocations. It also runs a **temporary copy** of the unchanged layout test with only its constructor setup replaced by `package.managed_program_v1()`: **every original layout assertion passes**. The original test and assertions remain unedited.
* Unchanged owned-symbol suite: **195 passed**. Unchanged coordinator owned-Result reconciliation: **593 + 17 passed**. Unchanged managed runner: shapes, package, namespace-package and hybrid, **21 each / 84 passed**. Together with the final repair runner these are **1,005 product checks** against the final compiler binaries.
* Affected-package all-feature/all-target Clippy with `-D warnings`, formatting and diff checks pass. Root Cargo files are unchanged.

The repair runner bounds each subprocess to 180 seconds wall time, 120 seconds CPU, 4 GiB address space, 512 MiB file size, and 2 MiB retained output per stream, with process-group cleanup. Historical runners retain their own documented bounds. Cargo job limits are not aggregate cgroup isolation; no uniform subprocess resource limit is claimed for existing Rust tests. Signals do not count as expected error exits.

A final inventory check found a pre-freeze binary race: Cargo CLI tests had replaced the mutable debug executable during earlier product verification. Those outputs are explicitly intermediate. After all Cargo commands finished, `freeze.py` copied the compiler/consumer executables and native helper, recorded hashes, and the complete 1,005 product checks were rerun against those private copies with matching before/after hashes. Probe libraries are separately hashed; standalone emitted programs retain embedded runtime code.

Earlier failed fixture/compilation/Clippy attempts, the first frozen copy's missing-native-helper failure, and the first reconciliation's missing-consumer-binary spawn failure are retained rather than overwritten; the required consumers were then built read-only in the private target. Runner snapshots preserve hashes for earlier successful attempts when the new runner gained stronger assertions.

General fully qualified local nongeneric constructor call effect classification in the read-only `effects.rs` is unchanged: under explicit `effects()` that spelling can still be conservatively `Unknown`. Qualified type/match paths, qualified generic constructors and imported constructors are covered; no wider constructor syntax expansion is claimed here. The initial unsupported-fixture attempt is retained.

Combined three-lane review, full workspace acceptance, leak-enabled external audit, broader region/transfer/entry-recovery work and release measurements remain unrun/open. No installation, external contact/spend, host security change, main merge, publication, delegation or follow-on task was performed.
