# Executable release evidence v1

`python3 bench/release_evidence.py` is the single entry point. It orchestrates
the existing scored worker, isolation selftest, public syntax exporter,
task/trace intake, native/worker measurements and LibTorch measurements. It
does not replace their scoring, task-cluster bootstrap, numerical checks,
baseline separation, or full failure-budget accounting. Old engines, schemas
and historical results are unchanged.

From the repository root, run the bounded local acceptance path:

```sh
export CARGO_BUILD_JOBS=2 RUSTUP_TOOLCHAIN=1.98.1
export CARGO_TARGET_DIR="$PWD/target/release-evidence"
export TMPDIR="$PWD/target/tmp-evidence"
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
python3 -m unittest discover -s bench/tests -q
python3 bench/release_evidence.py collect \
  --plan bench/release-evidence-plans/local.json \
  --out "$CARGO_TARGET_DIR/bundles" > "$CARGO_TARGET_DIR/collection.json"
```

The local plan intentionally has no compiler pin or independent review. It
probes the actual host, imports the two existing unreviewed candidates through
the real intake engine, exports a public context, queues timing jobs, and
attempts the explicitly missing external owner runners. Exit **2** means
the inventory is not ready, **1** means invalid evidence/input, and **0** means
all inventory rows passed (or integrity verified for the `verify` action).
An absent job is `unrun`; a missing prerequisite/review is `blocked`; an
executed failed check, invalid trace, or rejected review is `failed`. Every
observation remains visible; another success cannot erase a failure.

Use the printed `bundle` path as `BUNDLE` below. Reports are deterministic for
that sealed bundle, with no current-host probes during reporting:

```sh
python3 bench/release_evidence.py verify --bundle "$BUNDLE" \
  --expected-sha256 "$(basename "$BUNDLE")"
python3 bench/release_evidence.py report --bundle "$BUNDLE"
```

For independent verification, obtain the expected digest from the operator's
separate authenticated archive, rather than deriving it from an untrusted
download's directory name. Hashes establish integrity, not authenticity of
models, people, judgments, machines or safety audits. An operator must
authenticate external records and reviewers. Nothing in this command recruits
people, contacts a model API, installs packages, changes isolation policy, or
manufactures reviews. The committed [fixture](fixtures/release-evidence/v1/README.md)
is excluded from release credit even if someone later attaches a receipt.

## Immutable bundle and review contract

Each output is a new `<out>/<sha256-of-canonical-manifest>/manifest.json` with
`blobs/<sha256>` objects. The manifest pins the exact plan and registered job
IDs, compiler executable bytes (or explicitly null), all tracked compiler
sources plus Cargo manifests/lock, benchmark Python tools, interpreter,
nonsecret execution environment, host/CPU affinity/load, commands, executable
bytes, exit codes, bounded raw stdout/stderr, raw JSON results, input study
trees, and generated artifacts/sidecars. Study inputs are checked again after
execution; compiler/source/tools are checked again before sealing. A bundle
is private operator material: it contains held-out tests, raw traces and
reviews. **Never give it to a model or participant.** Only the existing
`public-export` output is model context; that exporter retains the hidden
test/reference boundary.

The manifest [schema](schema/release-evidence-v1.schema.json) describes the
wire format. The stdlib verifier additionally checks all referenced bytes,
content address, registered observations, adapter/gate compatibility, commands,
review binding, and import history. An externally edited manifest under its
old address, missing blob, substituted compiler, or modified raw result
rejects. Keep the original directory and digest when copying a bundle. The
collector never writes into an existing sealed directory. Files are ordinary
portable files, not OS-enforced write-once storage; tampering is detected on
verification. Preserve the original and publish a new bundle for any review
or new observation.

An independently authenticated reviewer supplies a JSON receipt with exactly:
`schema_version: 1`, `reviewer_id`, `operator_id`, `decision` (`accepted` or
`rejected`), `result_sha256`, `subject_sha256`, and nonempty `notes`.
The report supplies `review_subject_sha256`; copy that authenticated value
into `subject_sha256`. It binds the result, command, environment, raw artifacts,
candidate and purpose, so a receipt cannot be transplanted onto different
inputs that happen to produce the same summary. The reviewer must differ
from the bundle operator, and the receipt must bind the exact raw result.
The collector does not fill in an approval or validate someone's identity.

