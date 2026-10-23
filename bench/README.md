# CGx2 baseline and pilot benchmark harness

The versioned [release evidence command](RELEASE-EVIDENCE.md) composes these
engines into immutable, provenance-checked bundles with a complete empirical
gate inventory. Its local plan reports external blockers and does not run
performance benchmarks without explicit quiet-host admission.

This is an on-demand, standard-library-only harness for calibrating tasks and measurement noise. It does not invoke a model, install dependencies, implement CGx2, or make a release judgment. The older smoke and sample-result fixtures are synthetic or harness-only and are **not evidence of a CGx2 result**. The Wave 4 pilot below records real CLI invocations but is too narrow for a release claim.

## Wave 4 executable scalar pilot

The versioned [six-pair corpus](fixtures/pilot/v1/manifest.json) runs against the Wave 3 checked scalar CLI and Python. It covers fixed arithmetic, nested calls, mutation, argument count, a Unicode argument, and checked i64 overflow. The `.cg` and `.py` sources are frozen public fixtures, each with an independent expected stdout and success/error class. Overflow requires `CGR005` from CGx2 and `PILOT_I64_OVERFLOW` from Python. This corpus uses no loops, arrays, FFI, memory operations, notebooks, or Python libraries beyond `sys`. The Python overflow case explicitly checks its result because Python integers do not overflow at i64 boundaries.

From the repository root:

```sh
python3 -m unittest discover -s bench/tests
python3 bench/pilot.py parity --build
python3 bench/pilot.py runtime --cgx2 target/debug/cgx2 --warmups 2 --repetitions 10
```

`--build` runs `cargo build --offline -p cgx2-cli` before measurement and records its separate wall time. `--cgx2 PATH` uses an existing executable. `--python PATH` selects the interpreter; the default is the interpreter running `pilot.py`. Both modes accept `--manifest PATH`, `--timeout SECONDS` (default 5), and `--out-dir PATH`. The runtime mode also accepts `--warmups N` (default 2, minimum 1) and `--repetitions N` (default 10, minimum 10). The default output is a new unique directory under `/tmp`; an explicit output directory must not exist. Parity writes `parity.json`, and runtime writes `runtime.json` with its own full parity preflight. A failed preflight prevents timing, exits nonzero, and remains in the JSON. Failures during timing retain their raw samples and invalidate the summary. A missing executable, check failure, timeout, wrong stdout, wrong error class, or unexpected stderr is explicit.

The output records every source and executable SHA-256, host, CPU, compiler and interpreter versions, arguments, workload description, check and run status, exact deterministic stdout comparison, stderr diagnostic excerpt, and per-process wall time. Runtime records order and phase for every sample, alternates which language starts each pair, and excludes warmups from median and interquartile range. The metric is **fresh subprocess wall time**, including interpreter/CLI startup and excluding the separately measured build. The first parity invocation is recorded but is not a controlled cold-start benchmark. These tiny tasks are dominated by startup and cannot establish native CGx2 runtime performance. A future native benchmark needs behaviorally equivalent long-running or in-process workloads, pinned CPU affinity and thread counts, repeated independent processes, and separate cold/warm reporting. Do not quote a ratio without the JSON raw samples and validity fields.

The `utf8_bytes`, `characters`, `lines`, and `lexical_tokens_descriptive` fields describe source length. Lexical tokens are **not model tokens**. To pilot LLM repair quality with the existing `runner.py`, independently review six equivalent task descriptions and acceptance tests, then create one `task.schema.json` manifest per task, language, model version, and seed. Pin identical task prompts, allowed tools, documentation budget, model versions, seeds, dependency locks, time budgets, and randomized task order. Keep hidden acceptance tests outside each model workspace. Record real `trace_path` JSON fields `output_tokens`, `total_tokens`, `iterations`, and `human_interventions`; preserve each failure and its budget charge. Do not infer these fields from source length or invoke a model from this pilot. Six public scalar pairs are a tooling check and noise sample, not a fresh holdout or evidence for first-release LLM productivity.

## Run the public smoke checks

From the repository root with Python 3.14.4:

```sh
python3 -m unittest discover -s bench/tests -v
python3 bench/runner.py --manifest bench/fixtures/smoke.json --out /tmp/cgx2-smoke-result.json
python3 bench/report.py --results bench/fixtures/sample-results --out /tmp/cgx2-smoke-report.json
```

