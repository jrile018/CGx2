# Trusted source layout reporting

This is the bounded S2b report-layout repair on base
`9fcefbbb201f1f73f387f43672bc9b4f9d23eeb7`. It fixes reporting for the existing
explicit `memory.*` source profile. The full ordinary-memory checkpoint, S2b,
S3/S4 and R01–R15 release gates remain incomplete.

## Implementation and scope

`crates/cgx2-cli/src/memory.rs` records construction provenance in the
invocation's `FailureState.layouts: HashMap<Handle, SourceLayout>`.
`crates/cgx2-emit-rust/src/memory_runtime.rs.txt` records the same metadata in
`CGX2_LAYOUTS`, a thread-local map for the generated executable's single session.
Metadata is registered only while reporting is enabled, so ordinary execution
adds no provenance-map allocations. `SourceLayout::Bytes` comes from byte allocation and owned promotion;
`SourceLayout::Object { slots }` comes from raw object allocation;
`SourceLayout::Graph` comes only from explicit graph allocation.

Snapshots enumerate a raw object's exact declared slots, or validate and visit
an explicit graph's logical slots. They never select a layout by inspecting
mutable payload magic. Runtime handle/session validation and graph-chain layout
validation still run. A live root with missing provenance or invalid graph
layout yields unknown (`null`) source root/edge counts. Duplicate handles are
counted once. Metadata survives binding drops, branches, nested scopes and
function frames; session/generation identity prevents stale slot provenance.
The maps hold only reporting metadata, never replacement CGx2 payloads or edges;
all CGx2 storage, accounting, collection and roots still use `Session`. Historical
metadata lasts for the invocation (or generated process) and is bounded by the
existing execution/allocation limits; it is not a general persistent evaluator.

Generated helper names/arity and `Binding::Managed(Handle)` are unchanged.
Optional `reference`/`graph_reference` results preserve the runtime child handle
and do not infer layout from the accessor used. The current source profile admits
`use_reference` for these non-rooting optional values, but has no optional-to-
managed conversion or memory-reference function return. Direct interpreter and
actual-template helper tests explicitly simulate managed aliases returned from
both accessors, duplicate aliases, missing child metadata and corrupted graph
headers. These helper tests supplement the real source cases; they do not claim
new source capabilities. The helper test module permits the existing template's
arity/tuple complexity because the generator and its interfaces are frozen.

## Exact source acceptance

The nine committed `.cg` fixtures run through the interpreter, native debug,
native release and standalone emitted Rust linked to a freshly compiled copy of
the repository runtime. Every failure is exact exit **1**, never a signal or an
arbitrary nonzero exit. Diagnostic code/message and per-route byte spans are
frozen in `expected.json`.

| Source case | Failure roots | Failure source edges |
| --- | ---: | ---: |
| supplied `forged-layout` | 2 | 1 |
| `raw-invalid-header` (raw cycle, graph-like invalid header) | 2 | 2 |
| `raw-valid-header` (valid single-chunk header and optional aliases) | 2 | 1 |
| `mixed-cycles` (raw/graph children, logical slot 65535 and aliases) | 2 | 4 |
| `dropped-graph-child` (raw reference alias) | 1 | 1 |
| `dropped-raw-child` (graph reference alias) | 1 | 1 |
| `transfers` (owned move/promotion/copy and borrow) | 2 | 1 |
| `frames-branches` (failing callee arm and caller root) | 3 | 2 |
| `mixed-success` | successful Unit; zero committed bytes after explicit collection/close | — |

Each failure has zero source roots/edges and zero runtime roots/registrations
after unwind, no cleanup error, and preserved rollback evidence. Failure reports
can retain committed bytes in the still-open manual region; removing roots is
not an implicit collection. All deterministic runtime counters and failure
lifecycle fields agree across routes. Reports do not alter values, stderr or
exit status within a route. The existing native span includes the trailing
newline while the interpreter expression span excludes it; both exact spans are
preserved and asserted separately. Pause timing, session identity, source path
and the native-only schema marker are the only excluded parity fields.

## Reproduce and evidence

From this attached worktree, run:

```sh
python3 crates/cgx2-cli/tests/memory_report_layout/verify.py
```

The driver uses `RUSTUP_TOOLCHAIN=1.98.1`, `CARGO_BUILD_JOBS=2`, private
`CARGO_TARGET_DIR=$PWD/target/release-report-layout` and
`TMPDIR=$PWD/target/tmp-report-layout`; it uses `/tmp/cgx2-wave22-lib` when present.
No package installation or network dependency is required. Each product command
has a 120-second limit; each verification stage has a 600-second limit. Product
programs run from a private temporary directory outside repository-relative
assumptions. The driver runs:

```sh
cargo test -p cgx2-cli -p cgx2-emit-rust --all-features --offline
cargo clippy -p cgx2-cli -p cgx2-emit-rust --all-targets --all-features --offline -- -D warnings
cargo fmt --all --check
git diff --check
cargo build -p cgx2-cli --all-features --offline
python3 crates/cgx2-cli/tests/memory_report_layout/run.py --binary "$CARGO_TARGET_DIR/debug/cgx2" --output crates/cgx2-cli/tests/memory_report_layout/evidence/product.json
python3 crates/cgx2-syntax/s2b-memory-evidence/run.py --binary-dir "$CARGO_TARGET_DIR/debug" --output crates/cgx2-cli/tests/memory_report_layout/evidence/existing-graph-product.json
```

`evidence/verification.json` and its referenced logs record exact command exits:
150 package tests passed, zero failed/ignored; Clippy, fmt and diff passed.
`evidence/product.json` records 265 passing checks on the nine source cases,
exact subprocess outputs, deterministic reports and source/runtime hashes.
`evidence/existing-graph-product.json` records all 99 existing checks passing
(five existing sources, six existing source rejections). That reviewed runner is
unchanged; its nonzero-exit acceptance is weaker than this repair's strict exit-1
checks. `evidence/baseline-red.json` records the new source assertion run against
the original CLI file: exit 101, `forged-layout` expected one edge, actual zero.
The original file was restored to the revised contents in a `finally` block
before final verification.

No manifest/dependency/root-lock changes or reconciliation are needed. Only the
two assigned implementation files and this owned test/evidence tree change.
Other repair lanes, the Rust generator and coordinator plans are untouched.
Ordinary owner/loan CFG lowering, default managed application aggregates,
recoverable source transfer Results, broader cleanup/package/no_gc closure and
external memory/leak acceptance are outside this reporting repair and remain
required. This evidence establishes neither full memory safety nor release
readiness.
