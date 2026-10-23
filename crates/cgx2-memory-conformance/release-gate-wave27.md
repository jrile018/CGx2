# Wave 27 memory range-loop evidence

Base: `e3a01dffee423ab93d9cc8cd6d69c0f910c667f4`. Work performed only in the attached
worktree's owned ownership/sema/CLI/Rust-emitter/compiler/conformance paths. Runtime
collector policy/APIs, root Cargo files and other lanes' source are unchanged.

Use Rust `1.98.1` (`rustc 1.98.1 (48a229cea 2026-09-01)`). From the worktree root:

```sh
export CARGO_TARGET_DIR="$PWD/.wave27/target"
export TMPDIR="$PWD/.wave27/tmp"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo +1.98.1 test --offline -p cgx2-ownership -p cgx2-sema -p cgx2-cli -p cgx2-emit-rust -p cgx2c -p cgx2-runtime -p cgx2-memory-conformance
cargo +1.98.1 test --offline -p cgx2-ownership -p cgx2-sema -p cgx2-cli -p cgx2-emit-rust -p cgx2c -p cgx2-runtime -p cgx2-memory-conformance --all-features
cargo +1.98.1 test --offline -p cgx2-cli --test memory_range_loops
cargo +1.98.1 clippy --offline -p cgx2-ownership -p cgx2-sema -p cgx2-cli -p cgx2-emit-rust -p cgx2c -p cgx2-runtime -p cgx2-memory-conformance --all-targets --all-features -- -D warnings
cargo +1.98.1 fmt --check -p cgx2-ownership -p cgx2-sema -p cgx2-cli -p cgx2-emit-rust -p cgx2c -p cgx2-runtime -p cgx2-memory-conformance
cargo +1.98.1 build --offline -p cgx2c
python3 crates/cgx2c/scripts/verify_range_asan.py
sh crates/cgx2c/scripts/verify_range_consumers.sh
git diff --check
```

`LIBRARY_PATH` is the supplied bridge-linking path, not evidence of live external
runtime validation. All target and temporary output stays private under this
worktree. The consumer script builds existing Python/notebook/verifier crates
without changing their source or Cargo manifests.

The machine-readable companion records final suite counts, limits and exclusions.
The owned tests and fixtures are the reproducible evidence, not saved allocator
addresses or pause durations. Failure-report comparisons normalize those fields.
The compiler parity suite includes success, exact 100,000 entry, shared-call
limit, the independent statement-step limit, cumulative budget, manual region,
negative-size, reserve fault and automatic-collection-before-fault cases. Invalid
empty loops reject before native outputs are published.

The ASan script compiles both the runtime and every generated binary with
`RUSTC_BOOTSTRAP=1`, Rust 1.98.1, `-Zsanitizer=address`,
`--target x86_64-unknown-linux-gnu`, `-Cdebuginfo=1`, and `-Copt-level=0` or `3`.
It executes with `ASAN_OPTIONS=detect_leaks=0`, checks exit status/primary error,
empty success output, report status, cleanup errors and zero final runtime roots.
It tests 12 cases in each profile (24 runs), including first- and second-call
reserve failures while a current iteration has a managed root, and a reserve
failure after automatic collection. ASan success does not establish leak freedom.

Python returns `CGE001`; notebook source facts and execution reject without saving
cell outputs (`CGNK001` or `CGNK011`). The scalar verifier returns JSON
unknown/unsupported (`CGV001`, no solver); its process exit status is not a proof
or rejection criterion.

Limits remain explicit: 100,000 entered memory-loop iterations per invocation,
1,000,000 statement steps, literal/parameter/index endpoints only, no arbitrary
iterators, loop mutation or escaping loop values. Runtime v1–v5 conformance is
unchanged. No host-blocked LeakSanitizer, live solver, real foreign runtime,
delegated-cgroup isolation or paired human/model trial was validated here. The
whole-language memory-safety release gate remains open.