`smoke.json` runs public Python commands under the `harness` language label. `sample-results/` contains fabricated Python/Rust records solely to exercise success, failure, and absent token traces. The `.cg` files in `fixtures/syntax/` are equal-intent *provisional* indentation and brace samples for later trials; neither is compiler-tested.

## Supply a fresh task

Create one manifest per task, language/variant, model version, and seed using [task.schema.json](schema/task.schema.json). Give equivalent variants the same `task_id`, `tier`, `model_id`, and `seed`. `small` means a real 3–10-file repository task; `large` means 20–100 files. Set `language` and `variant` explicitly, such as `python`/`standard`, `cgx2`/`indent`, or `cgx2`/`brace`. The smoke fixture is exempt from real-task tier sizing because it measures only the harness.

Supply `argv` and `hidden_test_argv` as nonempty JSON arrays of arguments. The runner rejects shell strings and shell metacharacters and uses `shell=False`. The `cwd` is relative to the manifest's directory. Both commands share one `timeout_seconds` wall budget; if either times out, the runner kills the subprocess group and marks failure. The first command must exit with `expected_exit`; the hidden test must exit zero. The manifest should point to the hidden test *entry point*, with acceptance tests and gold changes stored outside the model workspace. The result stores only hashes and exit codes, never captured test output or test source. Run manifests only from trusted task authors: an argv command is still executable code.

Pin the model and tool versions in `model_id` and `versions`; include compiler/interpreter, dependencies, lockfile digest, hardware/device, thread count, prompt and documentation budget IDs, and task template revision as applicable. Put controlled nonsecret environment variables in `environment`. Only `PATH`, `LANG`, and explicitly listed environment variables reach subprocesses; report provenance records their nonsecret values and the manifest hash. Keys that look like credentials are redacted from the result. Keep secrets out of manifests and model workspaces where possible. Freeze the task definitions, prompts, tools, dependency locks, task order, and acceptance criteria before a holdout run. Independently review task equivalence and adjudicate valid alternative solutions.

If model usage is available, set `trace_path` relative to the manifest. Its JSON object may contain nonnegative integer `output_tokens`, `total_tokens`, `iterations`, and `human_interventions`; `output_tokens` cannot exceed `total_tokens`. Missing trace fields remain `null`. The runner does not estimate token usage from text. The `budget.total_tokens` is the maximum charged for a failed run. A run result conforms to [result.schema.json](schema/result.schema.json). Run it with:

```sh
python3 bench/runner.py --manifest path/to/task.json --out path/to/results/task-language-variant-model-seed.json
```

The runner writes one JSON result per invocation. Preserve every run, including failures, and keep IDs unique. Use a fresh task set for a six-task pilot to assess task quality, noise, and power; that pilot cannot establish a release claim. Publish holdout definitions only after retirement.

## Build a paired report

```sh
python3 bench/report.py --results path/to/results --out path/to/report.json
```

The report reads `*.json` in one directory and rejects duplicate condition/case keys. A condition is `(language, variant)`; a matched pair shares `task_id`, `tier`, `model_id`, and `seed`. Unmatched cases appear in `unmatched_left` and `unmatched_right` counts and do not enter paired metrics. Completion difference is `right - left`; time and token ratios are `right / left` among tasks verified in both conditions. Output tokens and total tokens are separate. Absent token values stay missing, so a token ratio uses only pairs with traces on both sides.

For every condition, `all_task_charged_seconds` uses each failure's full `timeout_seconds` budget, including early failure. `all_task_charged_tokens` uses each failure's full `budget_total_tokens`; if a successful task lacks token usage, the aggregate token charge and tokens per verified success are `null`. This prevents false zero-cost success. The report includes cost per verified success, completion rate, success-only median wall time, token medians, and missing-trace counts.

With at least 30 eligible matched pairs for a metric, the report computes a deterministic 10,000-resample paired bootstrap 95% percentile interval using seed 0. Completion uses all matched pairs; time and token ratios require at least 30 pairs completed in both conditions with usable measurements. Smaller sets say `insufficient_data`. The report does not assert statistical significance or a CGx2 win. Apply the release gates in [the validation plan](../docs/validation-plan.md) only after an independently reviewed holdout and a real CGx2 implementation exist.

