# Complete command coverage inventory

This table is generated from [inventory.json](inventory.json). “Companion” means a
separate preview executable; “library-only” means no installed source command.
Empty fixture coverage is explicitly unrun by this lane, even when an owner suite exists.
The [source design](../language-design.md) remains the full intended contract.

| Contract | Current executable/API route | Coverage and limits |
| --- | --- | --- |
| `cgx2 check PATH` | cgx2 check FILE.cg\|PACKAGE (implemented-preview) | `scalar-check`, `package-effects`. Package fixture invokes the pinned cgx2-native companion directly; wrapper delegation requires colocated cgx2-native. |
| `cgx2 build PATH` | cgx2c build FILE.cg\|--package ROOT --out OUTPUT (companion) | `native-build`, `native-package`, `unified-build-unsupported`, `native-memory`. Unified command absent; scalar and bounded memory/native package target profiles differ. |
| `cgx2 run FILE.cg -- ARGS` | cgx2 run FILE.cg -- ARGS (implemented-preview) | `scalar-run`, `script-arguments`, `package-effects`. Package execution needs colocated pinned cgx2-native; foreign routes need separate locks/runtime artifacts. |
| `cgx2 FILE.cg -- ARGS` | cgx2 FILE.cg -- ARGS (implemented-preview) | `script-arguments`. Only the admitted interpreter/native package profiles execute. |
| `cgx2 test PATH` | No installed test command; Rust crate test suites are development checks (future) | `unified-test-unsupported`. No invented test discovery/property syntax; S2b owns integration. |
| `cgx2 verify PATH` | cgx2-verify FILE.cg --domain x:MIN:MAX --format json (companion) | `finite-audit-not-proof`, `unified-verify-unsupported`, `live-solver`. Exit zero can still mean unknown; finite verdict is not live SMT proof. |
| `cgx2 emit PATH --target rust` | cgx2c emit-rust FILE.cg\|--package ROOT --out FILE.rs (companion) | `rust-emission`, `unified-emit-unsupported`. Rust target has its own checked profile; memory emission requires embedded-runtime build/link support. |
| `cgx2 emit PATH --target python` | cgx2_emit_python::{emit_python,emit_package_python,write_package_python} Rust library APIs (library-only) | `python-source-cli`, `unified-emit-unsupported`. No installed emitter CLI; regions/no_gc/owned borrows/foreign effects explicitly unsupported. |
| `cgx2 fmt PATH` | cgx2 fmt FILE.cg (implemented-preview) | `canonical-format`. Writes canonical source to stdout; package directory formatting not supported. |
| `cgx2 api --json PATH` | cgx2 api FILE.cg --json (implemented-preview) | `public-api`. Argument order differs from design table; package public index is cgx2-package, not this single-file CLI. |
| `cgx2 notebook open FILE.cgnb` | cgx2-notebook-ui open FILE.cgnb [--project ROOT] [--port PORT] (companion) | **unrun here**. Interactive loopback UI unrun by checker; use supplied worker/loopback-enabled host and owned output paths. |
| `cgx2 notebook run FILE.cgnb` | cgx2-notebook run FILE.cgnb --out OUTPUT.cgnb (companion) | `notebook-local`, `notebook-effects-rejected`, `unified-notebook-unsupported`. Pure/local replay-safe subset; external effects, live handles and contract parity await S3-N. |
| `cgx2 bind check PACKAGE` | cgx2-bind check PACKAGE --format json (companion) | **unrun here**. Full lock/environment/runtime external prerequisites unrun here; import never authorizes installation. |
| `cgx2 memory report FILE.cg` | cgx2 memory report FILE.cg --format=json (implemented-preview) | `manual-memory`, `no-gc-memory`. Fixtures execute memory operations, not report-counter assertions; counters/timings cover CGx2 allocations only. |
| `cgx2 memory failure-report FILE.cg` | cgx2 memory failure-report FILE.cg (implemented-preview) | **unrun here**. Unrun by docs checker; partial operation/unwind snapshots do not prove complete safety or leak freedom. |
| `Security analysis` | cgx2-security audit FILE.cg --policy POLICY.json --format json; cgx2-effect-index FILE.cg --format json (companion) | **unrun here**. Unrun here; advisory bounded scalar analysis is not a general vulnerability proof or resource capability grant. |
| `Performance analysis and execution plan` | cgx2-execution Rust library; owned-input map/zip source integration reserved (library-only) | `security-performance`. No installed general performance command or arbitrary source parallel/GPU implementation; no timing during busy host work. |

## Additional preview surfaces

`cgx2-notebook` also provides inspect/show/restart/delete/export/import/sweep; the
notebook owner supplies their lifecycle and portable asset suites. `cgx2-package`
renders the checked local package index. `cgx2-effect-index` provides checked or
explicit syntax-only scalar call/effect summaries. Native binaries accept opt-in
`CGX2_MEMORY_REPORT` paths; ordinary stdout stays separate from memory telemetry.
These surfaces do not silently expand the first-release source profile.

## Target and execution profile

| Target | Admitted preview | Explicit limits |
| --- | --- | --- |
| Interpreter | Checked scalar/immutable records, local packages, bounded loops/conditionals, explicit memory scripts, reviewed bounded foreign adapters | Static acceptance does not imply every target can run the construct; no general generic/variant/aggregate library profile |
| Native/Rust | Scalar/record packages, checked arithmetic/contracts/loops, standalone bounded memory scripts/functions via embedded runtime | General foreign/tensor/package-memory lowering is not implied; compiler reports target-specific diagnostics |
| Python source | Rust library `cgx2-emit-python` handles scalar/record/local-package profiles, checked helpers and contracts | No installed emitter CLI here; reject regions, no_gc, owned borrows and foreign effects; no general tensor translation |
| Notebook | Pure scalar/record functions, checked pure local imports, bounded local accumulator loops | Explicit effect rejection; no general retained foreign runtime or package installation |
| CPU planner | Reviewed Rust library map/zip policy | Source integration pending; no arbitrary source parallelism, general GPU backend or guaranteed optimal plan |

Generated target edits do not round-trip into CGx2. Rust/Python behavioral and error
parity must be checked for the admitted profile, and readability/editability needs
the separate blinded S4 study. Python library access and Python source translation
are different routes.
