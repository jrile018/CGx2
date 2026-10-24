# Preserve source object layout in memory reports

Read AGENTS.md, docs/language-design.md, docs/validation-plan.md, plans/roadmap.md, plans/wave28/{README,REVIEW}.md, plans/release-completion/{README,contracts,worker-rules}.md, s2b-memory/plan.md and s2b-memory-review/README.md. MegaMemory overview/query/record and Serena initial instructions are required. Work only in this chat's private worktree on a riley/ branch. No delegation, new chats, main merge, publication, package installation, external API spend or security changes. Preserve root Cargo files and coordinator/shared docs/plans; new already-local dependencies may be added only in owned manifests, exclude generated lock edits and report reconciliation.
Use RUSTUP_TOOLCHAIN=1.98.1, CARGO_BUILD_JOBS=2, private CARGO_TARGET_DIR=$PWD/target/release-LANE and TMPDIR=$PWD/target/tmp-LANE (create both); LIBRARY_PATH=/tmp/cgx2-wave22-lib when needed. Commit code and owned evidence, report exact checks/limitations/commit and stop for review. Existing R01-R15, full ordinary-memory requirements, remaining S2b/S3/S4 gates stay mandatory.

## Exclusive ownership
- crates/cgx2-cli/src/memory.rs
- crates/cgx2-emit-rust/src/memory_runtime.rs.txt
- crates/cgx2-cli/tests/memory_report_layout.rs (new)
- crates/cgx2-cli/tests/memory_report_layout/** (new fixtures/runner/evidence)
All other paths read-only, especially emit-rust/src/memory.rs, existing graph-storage fixtures, ordinary-owned modules, shared AST/type/runtime APIs. The guard lane edits the emitter generator; preserve its existing helper signatures and generated call envelope. Add helpers/internal binding metadata as needed without requiring generator edits.

## Defect and deliverable
Read ../evidence/forged-layout.cg. Two ordinary raw objects with one actual source edge are initialized with legal bytes that resemble a two-chunk graph. At CGR015 failure, operation_failure currently reports rooted_handles=2 but rooted_edges=0, although head references tail. CLI graph_snapshot and native template infer object/graph from graph_shape(bytes). Payload content is mutable and is not source layout provenance.
Carry trusted object/graph classification from construction through all bindings, aliases, references, transfers, branches and function frames; enumerate raw object slots as raw slots and explicit graph logical slots as logical slots. Keep runtime layout validation, handle/session checks and conservative unknown reporting. Do not infer from user-writable magic bytes. Preserve generated helper call names/arity: classification can live in internal bindings or interpreter session bookkeeping, but must not become an unaccounted replacement for CGx2 storage. Audit reference-return paths when child provenance is known/unknown; never falsely certify a report.

## Required verification
Real .cg source/interpreter and native debug/release/standalone Rust failure reports for supplied raw-byte forgery must show two rooted handles and one source edge at failure, zero roots after cleanup. Test raw objects with graph-like invalid/valid headers, ordinary cycles/aliases, explicit graph cycles through slot65535, raw/graph children and reference alias paths. Values/diagnostics must be unchanged by reporting. Run CLI owned offline all-feature tests, emit-rust template affected tests, all-target Clippy -D warnings, fmt/diff and the existing99 graph product checks. Commit reproducible bounded runner and exact expected diagnostics (including normal exit, not signal). Explicitly label source reporting versus ordinary source memory capability.