## Wave 7 native scalar toolchain pilot

The [native v1 manifest](fixtures/native/v1/manifest.json) pairs all six frozen `.cg` cases with hand-written Rust references. Each reference uses checked `i64` arithmetic where relevant, the same CLI argument indexing and Unicode output, and the same success/error class. The overflow comparison requires `i64 arithmetic overflow` on stderr from both binaries. These public cases are tiny scalar toolchain checks; they use no owned/managed runtime, collector, Python bridge, or model trace. Independent review of workload equivalence remains a release prerequisite.

From the repository root:

```sh
cargo build --offline -p cgx2c
python3 bench/native_pilot.py --cgx2c target/debug/cgx2c --warmups 2 --repetitions 10
```

The command writes a uniquely named `/tmp/cgx2-native-pilot-*.json` and prints its path. `--out PATH` chooses a new file; it never overwrites an existing file. `--manifest`, `--rustc`, `--timeout` (default 10 seconds), and `--build-timeout` (default 120 seconds) select inputs. Each `.cg` file is built by `cgx2c build --release --rustc RUSTC`; each reference uses `rustc --edition=2024 -O`. Build wall times, commands, source/binary SHA-256, `rustc` version, `cgx2c` SHA-256, host, CPU, logical CPU count, and thread environment are recorded. Compilation failure remains in the JSON and blocks timing.

The first subprocess launch after all builds is a parity preflight and is also listed under `cold_launch`; its OS cache state is uncontrolled. Exact stdout, exit status, success/error class, overflow marker, and success stderr must agree before timing starts. Warmup and timed launches are fresh subprocesses in alternating condition order. Every raw sample, including failures and timeouts, is preserved with order and phase. Only valid timed samples receive medians, quartiles, and a deterministic 2,000-resample bootstrap 95% interval for each condition's median (seed 0). The interval describes sampling uncertainty in this local launch workload, not performance across machines. Build, first launch, and repeated launches are separate fields. These startup-dominated durations cannot support the native owned/managed runtime target; do not compare them with optimized application workloads or quote a speedup without the raw JSON.

## Task-matrix audit before any LLM comparison

Run the read-only v1 audit on trusted local manifests and results:

```sh
python3 bench/matrix_audit.py --matrix path/to/matrix.json --out /tmp/cgx2-matrix-audit.json
python3 bench/matrix_audit.py --matrix bench/fixtures/matrix-calibration/v1/matrix.json
python3 bench/matrix_audit.py --matrix bench/fixtures/matrix-calibration/v1/matrix-invalid.json
```

Exit codes are `0` for mechanically `ready`, `1` for `incomplete`, and `2` for `invalid`. The second command is intentionally incomplete public calibration material: it has no paired CGx2 entry, model result, or trace. The third deliberately points hidden tests into the starter workspace and is invalid. Neither is a hidden task, a model run, or a CGx2 outcome.

The matrix JSON has `schema_version: 1`, `kind: public_calibration` or `holdout`, `conditions: [{language, variant}, ...]`, and `entries`. Each entry names a `task.schema.json` v1 `manifest`, a `result.schema.json` v1 `result`, the starter `workspace`, external `hidden_tests` entry point and `gold_changes`, plus a nonempty `forbidden_markers` list of task-specific answer/history canaries. Paths are relative to the matrix file. The manifest's `cwd` must resolve to the workspace, and its `hidden_test_argv` must name the external test entry point. The runner and audit read only trusted local files; the audit does not execute commands.

The starter file-count rule counts every regular file recursively under the starter workspace, including README/config/source files. It excludes `.git`, `.hg`, `.svn`, `target`, `__pycache__`, `node_modules`, `.venv`, `venv`, and `.pytest_cache` directories; symlinked files are rejected. `small` requires 3–10 counted files and `large` 20–100. Keep generated dependencies out of the starter. `.git` history in the workspace, hidden tests or gold inside it, or a forbidden marker in any counted file is invalid.

