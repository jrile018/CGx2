# CGx2 Executable Benchmark Pilot Plan

> Implement this plan in your attached worktree, commit only owned paths, and stop for user review. Do not delegate, merge, or launch another chat.

**Goal:** Replace synthetic-only CGx2 examples with a small executable parity corpus and on-demand runtime pilot that can reveal failures and measurement noise. Do not claim an LLM or runtime win from the pilot.

**Architecture:** Extend the standard-library `bench` harness with six paired, frozen scalar tasks that CGx2 and Python can both express today. Each pair has an independent oracle, deterministic inputs, expected output, and recorded compiler/interpreter provenance. Add an on-demand runtime sampling command with warmups, repeated paired runs, alternating condition order, raw sample retention, and a compact report of medians/spread. Keep the existing LLM task runner/report schema separate; actual LLM token/iteration data still requires model traces.

**Tech stack:** Python 3 standard library, existing `cgx2` CLI binary built from Wave 3, no new packages or network access.

**Specification:** `docs/validation-plan.md`, `bench/README.md`, `plans/wave4/README.md`.

## Owned paths and boundaries

- Own only `bench/**` and this plan. Do not edit Rust crates, root manifests/lock, shared docs, or existing sample results. Do not use fabricated CGx2 success records.
- The six tasks must use only the CLI's checked scalar subset: i64 checked arithmetic, functions/calls, booleans, strings, `arg`/`arg_count`, local `let`, and mutation as supported. Avoid loops, arrays, FFI, memory ops, notebooks, or Python-specific libraries. Explicitly record this narrowness.
- Oracle checks compare semantic result and exit/error class where meaningful, not just process exit zero. Use exact output for deterministic cases; include at least one integer boundary/error task and one argument-handling task. Ensure equivalent computational work in both languages as far as the subset permits.
- Runtime samples must not include compilation time unless an explicit separate `compile` metric is recorded. Use warmups and at least 10 timed repetitions per condition, record each subprocess wall time and exit/output hash, alternate which language runs first. State host, interpreter/compiler versions, CPU, and workload size; never publish a ratio without the raw samples and validity checks.
- Source length may report UTF-8 bytes, characters, lines, and lexical token count as *descriptive* values. Do not call lexical tokens model tokens; only the existing trace schema can report real LLM output/total tokens.

## Review focus

1. Every CGx2/Python pair passes the same independent oracle before any timing is accepted; a deliberate wrong output fails the pilot.
2. Syntax/check failures, timeouts, missing executable, and Python errors remain explicit invalid measurements, not zeros or skipped successes.
3. Runs are reproducible from checked-in fixtures and command line, with raw data in a user-selected output directory under `/tmp` by default. Do not overwrite prior results silently.
4. Alternating execution order and warmups are visible in the sample records; runtime claims distinguish cold start from steady process execution where possible.
5. The report calls the sample a pilot; six scalar tasks cannot establish first-release LLM productivity or native runtime performance.

## Tasks

### 1. Paired corpus and oracles

- [ ] Add six `.cg`/`.py` fixture pairs, a versioned manifest, and independent expected outputs/errors. Unit tests validate manifest schema, pair coverage, and oracle behavior.
- [ ] Add a command that builds or locates `cgx2` explicitly, runs every pair once, and emits a machine-readable parity result. Use `shell=False` and bounded timeouts.
- [ ] Run it locally against the integrated Wave 3 CLI and record any unsupported task by revising the task, never weakening the oracle.

### 2. Runtime pilot

- [ ] Add an opt-in timing command with configurable warmups/repetitions, fixed minimum repetitions, alternating order, raw samples, and median/IQR report. Pin/process provenance, hash executable/fixtures, and retain failed samples.
- [ ] Add tests using tiny stub executables to verify order, warmup exclusion, timeouts, output mismatch rejection, and no silent overwrite. Do not assert a speed win in tests.
- [ ] If timing fresh subprocesses mostly measures startup, label it explicitly and provide a design note for an in-process/native future benchmark.

### 3. LLM task bridge and handoff

- [ ] Document how six source-equivalent tasks could be given to the same model with pinned prompts/tools/seeds under the existing runner, and what trace fields are needed. Do not invoke a model or write guessed token counts.
- [ ] Run `python3 -m unittest discover -s bench/tests` and the new parity smoke. Update `bench/README.md` with exact commands, scope, environment, and limitations. Commit only owned paths.

**Acceptance:** The checked-in pairs and pilot tool produce verifiable raw parity and runtime records from real CLI/Python invocations. No benchmark superiority claim is made before a larger independently reviewed holdout.
