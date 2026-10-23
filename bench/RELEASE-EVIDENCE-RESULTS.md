# S1-11 executable release evidence delivery

Implemented from reviewed base `0f0b0b9c87d817f39830f3e2b311dbae2ae3e0f3`
on `riley/release-evidence` in the attached `5b29/lang` worktree. Changes are
limited to `bench/**`. Root Cargo files, shared plans and historical benchmark
results are unchanged.

## Supported scope

The executable `bench/release_evidence.py` collects, verifies, imports, reviews
and reports content-addressed evidence through existing isolation/scoring,
public export, trace intake, confidence, native/worker and LibTorch engines.
It adds no CGx2 language semantics. Source-level release acceptance remains
dependent on integrated owner runners and actual evidence; Rust library tests
cannot close those gates. A finite 23-row inventory includes all empirical
requirements plus explicitly separate bounded harness observations.

New schemas, local plan, a tiny blocked fixture bundle, 44 validation tests,
and [operator recipes](RELEASE-EVIDENCE.md) accompany the command. Review
receipts bind raw command/environment/artifacts and the compiler/source/tool
candidate. Imports preserve original manifests and results; fixture promotion,
mixed candidates, duplicate jobs, changed bytes and incomplete provenance
reject. No historical bundle or review is overwritten. Independent identity
authentication and separate digest archival remain operator responsibilities.

## Verification on 2026-10-23

- `python3 -m unittest discover -s bench/tests -q`: **166 passed**, exit 0
  (all 122 existing tests plus 44 new checks). Includes actual temporary owner
  subprocess capture, timeout/output exhaustion, missing runners, tampering,
  missing provenance, independent-review binding, import preservation/rejection,
  incomplete genuine-study intake, source/library distinction, and quiet-host
  gating. Synthetic temporary receipts are test fixtures only.
- `python3 -m py_compile bench/release_evidence.py bench/release_gates.py bench/tests/test_release_evidence.py bench/fixtures/release-evidence/v1/generate.py`:
  exit 0.
- Draft 2020-12 schema validation of the actual collected manifest, fixture
  manifest and owner assessment: passed. Both schema definitions also validate.
- Fixture `verify --expected-sha256 348decc7409f767400381bdd4c5e6984aebe77b5c772330c4d4fb09f37ef3574`:
  exit 0. Fixture reports never earn release credit.
- Actual `collect` using the local plan plus an installed Rust 1.98.1
  isolation-selftest job: exit **2**, expected incomplete release status.
  `verify` on that sealed archive: exit 0. Two separate CLI reports were
  byte-identical and both exited 2. `git diff --check`: exit 0.

The full executed plan, command/exit records, raw observations, provenance and
all gate statuses are in [the owned JSON report](results/release-evidence-functional-v1.json).
The full local archive is under ignored
`target/release-evidence/functional-bundles/b26cf9b2344d0de9b9609277c08f3abcba9749d38ba4298f6bfe4b7d1853c692`.
Its independently verifiable digest is
`b26cf9b2344d0de9b9609277c08f3abcba9749d38ba4298f6bfe4b7d1853c692`.
The committed JSON is a report, not a partial replacement for the complete
archive. The collector command reproduces a fresh fully sealed local archive.

No Rust sources or Cargo dependencies changed, so Cargo tests/Clippy/fmt were
not run in this Python-only lane. No release benchmarks ran alongside the
parallel completion chats. `CARGO_BUILD_JOBS=2`, the private release-evidence
target/temp directories and Rust 1.98.1 are enforced for child jobs.

## Actual blockers and unrun acceptance

The real local report has **0 passed, 0 failed, 7 blocked, 16 unrun** gates.
The actual isolation probe and full selftest entry point both report
`cgroup_controllers_not_delegated:cpu,memory,pids`; no positive/adversarial
fixture executes on this unsupported host. A capable worker must execute and
pass all twelve real cases before scoring in that same plan/toolchain.

The existing two-task candidate intake returns `insufficient_evidence` with
no comparison: independent task reviews, frozen protocol and registered real
trials are absent. Public export completes, but no CGx2 compiler was selected
in this blocker plan and no independent review is supplied. Native/LibTorch
timing jobs stay queued behind explicit quiet-host admission. Both external
owner-runner handoff paths are absent. Leak-enabled assurance, trusted live
solver results, full source conformance, calibrated native/managed/FFI/bridge
resources, compute-dominant inference, dispatch, human comprehension, blinded
generated-source assessment, syntax decision and cold/warm product latency
remain blocked or unrun. The owner schemas/recipes are executable integration
handoffs, not implementations or invented successful results for these gates.

No model API, participant contact, paid study, package installation or host
policy change occurred. No whole-language memory, performance, model/human
advantage, GPU support or release-readiness claim is made.