The audit checks unique `(task_id, tier, model_id, seed, language, variant)` keys, all declared paired conditions, identical paired prompt/tools/hardware/thread pins and budgets, model family/version IDs, manifest/result IDs and SHA-256 provenance, saved trace consistency, nonnull model token/iteration/intervention values, and full timeout/token-budget recording for failed runs. A missing result, trace, pin, pair, or holdout freeze is incomplete. A duplicate key, contamination, wrong tier size, mismatched provenance, or inconsistent result is invalid. `ready` means only that these mechanical checks passed; it cannot establish that traces are genuine, tasks are equivalent, or a holdout was kept private. The JSON carries findings and a reviewer checklist.

For a future holdout, freeze 60–100 independently reviewed fresh tasks before opening them. Use at least two model families with explicit versions and repeated paired seeds. Set `kind: holdout` and include `freeze` fields `task_set_sha256`, `prompts_sha256`, `tools_sha256`, `locks_sha256`, `acceptance_sha256`, `order_seed`, `frozen_at_utc`, and `reviewer_ids`. Pin prompts, allowed tools, documentation budget, time/token limits, dependency locks, model versions, hardware, threads, and randomized task order. Store hidden tests and gold changes outside every model workspace and strip future history. An independent reviewer should adjudicate equivalent starter repositories and acceptable solutions before freeze. Preserve every failed run at full budget charge. After retirement, publish task definitions, failures, exclusions, changes, and paired uncertainty; do not infer LLM productivity from this audit or the six public scalar cases.

## Wave 9 paired holdout protocol (matrix and trace v2)

The [v2 matrix contract](schema/holdout-matrix-v2.schema.json) and [v2 trace contract](schema/holdout-trace-v2.schema.json) extend the v1 pilot without changing its commands or public output. `bench/holdout.py` never calls a model. The [public calibration templates](fixtures/holdout/v1/matrix.json) cover the small (3 files) and large (20 files) tiers and memory, modularity, foreign-library, and comprehension/repair themes. Their acceptance files and gold patches are **placeholders**. They have not passed independent task-equivalence review, are not a private holdout, and have no model traces. Their intentionally incomplete report shows no completion rate.

For a real experiment, independently review task statements and equivalent per-language `starters` trees. Each task selects `baseline_language` before seeing holdout results and partitions the five conditions into `applicable_languages` and explained `exclusions`. Hidden tests and gold changes must live outside *every* starter tree. In a private holdout, use distinct starter paths per language and record `freeze` hashes, order seed, reviewers, and time. `freeze.task_set_sha256` hashes the compact, sorted-key UTF-8 JSON serialization of `tasks`; `freeze.acceptance_sha256` hashes the compact UTF-8 JSON serialization of the sorted `[task_id, hidden_test_sha256]` pairs. The other freeze hashes identify externally frozen prompt/tool/lock materials for operator review. The audit checks path separation, 3–10/20–100 counted files, no starter Git history or symlinks, task-specific forbidden markers, duplicate task/run IDs, paired condition coverage, two model families, repeated seeds, and fixed tool/documentation/token/time/hardware/toolchain budgets. Keep a private acceptance oracle and a frozen hidden-test version; the agent sees only the task statement and its own starter. The operator must verify the hidden result independently. The audit cannot prove review quality, trace authenticity, or that a private file stayed secret outside the recorded starter tree.

Pre-register every run in `runs`, including its model family/version, seed, randomized condition `order`, run ID, and external trace path. Each task registers `hidden_test_version` and `hidden_test_sha256`; the audit compares the latter with the exact hidden file bytes. A trace must have exactly the v2 required fields in [the schema](schema/holdout-trace-v2.schema.json), plus optional `tool_tokens` and `tokenizer_version`; **both are required when `kind` is `holdout`**. `prompt_tokens` counts actual model input, including tool messages subsequently sent to the model; `completion_tokens` counts actual model output; `tool_tokens` counts tool or diagnostic payload tokens not already included in either count, using the named `tokenizer_version`. These buckets must be disjoint. The trace also records actual `wall_seconds`, `final_patch_sha256`, `hidden_test_result` (`pass`, `fail`, or `timeout`), `failure_cause`, and the frozen model, run, hardware, tool, documentation, and budget identifiers. The sum of the three token buckets must fit `token_budget`. The version-2 file limit is 64 KiB per trace and 2 MiB per matrix; patch inputs are limited to 10 MiB. A timeout or early failure consumes the full token and wall budget in all-task cost-to-success. Successful runs use measured tokens and time. Do not substitute source lexical tokens, synthetic model output, or a six-case native pilot result.

