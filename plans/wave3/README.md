# Wave 3: checked CLI and independent compiler contracts

All four chats start from the same `wave2/integration` commit after the coordinator integrates Wave 2 and commits this ledger. This is a tactical one-human/multiple-chat split. The Wave 1 `cgx2-syntax` AST and Wave 2 `cgx2-sema` API are frozen for this wave; changing either would create hidden serialization between lanes. Estimated coordination cost is 15–25%, including four reviews and integration.

| Track | Exclusive write surface | Consumes | Produces |
| --- | --- | --- | --- |
| [Checked CLI](cli/plan.md) | `crates/cgx2-cli/**`, its plan | `cgx2-syntax`, `cgx2-sema`, existing interpreter | `cgx2 check`, static gate before CLI run, JSON Lines diagnostics |
| [Ownership analysis](ownership/plan.md) | `crates/cgx2-ownership/**`, its plan | Memory rules in design and runtime README | Conservative source-independent ownership/region operation checker |
| [Python source emitter](emit-python/plan.md) | `crates/cgx2-emit-python/**`, its plan | Frozen syntax AST and semantic summary | Readable Python for an explicit pure scalar subset; differential fixtures |
| [Public API index](api-index/plan.md) | `crates/cgx2-api-index/**`, its plan | Frozen syntax AST and semantic summary | Deterministic public signature index and machine-readable query output |

## Index-card contracts

- `cgx2_syntax::parse_module(&str) -> Result<Module, Vec<SyntaxDiagnostic>>` and `cgx2_sema::check_module(&Module) -> Result<TypedModule, Vec<TypeDiagnostic>>` are fixed inputs. No worker edits syntax or sema. The CLI uses them for static checking; emitter/index consume them without reaching into private fields.
- The CLI owns user-facing command parsing and human/JSON diagnostics. The emitter produces a library string, not a CLI command. The API index produces a library index, not `cgx2 api`. The coordinator wires these in a later wave.
- The ownership checker consumes its **own explicit operation IR**, not parser AST. It must report assumptions and unsupported control flow; it does not claim that arbitrary `.cg` source is memory safe. A later source-to-IR adapter can be designed after memory syntax is frozen.
- Root `Cargo.toml` uses `members = ["crates/*"]`; workers do not edit root manifests, root lockfile, shared docs, `AGENTS.md`, or another lane's files. New dependencies must be available offline or be reported as a blocker; root lockfile changes are coordinator-owned.
- Every worker runs exact tests, format check, and Clippy, commits only its owned paths, reports limits, and stops. The coordinator reviews and integrates on this isolated branch after the user says all chats are complete. `main` stays untouched.

## Why four

Parser changes for regions, contracts, and imports would modify exhaustive AST matches in the CLI and emitter while they are being built. A notebook executor adapter needs a persistent evaluator API that the current CLI does not expose. Native Rust emission needs a stronger typed IR than Wave 2's summary. Those tasks are staged after this wave instead of creating false parallelism or silently duplicating semantics.
