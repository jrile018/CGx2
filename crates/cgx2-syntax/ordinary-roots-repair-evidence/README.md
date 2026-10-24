# Runtime roots repair evidence

This directory belongs only to the bounded OM05/OM06/OM07 repair based on
`7bb7a480119826693db67ed503caaf7f23f6abb2`. Ordinary-memory and release acceptance
remain open. The other lanes' source/API failures are retained separately from
the checks of this repair.

`checks/*.json` are immutable command attempts with argv, cwd, environment,
limits, exits/signals, and bounded output bytes/hashes. Failed compilation,
formatting, baseline regressions, and the initial regression-supervisor setup
failure remain present. Later attempts use distinct labels. `evidence/` contains
losslessly compressed raw reports/source snapshots and a hash manifest;
`evidence/summary.json` records final results and remaining requirements.

Use Rust 1.98.1 and the existing offline dependencies. From the attached worktree:

```sh
python3 crates/cgx2-syntax/ordinary-roots-repair-evidence/check.py fresh-tests \
  cargo test -p cgx2-runtime -p cgx2-sema -p cgx2-memory-conformance \
  --all-features --offline --locked
python3 crates/cgx2-syntax/ordinary-roots-repair-evidence/check.py fresh-clippy \
  cargo clippy -p cgx2-runtime -p cgx2-sema -p cgx2-memory-conformance \
  --all-targets --all-features --offline --locked -- -D warnings
```

`check.py` establishes private `target/release-ordinary-runtime-roots` and
`target/tmp-ordinary-runtime-roots`, `CARGO_BUILD_JOBS=2`, and bounded supervision.
Finish all Cargo writes before using `freeze.py`. The read-only common freeze
helper snapshots seven binaries and four canonical top-level libraries; the
owned extension also snapshots the canonical `libcgx2_runtime.rlib`, dependencies,
compiler/runtime source hashes, and compiler/toolchain identities. No Cargo
commands are permitted during product/parity/standalone verification.

`parity.py` takes explicit `--binary-dir`, new `--output-dir`, and new
`--outside-dir` paths. It executes the shipped assignment/copy probes and nested
normal/return/error fixtures through the interpreter, native debug/release,
and independent emitted Rust. Its standalone cwd must be outside the repository.
The direct runtime probe links the frozen canonical runtime library. Memory
comparison excludes pause distributions, following the unchanged product runner.

The unchanged ordinary-memory `run.py --regressions` remains the product gate.
The initial attempt's inherited 900-second hard CPU ceiling prevented all
1,800-second regression parents from spawning. The retry uses a 2,400-second
parent ceiling; individual regression and product child limits are unchanged.
This is a retained harness failure, not a product rejection or a passing negative.

`standalone_private.py PRIVATE_OUTSIDE_BASE --product-dir ... --output-dir ...`
executes the unchanged standalone helper, redirects only its literal `/tmp`
scratch placement to the explicitly provided private base, and independently
journals every attempted child. This avoids the prior coordinator quota failure;
the historical traceback/audit is also retained under `evidence/historical-coordinator`.
The helper's assertions and generated programs are unchanged.

`export.py --scratch-dir ... --outside-root ...` preserves successful and failed
raw records, standalone source, historical quota evidence, final source and
artifact hashes, and verifies that nothing changed after freezing. It exports
no executables, libraries, or performance claims. All commands remain bounded
per process; these supervisors do not claim aggregate cgroup isolation.