An external operator can save a trace without any model API call:

```sh
python3 bench/holdout.py ingest --matrix PRIVATE/matrix.json \
  --trace-input PRIVATE/external-observation.json --patch PRIVATE/final.patch \
  --out PRIVATE/traces/registered-run.json
python3 bench/matrix_audit.py --matrix PRIVATE/matrix.json --out /tmp/cgx2-holdout-audit.json
python3 bench/report.py --matrix PRIVATE/matrix.json --out /tmp/cgx2-holdout-report.json
python3 bench/holdout.py report --matrix PRIVATE/matrix.json \
  --out /tmp/cgx2-holdout-report-2.json --markdown /tmp/cgx2-holdout-report.md
# Read-only external observations; repeat --trace-input once per registered run.
python3 bench/holdout.py report --matrix PRIVATE/matrix.json \
  --trace-input PRIVATE/external-run-1.json --trace-input PRIVATE/external-run-2.json
```

`ingest` checks the registered run identity, hidden-test version, protocol pins, numeric bounds, and final patch SHA-256, and writes only to that run's registered trace path. The `report` and `audit` actions can instead read one or more `--trace-input` paths, match them by `run_id`, and write nothing when `--out` and `--markdown` are omitted. These actions never launch a model or open a holdout automatically. The external observation must come from the independent test operator. All audit and report commands return `0` when mechanically ready, `1` when incomplete, and `2` when invalid. Duplicate, unknown, or incompatible trace identities are invalid; a missing registered run is incomplete. An incomplete or invalid matrix has **no numeric completion rate or gate verdict**, even if some traces exist. The public template demonstrates this:

```sh
python3 bench/holdout.py report --matrix bench/fixtures/holdout/v1/matrix.json
```

## Wave 12 candidate tasks and version 3 evidence import

The [six-task candidate pack](fixtures/task-candidates/v3/pack.json) covers memory-sensitive repair, package/API use, a native C boundary, pure notebook comprehension, proof-result interpretation, and numerical edge cases. It plans three 3–10-file and three 20–100-file repository tasks. Each entry states language-neutral acceptance criteria and selects the strongest proposed applicable baseline **before** results. The pack has no actual starter repositories, independent reviews, hidden acceptance tests, model traces, or frozen compiler/dependency hashes. Its `status: candidate` entries are planning fixtures and ineligible for a holdout claim. An opaque `hidden_test_ref` is a future private identifier, not an acceptance test. Do not put private tests or gold changes into any model-visible starter.

Run the standard-library-only summary and honest report smoke from the repository root:

```sh
python3 -m unittest discover -s bench/tests -q
python3 bench/task_evidence.py summarize --pack bench/fixtures/task-candidates/v3/pack.json
python3 bench/task_evidence.py report --pack bench/fixtures/task-candidates/v3/pack.json
```

The last two commands intentionally exit `1`. Their JSON shows six candidates, three per tier, zero validated runs, and `status`/`release_verdict: insufficient_evidence`. `summarize` lists category counts, exclusions by language, registered and validated run counts, and every missing or invalid prerequisite **before** any metric is computed. `--out PATH` writes that JSON to a separate derived report file. `validate` uses the same audit with no metric calculation. No command invokes a model or acceptance test.

Version 3 is opt-in. It does not migrate or reinterpret v1 task/result files or v2 holdout matrices/traces; the Wave 11 report and task-clustered scoring remain unchanged. For a real holdout, independently review equivalent starters and hidden-test criteria, set every task to `frozen`, provide two distinct reviewer IDs and a review time before the freeze, and place each language's 3–10- or 20–100-file starter in a separate tree. Record `workspace_paths`, SHA-256 `repository_snapshots`, an external `hidden_test_path`, its SHA-256, and the opaque reference. The importer hashes each starter as SHA-256 of compact sorted-key UTF-8 JSON of sorted `[relative_path, file_sha256]` rows. It rejects symlinks, `.git` history, hidden tests in *any* model workspace, shared workspaces, and changed snapshot bytes. A human must still verify task equivalence and reviewer independence; those cannot be proven from IDs.

