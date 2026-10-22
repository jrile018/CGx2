# Wave 6: memory conformance and usable frontends

All five chats start from the same verified `wave6/integration` commit after Wave 5 integration. This is a tactical split with an expected 15–25% review/integration cost. Each chat works in a separate user-owned worktree, commits only its bounded paths, reports tests and limitations, and stops for review. No automation, worker-to-worker messaging, or merge to `main`.

| Lane | Exclusive write surface | Frozen Wave 5 input | Reviewable output |
| --- | --- | --- | --- |
| [Memory conversions](memory-conversions/plan.md) | `crates/cgx2-sema/**`, `cgx2-ownership/**`, `cgx2-cli/**` | Existing AST and runtime owned/managed APIs | Source `promote_owned` and `copy_to_owned` with static ownership and runtime checks |
| [Memory conformance](memory-conformance/plan.md) | New `crates/cgx2-memory-conformance/**` | Wave 5 memory script API and runtime reports | Deterministic generated memory corpus, invalid fixtures, reproducible seeds |
| [Native scalar compiler](native-scalar-compiler/plan.md) | New `crates/cgx2c/**` | Rust emitter, parser, sema | `cgx2c build FILE.cg --out BIN` for supported scalar source |
| [Notebook UI](notebook-ui/plan.md) | New `crates/cgx2-notebook-ui/**` | `.cgnb` format and pure-scalar kernel | Safe loopback browser UI for editing, clean run, and save |
| [Effect/call index](effect-index/plan.md) | New `crates/cgx2-effect-index/**` | Frozen AST and public function summaries | Conservative source-located call/effect index for code understanding and later optimization |

## Shared contract

- Source AST and parser are frozen. The memory lane extends only recognized `memory.*` calls, which the existing parser accepts, and owns sema/ownership/CLI changes. The other four lanes consume the Wave 5 public APIs and cannot edit shared crates. If an interface is missing, report it and implement a bounded adapter inside the owned new crate rather than changing another lane.
- Memory conversions must preserve one owner and explicit copies; a failed promotion cannot silently lose or duplicate the owned value. The conformance lane targets the **frozen Wave 5 profile** so its tests are independent of the conversion lane; integration can add conversion cases later.
- `cgx2c` emits only the checked scalar Rust profile. Memory scripts, source effects, and Python interop must produce target diagnostics rather than altered behavior. The notebook UI is a pure-scalar preview; it must not claim Jupyter's full persistent arbitrary-cell semantics or execute untrusted HTML.
- The effect/call index is **advisory**. Unknown or dynamic calls remain `Unknown` and block an automatic claim of purity or safe parallelism. It does not add effect annotations or enforce capabilities in source.
- Root `Cargo.toml`, `Cargo.lock`, `README.md`, `docs/**`, and roadmap/progress files are coordinator-owned. Workers run offline package tests, fmt, and Clippy. Review requires combined workspace tests, Python bridge test with its local linker setup, CLI memory/report smoke, native scalar differential output, notebook UI smoke, and benchmark parity. Real PyTorch and GPU remain unavailable unless verified separately.

## Later dependency gate

Source imports, effects/capability declarations, contracts, whole-language typed IR, richer memory syntax, persistent notebook side effects, and general GPU execution would change shared AST/semantic interfaces. Plan those after this frozen-syntax wave instead of assigning conflicting edits to parallel chats. The first-release memory correctness gate remains open until broader model, fuzz, and audit evidence is integrated.
