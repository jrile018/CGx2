# S2a API/text repair evidence

Bounded API/text delivery on `riley/s2a-api-text-repair`, based on
`8902cbd92533b81e801bf2c5bea725f5508fe3e2`. All changes are inside
`crates/cgx2-api-index/`. No public model shape, JSON renderer/schema/version,
semantic admission rule, dependency, CLI source, root Cargo file, or shared
ledger changed.

## Result

- `src/render.rs`: module headers display the stored `semantics_status` for all
  text indexes. Function contracts use their stored status: checked contracts
  remain `runtime-checked`; syntax-only contracts remain `unchecked`.
- Record/variant declarations display their canonical identity and generic
  parameters, including named constraints in declaration order. Recursive
  `ApiType.arguments` render on payloads, fields, function parameters and results.
  Existing string fields remain fallbacks when structured metadata is absent.
- Loop annotations display kind, expression, stored status, and zero-based
  half-open UTF-8 span. Foreign text displays `unsafe_call` independently of
  optional checked `binding_id`; syntax-only native declarations retain this flag.
- Existing public filtering and ordering are preserved. Private functions,
  records and variants are absent from both golden output formats.
- `README.md` documents the human format and its semantic limits. Legacy text
  headers now also contain `[checked]`; legacy JSON output remains unchanged.

## Verification

Used Rust 1.98.1 (`48a229cea 2026-09-01`), two Cargo jobs, offline locked builds,
and worktree-private target/temp directories:

```sh
export RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-s2a-REPAIR"
export TMPDIR="$PWD/target/tmp-s2a-REPAIR"
export LIBRARY_PATH=/tmp/cgx2-wave22-lib
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo test --offline --locked -p cgx2-api-index --all-features
cargo clippy --offline --locked -p cgx2-api-index --all-targets --all-features -- -D warnings
cargo fmt --package cgx2-api-index -- --check
git diff --check
cargo build --offline --locked -p cgx2-cli
python3 crates/cgx2-api-index/tests/verify_text_cli.py \
  --binary "$CARGO_TARGET_DIR/debug/cgx2" \
  --output-dir "$CARGO_TARGET_DIR/cli-conformance-2"
```

Use a fresh CLI output directory for each run. The recorded run used
`cli-conformance-1`. Results:

- **25 tests passed, 0 failed, 0 ignored** across the owned crate's integration
  suites; five new renderer tests. Clippy, owned formatting, and diff checks pass.
- Before the renderer fix, the three new source/snapshot regressions all failed.
  After the fix, exact human snapshots pass for checked and syntax-only scalar
  contracts, checked nominal records/variants, and syntax-only generic
  records/variants/functions with `Eq + Show`, nested
  `Option<Result<T, Bool>>`, qualified aggregate payloads, array parameters,
  loop annotations, and private declarations. Rebuilding the indexes repeats
  identical output. Separate tests preserve arbitrary stored loop status,
  unannotated result text, and native unsafe metadata.
- Four exact JSON snapshots were captured using the unmodified baseline CLI
  before editing the renderer. Owned tests and rebuilt CLI compare bytes to
  these snapshots, covering schemas 2 and 6 and checked/syntax-only modes.
  Existing legacy/foreign JSON regressions also pass. `render_json` is untouched.
- **33 CLI/source/target checks passed**, with full commands, exits, stdout,
  stderr, environment, and binary SHA-256 in
  [`evidence/s2a-text-cli.json`](evidence/s2a-text-cli.json). Each of four API
  modes was repeated in both text and JSON. Checked scalar source returns `7`
  through the interpreter, native build, compiled emitted Rust, and emitted
  Python. Unsupported generic source fails checked API text/JSON, check, run,
  native build, Rust emission, and Python emission with semantic diagnostics;
  rejected target paths remain absent.

Detailed local build/test logs are under `target/release-s2a-REPAIR/` (ignored).
The committed source fixtures, golden outputs, CLI runner and evidence permit
independent reproduction without modifying another lane.

## Limits

This is local fixture conformance for the assigned human-output repair. Generic
and annotated-loop syntax indexing remains unchecked; this work adds no generic
execution, loop enforcement, or proof support. The synthetic stored-status test
only checks renderer fidelity. Native/Rust/Python execution checks use the small
checked scalar fixture, not the unsupported generic fixture. No external solver,
performance, leak-enabled, human/model study, release acceptance, other-lane
repair acceptance, or S2b/S3 readiness is claimed.
