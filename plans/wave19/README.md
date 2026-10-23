# Wave 19: usable language breadth and measured CPU execution

All six coding chats start from the same Wave 19 plan commit on `wave19/integration`, based on the reviewed Wave 18 integration. Read `AGENTS.md`, `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`, this ledger, and your assigned `plan.md` before coding. Call knowledge graph overview and query first, and record your result after the task. Work only in the attached Git worktree and the exclusive paths below. Deliver one bounded coding change, run the checks in your plan, commit, and stop for the user's review. Do not merge, automate follow-on work, create more chats, edit another lane, or edit root Cargo files or shared design/progress/roadmap documents. Do not infer general claims from bounded examples.

| Lane | Exclusive write surface | Bounded deliverable |
| --- | --- | --- |
| [Memory unwind assurance](memory-unwind-assurance/plan.md) | `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**` | Replayable early-exit/panic/no-GC and root-lifecycle conformance, with dynamic-tool evidence |
| [Checked scalar range loops](checked-range-loops/plan.md) | `crates/cgx2-syntax/**`, `crates/cgx2-sema/**`, `crates/cgx2-cli/**`, `crates/cgx2-emit-rust/**`, `crates/cgx2c/**` | Existing readable `for` syntax executes allocation-free `range(start, end)` with interpreter/native parity |
| [Direct CPU matrix route](direct-cpu-matrix/plan.md) | `crates/cgx2-bind/**`, `crates/cgx2-libtorch/**` | Additive, pinned two-dimensional float64 LibTorch matrix operation without Python at runtime |
| [Notebook experiment sweep](notebook-experiment-sweep/plan.md) | The five `crates/cgx2-notebook-*` crates | Clean, bounded `.cgnb` parameter sweeps and saved output provenance |
| [Python package translation](python-package-translation/plan.md) | `crates/cgx2-package/**`, `crates/cgx2-emit-python/**` | Emit and run an acyclic local scalar package as readable Python modules |
| [Measured parallel zip map](measured-parallel-zip-map/plan.md) | `crates/cgx2-execution-policy/**` | Correct sequential/parallel checked two-input map with conservative measured `Auto` fallback |

## Why these lanes

Wave 18 strengthened root observability and direct-runtime tests, reproduced one real copied LibTorch CPU package, and made pure notebook function deletion safe. It did not close the memory release gate. The language still lacks convenient executable loops and broader structured values; Python source output is standalone only; the direct C++ adapter covers one-dimensional float64 tensors; notebook experimentation is manual; the measured parallel policy covers one isolated primitive and has no GPU backend. Wave 19 adds bounded user-facing behavior at seams that can be edited independently. Range loops remain a scalar preview and do not add managed heap objects. Matrix operations remain an explicit direct C++ adapter; the source `.cg` tensor syntax stays bounded until a separate core integration. The parallel zip map remains a reviewed primitive, not arbitrary `.cg` automatic parallelization.

The proof and LLM lanes pause for this wave: the host has no Z3/CVC5 executable, no genuine paired model telemetry, and no delegated CPU/memory/PID cgroup controls for scored candidate isolation. Their current code and evidence remain intact. A mock proof or self-authored trial would not close those gates. No worker should silently weaken them.

## Coordinator integration gate

Review exact commits and path ownership before integrating. Run combined offline Rust workspace tests, notebook loopback UI tests, Python benchmark tests/readiness, formatter, Clippy, source/native loop parity, translated package execution, and copied real LibTorch matrix test if the local pinned package can be rebuilt. Fix concrete integration defects on the review branch. Keep `main` unchanged until the user reviews the combined result.

## Review result

All six worker commits were reviewed for exclusive path ownership and integrated on `wave19/integration`: memory `ac64986`, range loops `b4de0d4`, Python package translation `db79b94`, CPU matrix `d38dbac`, notebook sweep `f025072`, and parallel zip map `e1f142e`. The coordinator updated the root `Cargo.lock` for Python-emitter dependencies and the frozen benchmark compiler/Cargo hashes; those changes belong to the integration branch only.

Combined offline workspace Rust tests (excluding the loopback UI), UI loopback tests, 102 Python benchmark tests, readiness checks, `cargo fmt --all --check`, and workspace Clippy with `-D warnings` passed. The ignored real pinned LibTorch CPU matrix test was rerun independently: one test passed in 190.21 seconds. The matrix result is specific to the copied Torch 2.11 CPU package on this host. The source/native range-loop and translated package cases are covered by the workspace integration tests.

Review did not close the memory release gate: the sanitizer run covered the direct runtime with leak detection disabled. Notebook sweeps deliberately discard old rich outputs before each trial; the format tracks assets through those outputs, and markdown file links have no portable-asset contract yet. Symlink component checks are path based and could race a concurrent writer in an untrusted directory. The standalone checked parallel zip map is not compiled from `.cg` and has no GPU path. Proof and LLM evidence remain blocked by absent local solver, genuine paired model trials, and scored candidate isolation.