Freeze all 60–100 tasks across both tiers and six categories. Pin `prompt_sha256`, `tools_sha256`, `documentation_sha256`, `dependency_lock_sha256`, `cgx2_source_sha256`, `compiler_sha256`, `dependency_sha256`, `tool_version`, hardware, time/token budgets, and `randomization_seed` in `protocol`. Register the complete two-family, repeated-seed, paired-language `expected_runs` grid before running models. Set `freeze.task_set_sha256`, `freeze.run_grid_sha256`, and `freeze.protocol_sha256` to SHA-256 of each object's compact sorted-key UTF-8 JSON; set `freeze.frozen_at_utc` after review and before the first run. The [pack schema](schema/task-pack-v3.schema.json) documents fields. The importer checks exact frozen hashes, model versions, pair completeness, order uniqueness, and run chronology.

An external operator stores each genuine [v3 run](schema/run-v3.schema.json) and passes its file with a repeated `--run` argument. Each run references a separate raw trace file and actual tokenizer output file by relative path, with exact SHA-256s. Tokenizer output is JSON containing `tokenizer_id`, `tokenizer_version`, and nonnegative integer `input_token_ids`, `output_token_ids`, `tool_token_ids` arrays. Count input messages, model output, and tool or diagnostic payload **once** in those disjoint buckets. The importer checks array lengths against `prompt_tokens`, `completion_tokens`, `tool_tokens`, and `total_tokens`, and pins one tokenizer/version per model family. The raw trace is JSON with `run_id`, an `events` array, and `verification` containing `verifier_id`, `hidden_test_ref`, `hidden_test_sha256`, `result`, and `final_patch_sha256` matching the run. The external operator must actually execute the hidden acceptance test and save its result; a matching file alone cannot prove trace authenticity. Keep both evidence files and the run JSON outside all model workspaces.

```sh
python3 bench/task_evidence.py summarize --pack PRIVATE/pack.json --run PRIVATE/runs/a.json --run PRIVATE/runs/b.json
python3 bench/task_evidence.py report --pack PRIVATE/pack.json --run PRIVATE/runs/a.json --run PRIVATE/runs/b.json --out PRIVATE/derived-report.json
```

Only a fully reviewed, frozen, complete run set reaches the unchanged Wave 11 `analyze_ready` scoring. A pass needs matching external hidden-test verification, positive measured time and token use, and no failure cause. A fail or timeout needs a cause; the analysis charges its **full** wall and token budgets even when the recorded failed attempt ended early. Any missing, duplicate, leaking, mismatched, out-of-order, or unpaired evidence yields `insufficient_evidence`, with no numeric completion rate or superiority verdict. A ready report separates each task's preregistered baseline (including Python) and reports task-clustered confidence intervals, paired-success tokens/time, and all-task cost-to-success as described below. Creating this pack supplies no LLM advantage evidence.

## Wave 13 executable candidate set

The [Wave 16 review packet](wave16-review-packet.md) records the current
task specifications, independent reviewer fields, hidden-oracle boundary,
and source/toolchain hashes. Scored execution now requires a verified cgroup
v2 scope with aggregate CPU, memory, and process limits; unsupported hosts
return `unsupported_isolation` with no candidate run.

The Wave 14 [review and measurement protocol](candidate-review-protocol.md),
[`task-cards.json`](fixtures/task-candidates/v3/task-cards.json), and
[`evidence-unrun.json`](fixtures/task-candidates/v3/evidence-unrun.json)
add independent review fields, workspace-bound hidden acceptance, and an
explicit no-trials evidence packet. Reviewer decisions remain blank.


The v3 pack now contains two runnable repair candidates, both paired with a
Rust baseline selected before model traces:

| Candidate | Tier and starter files | Correctness target |
| --- | --- | --- |
| `quote-credit-v3` | small: 4 CGx2, 4 Rust | Apply a customer credit once after shipping; `quote(7,120,18,34) = 824`. |
| `grid-quote-v3` | large: 21 CGx2, 20 Rust | Apply each of 14 named charge rates to normalized usage once; `total(10) = 2810`. |

