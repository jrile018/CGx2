# First-release requirement-to-fixture map

All release gates remain **open**. The machine-readable source is
[inventory.json](inventory.json); [expectations.json](expectations.json) links
the requirements to specific executable or reserved case IDs. A passing small
example establishes its stated profile only. `expected-unsupported` is an exact
negative regression result, not completion of the corresponding release feature.
`future` cases contain no invented syntax; `external` cases are blocked until
their reviewed prerequisites and observations are supplied.

## R01

Canonical syntax, nominal records/variants, exhaustive matching and Option/Result.
`scalar-check`, `scalar-run`, `bounded-loops`, `scalar-record`, `canonical-format`
run the existing scalar/record profile. `nominal-variants-match` reserves S2a's
constructor/type/exhaustiveness, unreachable-pattern, package identity and
interpreter/native parity corpus. Formatter/API round trips must cover those
new forms after S2a review. Current syntax remains provisional pending R14.

## R02

Generics with named constraints and usable standard data.
`generics-standard-data` reserves S2b substitution/inference/arity/constraint
failures across packages, checked list/string indexing and usable error handling
in real applications. Parser or Rust library tests alone cannot close this row.

## R03

Safe managed/owned memory, borrows, region transfers, auto/manual and `no_gc`.
`manual-memory`, `no-gc-memory`, `native-memory` and `no-gc-collect-rejected`
exercise explicit bounded source operations. `ordinary-memory-ownership`
reserves general application aggregates/function lifetimes. S3-M/S4 must add
source/runtime invalid-program rejection, all failure/cancel/unwind paths,
counter/root/cleanup corpus, leak-enabled host assurance and independent audit.
Zero final roots is necessary evidence, not comprehensive memory safety.

## R04

Modules, public effects/capabilities and checked runtime contracts.
`package-effects`, `hidden-effect`, `runtime-requires` and `public-api` exercise
body-derived transitive purity, declared-boundary rejection and dynamic contracts.
`enforced-capabilities` reserves resource authorization and package contract
parity. Unknown/honesty failures and source provenance need the full package
corpus. Security advice cannot replace runtime capability enforcement.

## R05

Installed CLI, stable structured diagnostics and API tooling.
The scalar cases cover check/run/script/fmt/API, arguments, type errors,
UTF-8 package-relative locations and checked overflow. Native build fixtures use
the current companion. The `unified-*-unsupported` cases assert current command
rejections. S2b must integrate all commands, test/property discovery, runtime
arguments/uncaught exits, dependency-versus-symbol errors and structured results.
See [the complete command map](COMMANDS.md).

## R06

Reproducible packages and foreign declarations. `package-effects` and
`native-package` exercise checked local manifest/source packages.
`pinned-packages-foreign` reserves offline deterministic rebuild and complete
compiler/runtime/artifact/environment/version/target pins and drift rejection.
Native/worker/Python owner libraries must be integrated through source before
this row closes. Imports never authorize package installation.

## R07

Explicit Python-only package access. `python-only-package` is blocked pending a
reviewed pinned package/environment and integrated `.cg` workflow. Follow
[FOREIGN.md](FOREIGN.md), then record real identity/exception/lifetime failure
cases. Baseline isolated scalar calls and richer bridge library tests are not
this acceptance. Native tensor execution must remain independent of Python.

## R08

Direct typed LibTorch creation/inspection/math/inference/conversion.
`direct-libtorch-inference` is blocked pending the real CPU source application,
pinned native runtime/model/data and reference output. Validate shape/dtype/device,
copies, release and no-libpython runtime maps; compare the real native reference
before measuring. GPU acceptance needs actual supported hardware. Literal matrix
preview observations alone do not close typed tensor/inference acceptance.

## R09

