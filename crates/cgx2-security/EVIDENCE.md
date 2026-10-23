# S1-10 package effect and security analysis evidence

Date: 2026-10-23. Reviewed base: `0f0b0b9c87d817f39830f3e2b311dbae2ae3e0f3`.
Branch: `riley/package-effect-security-analysis`. Delivery revision is the commit
containing this report (`git rev-parse HEAD` after checkout). Only
`crates/cgx2-effect-index/**` and `crates/cgx2-security/**` are committed.

## Implemented scope

- `cgx2-effect-index/src/package_v1.rs`: `index_package(root)` consumes
  `load_checked_package`; `index_checked_package(&CheckedPackage)` uses typed
  body summaries and loader-resolved public call identities. `Identity` keys use
  relative source plus function name, preserving same-name exports. Related
  declarations include source and UTF-8 byte span. No trusted imported-effects
  objects are synthesized, copied into new trust stores, or accepted from callers.
- `cgx2-security/src/package_v1.rs`: checked package effect/capability policy,
  bounded cross-module positional explicit taint flow, conservative branches,
  shadow restoration and loop widening. Foreign calls are generic taint boundaries
  and their returned taint is unknown. Memory state/aliases remain unknown.
  Source/manifest/canonical provenance is verified on input and before every
  supplied JSON/text/SARIF serializer publishes output.
- Existing single-file structs, functions, policies, commands and JSON schemas
  remain unchanged. Each binary additionally supports `--package ROOT`.
  Package policy schema 2 adds required `allowed_capabilities`; package security
  JSON is schema 2. Effect package JSON has its own analysis tag/schema 1.
- Source-aware standalone analysis works on the reviewed parser/sema/package
  `.cg` profile, including checked imported scalar/record wrappers, branches,
  admitted range loops, and declared Python/native foreign signatures. Invalid
  private/unimported/duplicate/computed/shadowed source rejects through the loader.
  It is not represented as a successfully checked unknown call. Effectful loop
  bodies still reject under the frozen checker; admitted loops report taint unknown.
- Library-only configuration: `AnalysisLimits { max_steps, max_call_depth }` may
  lower positive defaults of 16,384 total package AST steps and 16 call frames;
  zero or larger bounds reject. CLI uses defaults. Existing checked snapshots can
  be passed by library consumers. Accessor data is a snapshot, so custom publishers
  must call `verify_unchanged()` themselves.

Per-rule supported and unknown behavior is documented in
[README.md](README.md). These findings never authorize runtime capabilities,
validate a foreign installation, execute a package or prove vulnerability absence.
There is no compiler/installed `cgx2` integration change in this delivery.

## Verification actually run

Worktree: `/home/john-riley/.codex/worktrees/1675/lang`. Build environment:

```sh
export RUSTUP_TOOLCHAIN=1.98.1
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-analysis"
export TMPDIR="$PWD/target/tmp-analysis"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo test --offline --all-features -p cgx2-effect-index -p cgx2-security
cargo clippy --offline --all-targets --all-features -p cgx2-effect-index -p cgx2-security -- -D warnings
cargo fmt -p cgx2-effect-index -p cgx2-security --check
git diff --check
```

All four final commands exited **0**. Tests: **44 passed, 0 failed, 0 ignored**,
including **22 new package tests** (10 effect-index, 12 security). Final local logs:
`target/release-analysis/tests-final.log` and `clippy-final.log` (ignored build
artifacts). Earlier fixture repairs are retained in `tests-initial.log` through
`tests-eighth.log`; they were followed by the complete passing final suite.
No package installations, timing benchmarks or external runtime calls were run.

New tests cover pure imported wrappers with nonminimal declared upper bounds,
transitive hidden effects/capabilities, exact same-name identities, private and
unimported calls, duplicate functions/modules, computed/shadowed rejection,
imported record construction/comparison purity, native declaration summaries,
inactive branches, branch shadowing, clean literals, cross-file taint forwarding
and source origins, loop uncertainty, recursion/depth/step exhaustion, invalid
limits/policies, source/manifest mutation, identical-byte canonical symlink
retargeting, UTF-8 SARIF columns, deterministic output and command exit codes.

