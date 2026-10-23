# Wave 23: native execution boundary and stronger runtime evidence

All four coding chats start from the same reviewed plan commit. Read `AGENTS.md`, `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`, this ledger, and your assigned `plan.md` before coding. Use MegaMemory overview → query → record. Work only in the attached worktree and owned paths. Commit one bounded deliverable, report verification and limitations, and stop for user review. Do not merge, start another wave, edit another lane, or change the root Cargo manifest, root lockfile, or shared design/progress/roadmap documents.

| Lane | Exclusive write surface | Bounded result |
| --- | --- | --- |
| [Automatic collection pressure](automatic-collection-model/plan.md) | `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**` | Structurally varied model with actual automatic collection and failure recovery |
| [Python-free native CLI](native-python-boundary/plan.md) | `crates/cgx2-cli/**`, `crates/cgx2-foreign/**`, `crates/cgx2-python-bridge/**` | Native process without eager CPython linkage; existing explicit scalar Python calls run in an isolated worker |
| [Record-typed notebook functions](notebook-record-functions/plan.md) | The five `crates/cgx2-notebook-*` crates | Local pure functions accepting and returning immutable scalar records |
| [Release float32 baselines](release-f32-baselines/plan.md) | `bench/**` | Reproducible optimized source/C++ and warm direct-facade/C++ float32 observations |

## Frozen inputs and independence

Wave 22 syntax, checker, `Value::Record`, local scalar record function semantics, and the f32/v3 facade are frozen inputs. No lane adds syntax or changes public compiler types. The memory lane does not edit `cgx2c` tests; source parity additions live under memory-conformance. The Python lane preserves public CLI check/run APIs and source errors. The notebook lane consumes those APIs and keeps foreign/effectful cells rejected before execution. Existing in-process bridge APIs, numeric arrays, and handle semantics remain available to opted-in host-library users; the CLI worker implements only the existing copied scalar source profile.

The benchmark lane measures this common commit and cannot depend on concurrent Python isolation. It records the base CLI's observed Python linkage. The source cap stays 32 rows/columns and 1,024 elements; larger cases are warm direct calls. Root `Cargo.lock` belongs to the coordinator. Report any required change outside ownership as a concrete blocker.

A conditional-control-flow wave remains later because a new statement AST needs coordinated compiler, verifier, and notebook changes. The 92% tracked implementation estimate gives no prospective gate credit, release label, safety proof, or speed/LLM claim.

## Coordinator review gate

After the user reports completion, inspect exact commits and ownership, integrate on a separate review branch, and run combined offline Rust/UI/Python benchmark tests, format, Clippy, readiness, memory checks, record-function examples, and real pinned f32 source execution where available. Inspect the exact production binary's ELF dependencies and mapped libraries, Python child errors/timeouts, and benchmark build provenance/sample counts. Keep `main` unchanged pending user review.