```sh
python3 bench/release_evidence.py review --bundle "$BUNDLE" \
  --job JOB_ID --receipt /private/authenticated-review.json \
  --out "$CARGO_TARGET_DIR/reviewed"
```

This creates a new bundle retaining the old manifest as a hashed history blob.
Existing reviews cannot be overwritten. A review cannot turn fixtures,
unsupported hosts, missing compiler pins, incomplete studies, library-only
checks, or failed checks into passing release evidence.

## Plans, imports and adapter commands

A plan has exactly `schema_version: 1`, `purpose` (`release` or `fixture`),
`operator` (the responsible authenticated operator ID), `compiler` (the actual
CGx2 compiler executable path or null), and `jobs`. Each job has `id`, `adapter`,
optional `args` (an argv array, never shell text), `timeout_seconds` (default
180, positive, at most 7200), and `artifacts` (objects with `path` and exact
`sha256`). Only `owner` jobs set `gate`. Use absolute paths for external
assets; relative paths resolve from the repository root. Specify input options
as separate argv entries. `--out` is reserved for the pipeline. Stage private
study trees with at most 512 MiB per job. Raw JSON and each output stream are
bounded to 16 MiB; timeout/output overflow kills the trusted runner process
group and fails the observation. Candidate execution always uses the existing
isolated `score` path, never an `owner` runner.

| Adapter | Existing engine | Arguments supplied by operator |
| --- | --- | --- |
| `isolation-probe` | `scored_candidate.py --probe` | none; probe alone never passes the isolation gate |
| `isolation-selftest` | `isolation_selftest.py` | `--toolchain-root ROOT --compiler-relative bin/rustc` |
| `score` | `scored_candidate.py` | `--pack PRIVATE --task ID --language cgx2\|rust --candidate-workspace SUBMISSION --toolchain-root ROOT --compiler-relative bin/cgx2c` |
| `public-export` | `syntax_export.py` | `--task precedence --variant indent\|brace`; use each registered task/variant |
| `task-study` | `task_evidence.py report` | `--pack PRIVATE/pack.json` and repeated `--run PRIVATE/run.json` |
| `syntax-trace` | `syntax_trial.py` | `--trace PRIVATE/trace.json --export PUBLIC_EXPORT` |
| `native-runtime` | `native_worker_baselines.py` | `--python PYTHON --binary-dir RELEASE_BIN --build-provenance BUILD_JSON` |
| `libtorch-runtime` | `matrix_runtime.py` | `--package PACKAGE --torch-root TORCH --cgx2 CGX2 --binder BINDER --dtype f32 --build-profile release --python-torch PYTHON --cold-reps 5 --warm-reps 30` |
| `owner` | authenticated, integration-specific runner | executable argv; emit owner assessment v1 on stdout |

Native and LibTorch results preserve their raw samples, resource observations,
confidence intervals, baseline separation, exclusions, and route restrictions.
A completed scalar/matrix observation does not pass the native resource or
compute-dominant inference release gate. External runtime/dependency archives
must remain available under the hashes recorded by those engines. No debug
timing is silently promoted to release timing, and no CPU data establishes GPU
support. Syntax trace structural validity is a bounded observation, not a
genuine-human assessment or a final syntax decision. Full syntax assessment
must reuse `syntax_trial.paired_report` for the registered pilot grid and
report its limited scope, then supply actual preregistered broader data.

`task-study` invokes the unchanged v3 intake before any analysis. It requires
the independently reviewed 60–100 task initial holdout, two model families,
repeated paired runs, exact tokenizer artifacts, frozen baseline choices,
hidden-test receipts and task-cluster confidence reporting. Invalid provenance
fails; absent/incomplete pairs block without a comparison. Failed/timeout
attempts retain their full budget charges in the underlying analysis. For
release credit, the frozen protocol's `compiler_sha256` must match the sealed
compiler bytes; `cgx2_source_sha256` and `tools_sha256` must match the source
and tool snapshot digests. Collect an empty-job candidate bundle first to
obtain these digests before preregistration. Freeze the candidate and tools
before launching trials. A passing holdout does not establish human value.

To import a real external worker's sealed results, use an empty-job plan with
the same purpose/operator/compiler and unchanged source/tool candidate:

```sh
python3 bench/release_evidence.py collect --plan /private/import-plan.json \
  --import-bundle /private/worker-bundles/DIGEST \
  --out "$CARGO_TARGET_DIR/combined"
```