The shipped package under `examples/package-audit` uses harmless `builtins.str`
to demonstrate data reaching a foreign boundary. It is statically checked, never
executed. Independently repeated binary runs produced byte-identical output:

```sh
target/release-analysis/debug/cgx2-effect-index --package crates/cgx2-security/examples/package-audit --format json
# exit 0
target/release-analysis/debug/cgx2-security audit --package crates/cgx2-security/examples/package-audit --policy crates/cgx2-security/examples/package-audit/policy.json --format json
# exit 1: one CGSEC003 and two CGSEC004 findings
target/release-analysis/debug/cgx2-security audit --package crates/cgx2-security/examples/package-audit --policy crates/cgx2-security/examples/package-audit/policy.json --format sarif
# exit 1: same findings
```

Pretty-printed copies of actual JSON/SARIF output are persisted in
`cgx2-effect-index/evidence/package-example.json`,
`cgx2-security/evidence/package-example.json` and
`cgx2-security/evidence/package-example.sarif`. Formatting those saved copies
changes whitespace only. All artifact locations are package-relative.

## Shared dependency reconciliation

No new dependency version was introduced. Owned manifests add existing workspace
`cgx2-package` to both crates and existing `serde_json = "1"` to effect-index.
Cargo generated exactly these three dependency-list additions in root `Cargo.lock`
during verification; the generated root edits were restored before committing.
Coordinator must reconcile those entries when integrating; `--locked` commands
require that reconciliation. Root `Cargo.toml`, root `Cargo.lock`, other tracks
and shared plans are not part of this commit. Pre-existing untracked `.serena/`
metadata remains outside the delivery.

## Stage 3 and external acceptance handoffs (unrun)

S3-A remains dependency-gated on S2's reviewed syntax/types/foreign contracts.
Its owner must adapt `package_v1::Analyzer::expr/statements` and the existing
single-file visitors for each new exhaustive AST form; define record/variant,
collection/alias, generic argument and field taint joins; and reject unsupported
forms explicitly rather than assuming purity. Extend `index_checked_package` and
its builtin handling to consume the reviewed new typed body/capability summaries.
Use the reviewed checked foreign identity/effect declarations to classify new
foreign/tensor calls, preserving unknown external results and the imported trust
boundary. Do not infer honest library behavior from caller annotations or runtime
allowlists. Coordinate installed-command integration outside this lane.

After those dependencies exist, the executable adapter regression handoff is the
four commands above, followed by coordinator-owned integration checks with a
fresh private target:

```sh
CARGO_TARGET_DIR="$PWD/target/release-analysis-integration" TMPDIR="$PWD/target/tmp-analysis-integration" cargo test --offline --workspace --all-features
CARGO_TARGET_DIR="$PWD/target/release-analysis-integration" TMPDIR="$PWD/target/tmp-analysis-integration" cargo clippy --offline --workspace --all-targets --all-features -- -D warnings
```

Create those private directories first and retain the Rust/jobs/library settings
above. Workspace integration and future-type adaptation were **unrun** here.

Release R12 still needs a reviewed independent seeded corpus with expected rule
outcomes and measured misses/false positives. These unit fixtures are regression
evidence, not an empirical accuracy estimate. Once the coordinator supplies that
corpus/policy, its per-package analysis handoff is executable as:

```sh
: "${CGX2_REVIEWED_CORPUS_PACKAGE:?set reviewed corpus package directory}"
: "${CGX2_REVIEWED_POLICY:?set reviewed schema-2 policy file}"
: "${CGX2_SECURITY_OUTPUT:?set output SARIF path}"
target/release-analysis/debug/cgx2-security audit --package "$CGX2_REVIEWED_CORPUS_PACKAGE" --policy "$CGX2_REVIEWED_POLICY" --format sarif > "$CGX2_SECURITY_OUTPUT"
```

Exit 1 is an analyzed package with findings; exit 2 is rejected input, not a clean
case. Independent labeling and aggregate scoring remain blocked on that external
corpus/review and are **unrun**. Real foreign execution, runtime capability
confinement, leak assurance, live solver, GPU, isolation and paired human/model
studies are separate unrun release gates; no result from this advisory deliverable
claims to close them. Real performance measurements await a quiet host.