The shared language-neutral statements are under
`fixtures/task-candidates/v3/tasks/<id>/statement.md`. The model-visible
workspaces are only their `cgx2/` or `rust/` starter directories. The oracle
and reference overlays are in `fixtures/task-candidates/v3/private/`, outside
both workspaces. The pack pins SHA-256 bytes for prompts, oracle, starters,
and reference overlays, plus concrete build and run argv, scope, allowed
tools and docs, expected outcomes, and cross-language equivalence metadata.
The starter builds in both languages and returns a wrong value; the reference
repair passes normal, zero, group (large task), and overflow cases. This
negative fixture is a behavior failure, not a compile failure.

The private oracle without a submitted workspace is a starter/reference preflight. A
trusted local fixture smoke can pass `--trusted-local-smoke`, `--task`, `--language`,
and `--candidate-workspace`. The pack's `acceptance_command` deliberately omits
the trust flag, so it refuses to execute a submitted workspace on this host.
Untrusted holdout submissions require a separately reviewed isolated worker
with no candidate access to the private oracle or network before scoring.
The oracle copies that workspace into a temporary directory before injecting test
entry points; it does not copy the bundled reference overlay for a candidate.
The oracle and its expected values remain outside the model-visible workspace.

From the repository root, reproduce the correctness checks offline:

```sh
cargo build --offline -p cgx2c
python3 bench/fixtures/task-candidates/v3/private/accept.py --task quote-credit-v3
python3 bench/fixtures/task-candidates/v3/private/accept.py --task grid-quote-v3
python3 -m unittest discover -s bench/tests -q
python3 - <<'PY'
import json
from pathlib import Path
import jsonschema
schema = json.loads(Path('bench/schema/task-pack-v3.schema.json').read_text())
pack = json.loads(Path('bench/fixtures/task-candidates/v3/pack.json').read_text())
jsonschema.Draft202012Validator.check_schema(schema)
jsonschema.Draft202012Validator(schema).validate(pack)
PY
python3 bench/task_evidence.py validate --pack bench/fixtures/task-candidates/v3/pack.json
python3 bench/task_evidence.py report --pack bench/fixtures/task-candidates/v3/pack.json
```

The last two commands intentionally exit 1 and report
`insufficient_evidence`: there are no independent reviews, frozen protocol
pins, preregistered runs, or real model traces. The acceptance suites are
correctness checks, not timing benchmarks. Verified on the Wave 12 base
`a2a35ba0ac1a44e425cbb39c457cebd0ee980b2f` with Python 3.14.4,
rustc 1.98.1, and Cargo 1.98.1. Rust's overflow panic text and CGx2's
overflow diagnostic differ; both conditions require the same nonzero
overflow class. The CGx2 starters use only frozen scalar package calls and
checked i64 arithmetic; no source memory graph, foreign call, notebook,
proof, or newer Wave 13 API is exercised.
The pinned build/run commands were each run twice with identical starter
output. `cargo fmt --all -- --check` and targeted `cargo clippy --offline -p
cgx2c --all-targets -- -D warnings` could not run here because this host's
stable Rust toolchain lacks the `rustfmt` and `clippy` components; no Rust
source was changed in this lane.

Reviewer checklist before any task is marked `frozen`:

1. Independently inspect each statement, starter, hidden case, and reference
   change; confirm the task is meaningful and the same behavior is required
   in CGx2 and Rust. Check the large task's real 20-module call graph.
2. Re-run both acceptance suites on a pinned offline toolchain and verify the
   starter fails behavior while the reference passes. Recompute every hash.
3. Record two distinct reviewers other than the task author. Each reviewer
   must approve task quality, language equivalence, hidden tests, and Rust
   baseline choice in a separate hash-pinned review artifact. The v3 importer
   rejects missing, mismatched, or post-freeze records. Recorded identities
   still need human verification; a JSON file cannot prove independence.
4. Freeze prompts, tool/docs limits, dependencies, compiler, hardware, task
   set, and randomized paired run grid before model execution. Keep private
   acceptance material outside all model-visible workspaces and traces.

A later holdout needs 60–100 independently reviewed tasks across both tiers
and all six categories, two model families, repeated paired seeds, and genuine
external traces plus hidden verification. These two candidates alone are not
an LLM comparison and have no completion, speed, or release verdict.

## Wave 11 task-clustered statistics

