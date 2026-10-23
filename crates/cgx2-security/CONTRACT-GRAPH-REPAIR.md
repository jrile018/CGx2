# Stage 1 contract call graph repair evidence

Date: 2026-10-23. Base: `2b9855c48713794d4cf819bc391813c1260f287b`.
Branch: `riley/repair-analysis-contracts`.
Worktree: `/home/john-riley/.codex/worktrees/9bbd/lang`.

## Delivered behavior

`cgx2-effect-index/src/lib.rs::build_index` now visits each function's `requires`
with parameter bindings, then `ensures` with an additional `result` scope, then
the body after removing that scope. Contracts never see body locals. Existing
call classification, source sorting, effect sets and cycle propagation receive
the additional call sites without a new public API, schema or dependency.

The existing `package_v1::index_checked_package(&CheckedPackage)` path consumes
these sites, so same-module helpers used exclusively in contracts now appear in
`PackageEntry.calls`, including declaration identities and UTF-8 byte spans.
`cgx2-security::package_v1::PackageReport::analysis()` exposes the same graph.
Unknown contract calls contribute existing unknown-policy findings at their
actual source spans. Pure contract helpers do not acquire foreign effects,
capabilities or taint findings.

Ten new regression tests cover:

- Checked and syntax-only contract-only helpers, nested arguments, repeated sites,
  parameters and postcondition result values, and multibyte source prefixes.
- Body-local versus parameter shadowing, a helper named `result` before/in/after
  postconditions, qualified unresolved names and shadowed foreign namespaces.
- Contract edges in transitive effect propagation and recursion groups, with
  separate repeated sites and set-valued effects.
- Public checked package summaries with same-named helpers in three modules,
  exact local declaration identities, nested ordinary local/imported call edges,
  stable JSON and unchanged purity/import rejection boundaries.
- Security report graph access, separate unknown-policy contract locations,
  deterministic JSON/SARIF, no fabricated foreign capabilities, and rejection
  of source mutation before either serializer publishes output.

The original body-only implementation failed the contract-site assertions before
the traversal was added. Initial test development also exposed fixture mistakes
(the grammar prohibits mixing function items and script statements, and function
entries sort by name); those fixtures were corrected before final acceptance.

## Verification actually run

Environment: `rustc 1.98.1 (48a229cea 2026-09-01)`. Commands ran in the worktree
above, offline, with two Cargo jobs and private disk-backed build/temp paths:

```sh
export RUSTUP_TOOLCHAIN=1.98.1
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/repair-analysis-contracts"
export TMPDIR="$PWD/target/tmp-analysis-contracts"
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo test --offline --all-features -p cgx2-effect-index -p cgx2-security
cargo clippy --offline --all-targets --all-features -p cgx2-effect-index -p cgx2-security -- -D warnings
cargo fmt -p cgx2-effect-index -p cgx2-security --check
git diff --check
```

All acceptance commands exited **0**. Both complete owned package suites and
their existing positive/negative and CLI regression corpora passed: **54 tests,
0 failed, 0 ignored** (32 effect-index, 22 security). Clippy passed with warnings
denied; formatting and whitespace checks passed. Local ignored logs are
`target/repair-analysis-contracts/tests.log` and `clippy.log`.

The shipped checked-package CLI corpus also ran using the built binaries:

```sh
target/repair-analysis-contracts/debug/cgx2-effect-index --package crates/cgx2-security/examples/package-audit --format json
target/repair-analysis-contracts/debug/cgx2-security audit --package crates/cgx2-security/examples/package-audit --policy crates/cgx2-security/examples/package-audit/policy.json --format json
target/repair-analysis-contracts/debug/cgx2-security audit --package crates/cgx2-security/examples/package-audit --policy crates/cgx2-security/examples/package-audit/policy.json --format sarif
```

Exit codes were **0, 1, 1**, respectively; security exit 1 denotes expected
advisory findings. Each output, parsed with Python's `json.loads`, equaled its
committed structured evidence at `cgx2-effect-index/evidence/package-example.json`
and `cgx2-security/evidence/package-example.{json,sarif}`. The local comparison
record is `target/repair-analysis-contracts/compatibility.json`.

The host `/tmp` quota blocked sandbox startup while registering `/tmp/.git`.
Normal scoped execution approvals allowed checks to run with the private TMPDIR
above. No unrelated temporary files were removed and no dependencies installed.

## Boundaries and remaining evidence

This is static analyzer/library and standalone advisory CLI coverage of the
current `.cg` profile. Compiler, runtime, installed core runner and shared
interfaces were not changed. Checked contract purity, recursion and imported
termination restrictions remain enforced by the existing loader/checker.

The frozen checker admits some scalar-shadowed calls (for example a `result()`
call in an `ensures` when a helper has that name). The graph conservatively
reports those sites as unknown rather than treating the helper as resolved.
Tests preserve this boundary without changing checker acceptance. Existing
standalone builtin/unresolved classifications remain unchanged.

This repair demonstrates graph completeness for the exercised contract profile;
it does **not** establish a taint-policy bypass or a general security proof. The
bounded explicit-flow taint engine is unchanged. No foreign runtime, solver,
leak-enabled host assurance, performance benchmark, isolation acceptance or
human/model study was run or credited as passed. Those broader release gates
remain outside this bounded repair. No Stage 2 work or other-lane edits occurred.
