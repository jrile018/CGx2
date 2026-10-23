# CGx2 notebook command

## Retained runtime contracts

```sh
cgx2-notebook run crates/cgx2-notebook-cli/examples/runtime-contracts.cgnb --out /tmp/contracts.cgnb
cgx2-notebook sweep crates/cgx2-notebook-cli/examples/runtime-contracts.cgnb parameter \
  --value -1 --value 0 --value 5 --out-dir /tmp/contracts-sweep
cgx2-notebook run crates/cgx2-notebook-cli/examples/contract-imports/contracts.cgnb \
  --project crates/cgx2-notebook-cli/examples/contract-imports --out /tmp/imported-contracts.cgnb
```

Retained scalar functions, including permitted local accumulators, support checked
`requires`/`ensures` and postcondition `result`. False pre/postconditions and checked
arithmetic failures save failed trial provenance without publishing partial values;
the sweep continues with clean trials. Contract-only helper changes invalidate
definitions and callers. Pure imported predicates carry their transitive package
provenance. Imported functions themselves keep the existing no-contract/no-effect
profile. The [kernel contract profile](../cgx2-notebook-kernel/README.md) describes
the unchanged step/depth/byte/deadline limits and unsupported types/effects.

## Local accumulator and range functions

```sh
cgx2-notebook run crates/cgx2-notebook-cli/examples/local-loops/local-loops.cgnb \
  --project crates/cgx2-notebook-cli/examples/local-loops --out /tmp/local-sums.cgnb
cgx2-notebook sweep crates/cgx2-notebook-cli/examples/local-loops/local-loops.cgnb parameter \
  --value 2 --value 5 --value -1 --project crates/cgx2-notebook-cli/examples/local-loops \
  --out-dir /tmp/local-sums-sweep
```

[The example](examples/local-loops/local-loops.cgnb) uses local mutable i64
accumulators, signed literal-bound ranges, nested loops/conditionals and a pure
imported helper. Each invocation/restart/sweep initializes its locals anew.
The compiler retains `Mutate`; the notebook separately verifies local-only
computation for replay. Loop bodies may call only pure helpers. Captured,
parameter/index/record mutation and external effects remain unsupported;
imported module bodies keep the Wave 27 profile. The existing 100,000 static
steps include nested/call/replay work before worker execution. Save/reopen and
export keep observations and need the matching external `--project`.

## Local record functions

```sh
cgx2-notebook run crates/cgx2-notebook-cli/examples/record-functions.cgnb \
  --out /tmp/record-functions-result.cgnb
cgx2-notebook sweep crates/cgx2-notebook-cli/examples/record-functions.cgnb parameter \
  --value 2 --value -1 --out-dir /tmp/record-functions-sweep
```

[record-functions.cgnb](examples/record-functions.cgnb) passes immutable local
records through identity, construction, and field projection functions across
cells. Record annotations participate in invalidation even for unused
parameters. The ordinary worker, replay-byte, step, and depth bounds apply.
Foreign-layout records, generic/nested records, record mutation, contracts,
and externally effectful functions remain unsupported. Reopening restores cached outputs;
Restart and Run All recomputes them from source.

## Checked local pure imports

Run [the two-module example](examples/pure-imports/pure-imports.cgnb) with:

```sh
cgx2-notebook run crates/cgx2-notebook-cli/examples/pure-imports/pure-imports.cgnb \
  --project crates/cgx2-notebook-cli/examples/pure-imports \
  --out /tmp/pure-imports-result.cgnb
```

`--project ROOT` is explicit on `run` and `restart`. The notebook keeps native
`.cgnb` syntax: an import-only cell binds a local module short name, and later
cells call `module.function(...)`. Missing or changed package inputs are rejected
or shown stale on reopen; saved provenance includes the manifest and source
SHA-256 values. The project root is never searched through ambient Python,
network packages, or unlisted files. Pure scalar imports do not enable external
side effects, Python libraries, tensor handles, or general Jupyter kernels.

## Bounded pure scalar sweeps

```sh
cgx2-notebook sweep crates/cgx2-notebook-cli/examples/sweep-factor.cgnb factor \
  --value 2 --value 0 --value 5 --out-dir /tmp/factor-sweep
cgx2-notebook inspect /tmp/factor-sweep/trial-01.cgnb
cgx2-notebook inspect /tmp/factor-sweep/trial-02.cgnb
```

[sweep-portable-images.cgnb](examples/sweep-portable-images.cgnb) is a self-contained example with two one-pixel PNGs referenced from Markdown using `![alt](asset:sha256:<digest>)`. Sweep trials embed those verified images in each `trial-NN.cgnb`, so a successful or failed trial reopens after the original input and sidecar are removed. Clean execution discards stale code outputs while preserving Markdown image references. `run`, `restart`, and `delete` also carry still-referenced verified assets into their output files. Arbitrary Markdown file paths and URLs remain text and are never copied or fetched.

`sweep` requires a code cell containing exactly one immutable `i64`, `Bool`, or
`String` literal binding. Each `--value` is a CGx2 literal of that same checked
type. The command accepts 1–32 distinct values, each at most 4,096 UTF-8 bytes.
It validates every cell against the pure scalar profile, then starts a fresh
kernel for each value. `summary.json` records each trial's value, file, error,
and code cell statuses. Files are `trial-01.cgnb`, etc., in input order. A
runtime failure, including the per-cell worker timeout, saves that trial's
failed provenance and leaves later cells unrun; the remaining trials continue.
The default timeout is 2,000 ms, adjustable with `--timeout-ms N` from 1 to
60,000. Validation or serialization failure removes the staging directory and
publishes no output directory. The destination must not already exist; paths
with `..` or symlink components are rejected. Existing notebook size limits
still apply. Sweeps explore pure scalar variations; they are not a Jupyter
kernel or a general side-effect replay system.

