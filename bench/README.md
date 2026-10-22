# CGx2 baseline and pilot benchmark harness

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
