# CGx2 baseline and pilot benchmark harness

This is an on-demand, standard-library-only harness for calibrating tasks and measurement noise. It does not invoke a model, install dependencies, implement CGx2, or make a release judgment. The public fixtures are synthetic or harness-only and are **not evidence of a CGx2 result**.

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