The [function lifecycle example](examples/function-lifecycle.cgnb) defines `inc`
in one cell, calls it through `twice` in another, and keeps an independent
result. Run and reopen it with:

```sh
cgx2-notebook run crates/cgx2-notebook-cli/examples/function-lifecycle.cgnb --out /tmp/functions-run.cgnb
cgx2-notebook inspect /tmp/functions-run.cgnb
cgx2-notebook delete /tmp/functions-run.cgnb base --out /tmp/functions-deleted.cgnb
cgx2-notebook inspect /tmp/functions-deleted.cgnb
```

After deletion, `derived` and `value` are stale saved observations; the
independent result stays cached across reopen. `delete` writes a new notebook
and does not execute cells. A missing call reports which pure function must be
defined or rerun. Current persistent notebook support is limited to pure
scalar functions (`i64`, `Bool`, `String`), immutable scalar bindings, and
bounded calls. Effectful persistent state, arbitrary imported libraries, and
Python/Jupyter execution are not supported.

`cgx2-notebook` reads native `.cgnb` files and executes only pure scalar CGx2 cells. A final expression produces captured `text/plain` display; immutable `let` bindings may feed later cells. HTML and scripts in saved text remain inert. The notebook format limits one captured text output to 65,536 UTF-8 bytes, a document to 16 MiB, and the kernel to 128 code cells, 64 KiB of cumulative source, and 1 MiB of scalar evaluation state.

```sh
cgx2-notebook run experiment.cgnb --out executed.cgnb
cgx2-notebook run experiment.cgnb --out revised.cgnb --edit-cell seed --source 'let x = 5'
cgx2-notebook inspect revised.cgnb
cgx2-notebook show revised.cgnb result
cgx2-notebook export revised.cgnb --out portable.cgnb
cgx2-notebook import portable.cgnb --out reopened.cgnb
```

`run` starts a clean evaluator and executes code cells in file order. With both `--edit-cell ID` and `--source CG_CODE`, it then edits that cell and reruns only the edited cell and its transitive readers in the same process. Independent cells keep their output and execution revision from the clean run. This form provides a CLI selective-rerun workflow without trusting saved bindings as live state. A separate command invocation cannot resume an earlier process's bindings; `inspect` and `show` read saved output and provenance without executing cells. `show` escapes control characters in display text. Reopening preserved output is cached or stale, never current until execution.

`run` and `show` work on ordinary and portable `.cgnb` input. `export` embeds verified referenced PNG, JPEG, GIF, or WebP assets; `import` writes ordinary notebook text and verified sidecar assets. Neither command changes the input unless that path is explicitly used with `--out` and `--force`. Existing destinations require `--force`; failed validation does not replace them. Notebook cells cannot access external files, networks, host commands, Python, or arbitrary CGx2 effects. This command does not claim Jupyter plugin parity.

`cgx2-notebook restart INPUT.cgnb --out OUTPUT.cgnb` persists stale saved observations without executing. `run` accepts `--timeout-ms N` (1–60,000; default 2,000) for each cell's scalar worker phase. On `CGNK010`, `run` exits nonzero and writes the partial notebook to `--out`: completed earlier cells remain current in that run, the interrupted cell is failed, and dependent saved outputs are stale. The input is untouched unless selected as `--out` with `--force`. The child process is killed at the deadline; static parse/check and output serialization remain in process and are not wall-clock preemptible. A later invocation starts from a clean session and does not restore in-memory bindings from saved output.

## Bounded Boolean conditional profile (Wave 24)

The [conditional notebook](examples/conditionals.cgnb) demonstrates pure
Boolean function bodies, local record returns, independent branch scopes and
inactive-branch dependency invalidation. Top-level conditional cells remain
unsupported; notebook persistence and runner interfaces are unchanged.


## Public record package notebooks (Wave 26)

The [public-record example](examples/public-records/public-records.cgnb) includes
its two-module fixture. Both modules export `Point` and the notebook also defines
a distinct local `Point`.

```sh
cgx2-notebook run crates/cgx2-notebook-cli/examples/public-records/public-records.cgnb --project crates/cgx2-notebook-cli/examples/public-records --out /tmp/records.cgnb
cgx2-notebook inspect /tmp/records.cgnb --project crates/cgx2-notebook-cli/examples/public-records
cgx2-notebook sweep crates/cgx2-notebook-cli/examples/public-records/public-records.cgnb parameter --value 1 --value 2 --project crates/cgx2-notebook-cli/examples/public-records --out-dir /tmp/records-sweep
```

`run`, `restart`, `inspect`, `delete` and `sweep` accept `--project ROOT`.
A sweep checks the package before publishing trials and uses fresh kernels for
each trial. Source imports must be explicit/direct, and imported functions must
be pure in the bounded scalar/public-record profile; private, nested, generic,
managed, foreign, effectful and contract-bearing APIs remain rejected. Constructors
and annotations may use `alpha.Point` or `demo.alpha.Point`; functions use
`alpha.function`. Nominal identities remain module-qualified across values,
functions, workers, selective rerun, save/reopen and Restart and Run All.

Exported notebooks and sweep trials carry notebook text and verified assets;
they do not embed package source or its manifest. They still require the local
project to execute. `show` can display a saved observation without that project;
this is not execution or restoration of live state. Package captures bind the
canonical root as well as source/manifest hashes. Changed inputs discard affected
retained definitions, preserve independent outputs, and require reopening the
checked project. [Kernel details and limits](../cgx2-notebook-kernel/README.md).