The v2 analysis matches `task_id`, `model_family`, `model_version`, and `seed`, and compares CGx2 with each task's preregistered `baseline_language`. A holdout must have the same model/version/seed grid on every task, one frozen version and tokenizer per family, two families, and repeated seeds. The report includes raw registered/matched task counts, counts by tier, excluded task counts by condition, baseline-language breakdown (including Python separately), run and paired-success counts, and family/tier cells. These are mechanical checks: review quality and model trace authenticity still need independent evidence.

For each task and model family, the completion value is the mean over seeds of `1(C pass) − 1(B pass)`. The reported completion estimate is the mean of these task/family values. For each paired seed where **both** pass, compute `C/B` for total tokens (`prompt_tokens + completion_tokens + tool_tokens`), wall seconds, and output tokens (`completion_tokens`) separately. Within a task/family, take the median paired-success ratio; the reported ratio is the median of those task/family medians. A CGx2 pass with baseline failure remains in completion and all-task cost, never in paired-success speed. For each condition and the task-selected baselines, all-task cost-to-success is the sum of actual cost for successful runs plus **full allowed budget for each failure**, divided by verified successes; `null` means no successes. The trace's observed failure time/tokens do not lower this charge.

The interval resamples whole tasks **within each repository tier**, carrying every family and seed for a sampled task together. The random seed is **0**, the count is **2,000** draws, and bounds are the 2.5th/97.5th percentiles. At least **30 distinct eligible tasks** and at least **two eligible tasks in every model-family/tier cell** are required per metric; otherwise the interval is `insufficient_power` and its gate is `uncertain`. `seed_variation` reports the median within-task/family seed range separately; it is not another confidence interval. Synthetic tests show why resampling 600 seed rows as independent observations can falsely exclude the noninferiority margin when there are only 30 tasks.

The completion noninferiority gate is `met` only when its lower 95% bound is strictly greater than **−0.05**. Each paired-success token/time gate is `met` only when the point ratio is at most **0.80** and its upper bound is strictly below **1.00**. A confidence bound proving a missed margin is `failed`; overlapping bounds are `uncertain`. The report gives pooled and model-family gate decisions; `gates_met` requires all of them to be met. Output tokens and all-task cost are reported separately and cannot substitute for total model tokens or the completion gate. Public calibration has no release verdict, and the current public holdout template has no genuine traces.

**Runtime scope:** The existing six-case native pilot measures separate build, first subprocess launch, and repeated startup-inclusive launches. It is scalar and startup dominated. No application-shaped, behaviorally equivalent CGx2/Rust owned-memory workload is available in this frozen Wave 8 toolchain, so the native owned/managed, PyTorch, dispatch, and end-to-end application targets remain unmeasured. Do not interpret the v2 LLM report as runtime evidence.

## Wave 17 public syntax pilot readiness

The six-case paired syntax packet, exact-token trace contract, and one-command readiness report are documented in [fixtures/syntax-pilot/v1/README.md](fixtures/syntax-pilot/v1/README.md). Public candidate files are incomplete `TODO()` starters; complete compiler-checked examples and acceptance outputs remain under `private/` and must stay outside model workspaces. Independent human review and model trials are still unrun. The experimental brace form is normalized to canonical `.cg` by the same operator-provided compiler wrapper for both variants.

## Wave 21 real matrix runtime baselines

The offline Torch 2.11.0 CPU float64 source, direct facade, hand-written
LibTorch C++, and optional matching Python procedure is in
[MATRIX-RUNTIME.md](MATRIX-RUNTIME.md). It emits raw process and warm-call
samples with package/tool hashes and marks small-sample or unavailable claims.

## Wave 23 release f32 baselines

[Preregistered settings](WAVE23-F32-PREREGISTRATION.md) and
[results with raw evidence](WAVE23-F32-RESULTS.md) cover explicit real f32/v3
source and C-facade/C++ cases. New builds default to release; external binary
profiles remain unverified. Fixed numeric tolerance, source caps, sample
gates, base CLI Python linkage and unavailable routes are retained.

Wave 24 frozen scalar-native and isolated Python-child baselines:
[NATIVE-WORKER-BASELINES.md](NATIVE-WORKER-BASELINES.md),
[preregistration](WAVE24-NATIVE-WORKER-PREREGISTRATION.md), and
[results with limitations](WAVE24-NATIVE-WORKER-RESULTS.md).