Repeat `--import-bundle` for additional workers. Exact snapshots must match;
mixed candidates, duplicate job IDs and fixture-to-release promotion reject.
Each original manifest and all its raw blobs remain archived. External
observations must first be collected by this command on the worker; arbitrary
legacy summary JSON without its original provenance is not accepted as a
release bundle. Historical reports remain readable in their original engines.

## External execution checklist and finite gate inventory

1. Freeze the integrated release candidate and separately archive its bundle
   digest. Acquire owner-supplied reviewed runners and actual pins. No local
   library test can establish unsupported `.cg` semantics.
2. Prepare a worker with **already delegated** cgroup v2 `cpu,memory,pids`,
   supported namespaces, seccomp, Landlock, bubblewrap and the read-only pinned
   toolchain specified in [isolated runner](isolated-candidate-runner.md).
   Launch the command inside the worker's already provisioned delegated scope;
   the existing engine discovers membership from `/proc/self/cgroup`, not an
   environment override. Do not change host policy from this command.
   Run the full selftest job above; all twelve real positive
   and adversarial cases must pass before a `score` job executes. Put the
   selftest before scoring in the same plan with the same toolchain root;
   an imported selftest from another host cannot authorize local scoring.
3. On a leak-enabled host, run an `owner` job with `gate: "leak-check"` and
   `args: ["/absolute/owner/leak-runner", "--candidate", "PIN"]`. The actual
   integrated memory owner runner must exercise the full supported source
   corpus with leak detection enabled and emit the assessment below. A
   host-denied LeakSanitizer run remains blocked. ASan without leak detection
   cannot close it.
4. For live proofs use `gate: "trusted-solver"` and
   `args: ["/absolute/owner/solver-runner", "--candidate", "PIN", "--solver", "/pinned/solver"]`.
   It must check identity, source/SMT agreement, valid and false cases, witness
   validation and unknown handling. These are owner-runner arguments, not a
   promise of an already implemented shared compiler command. Absent runners
   or solvers block. Do not install a solver or count an unchecked result.
5. Collect actual external model traces, independent task reviews, genuine
   human comprehension decisions, and blinded generated-code edit/readability
   assessments under the frozen protocol. The pipeline creates none of them.
6. Queue runtime work manually until other chats/builds stop. Use a plan with
   actual pinned runtime arguments and run `collect ... --quiet-host` only
   after the operator confirms exclusive measurement time. Timing adapters and
   resource owner gates additionally require observed one-minute load <=0.5.
   This is a conservative admission check, not proof the host stays quiet;
   retain raw host observations and independently reject contaminated runs.
   No scheduling automation is created. Missing datasets, tuned OCaml/PyO3/C++
   baselines, actual inference workloads or device evidence remain unrun.

The owner runner emits the [owner assessment](schema/release-owner-v1.schema.json)
on stdout. It lists `schema_version`, `gate`, `status`, `scope` (`level` is
`source`, `library` or `harness`, with explicit `supported` and `unsupported`
lists), `limitations`, `checks`, and `evidence_sha256`. Register raw evidence
files in the job's `artifacts` with exact hashes before running; the result
must reference those hashes. For newly generated observations, use a two-step
owner handoff: run the owner's real study into its own immutable private
archive, then execute its assessment runner against that archive through this
plan. Each gate's exact required `checks` keys appear in every `report` and
in [release_gates.py](release_gates.py). Every check must be true for pass,
the scope must establish source-level acceptance, and independent review is
still required. Library/harness observations remain explicitly blocked for
source gates. Unavailable owner output may state `blocked` or `unrun`.

The complete finite inventory is the ordered `GATES` map: isolation, submission,
public context, holdout model study, syntax trace observation, syntax decision,
native observation, LibTorch observation, memory safety, manual/no_gc,
regions/foreign lifetime, leak assurance, functional source conformance, trusted
solver, managed resources vs OCaml, native resources vs Rust, LibTorch inference
vs C++ and Python separately, native FFI, Python bridge, automatic dispatch,
human comprehension, generated-source edit/readability and cold/warm CLI/cell
latency. Each maps to R01–R14 and a visible acceptance checklist. R15 packaging
and final release review remain coordinator responsibilities; this empirical
inventory does not claim to certify the entire language or release packaging.