Native notebooks with retained state and no replay of earlier external effects.
`notebook-local` runs the current pure bounded accumulator profile and reads its
saved output; `notebook-effects-rejected` asserts external admission rejection.
`persistent-notebook-effects` reserves S3-N selected dispatch once, explicit rerun,
clean restart, contract parity, uncertain-outcome/stale recovery, stdout/errors/
tables/plots, sanitized rich output, portable assets and cancellation/cleanup.
Never silently retry an invocation after a possible external effect.

## R10

Rust output and stated portable Python profile. `native-build`, `native-package`,
`native-memory` and `rust-emission` compile and execute admitted examples.
`python-source-cli` reserves integrated Python emission; its Rust library is
currently outside this checker. S3-C must compare outputs/errors, checked numeric
boundaries and contracts in every admitted target; unsupported targets must fail.
S4 supplies independent blinded generated-source readability/edit tasks.

## R11

Bounded integer/Boolean/checked-arithmetic/array-bounds/annotated-loop proofs.
`finite-audit-not-proof` asserts a live proof of `unknown` without a solver.
`live-solver` is blocked until a trusted real Z3/CVC5 executable is supplied.
From the fixture cwd, using independently approved pins:

```sh
cgx2-verify scalar/proof.cg --domain x:0:2 --format json \
  --solver /absolute/trusted/z3 --trusted-solver-sha256 APPROVED_SHA256 \
  --timeout-ms 2000
```

Use `cvc5` if that is the reviewed solver. Preserve exact executable/version,
obligation/source hashes, complete probe/witness transcript and audits. Zero exit
does not mean proof; inspect `proof.state`, selected scope and assumptions. The
baseline timeout is per process and `--max-cases` defaults to 100000. S3-V must
extend the admitted source profile and independently audit bounds/invariants;
unsupported/timeout stays unknown. This is separate from ownership safety.

## R12

Security and performance analysis. `security-performance` reserves integrated
source/package checks, known positive/negative taint/unknown corpus and actual
owned-input map/zip dispatch decisions. Existing advisory scalar tools and CPU
policy libraries do not enable arbitrary `.cg` parallelism or GPU. Include
launch, transfers, synchronization, fallback and planner overhead. Auto remains
sequential without valid correctness/benefit evidence. No busy-host timings here.

## R13

Memory/correctness/runtime/resource acceptance. `memory-audit-resource-gates` is
blocked pending a leak-enabled host, independent audit and real quiet-host owner
runners. Follow [the normative validation plan](../validation-plan.md) with full
supported corpus, faults, cancellation, foreign/native/managed lifetimes and
complete calibrated reference comparisons/uncertainty. Provisional ratios:
owned native runtime ≤1.25× and RSS ≤1.5× Rust; managed throughput ≥70% and RSS
≤2× OCaml; direct LibTorch throughput ≥90% and p95 ≤1.15× each applicable
baseline; tiny C/Python bridge overhead ≤1.25× after calibration. These are
proposed workload-specific gates, not measured results. Memory correctness
blocks release independently of performance.

## R14

Measured LLM/human value and grammar decision. `human-model-grammar-study` is
blocked pending an independently reviewed 60–100 task initial holdout, two model
families, repeated paired trials and genuine blinded human observations. Freeze
versions, prompts, tool budgets, exclusions and baselines before opening the
holdout; keep acceptance/gold/history hidden. Charge failures their complete
budgets. The validation plan proposes noninferiority within five percentage
points and ≥20% lower paired total tokens **and** wall time, with the stated
95% confidence bounds. Do not label synthetic traces, six-task pilots or skipped
studies as release evidence. No recruitment or paid execution is authorized here.

## R15

Release packaging, support matrix, documentation and independent final review.
`release-install-review` points to [outside-checkout smoke](INSTALL.md).
The checker, executable examples and complete inventory are this lane's bounded
delivery. S4 must rerun them on the integrated compiler, review every R01–R14
evidence artifact and resolve every blocker. A documentation pass does not imply
release readiness, a stable ABI, registry, debugger, LSP, general GPU compiler or
mature ecosystem breadth.
