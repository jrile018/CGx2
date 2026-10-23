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

## Reviewed integration — 2026-10-23

All four clean worker deliveries descend from `9b8f6f5610e697429b364528fa32a6b716aa288a` and stay within their assigned paths. They were cherry-picked into the attached coordinator worktree on `riley/wave23-review`. No worker branch or original checkout was changed. `main` remains `ce708328a06e5a48182107a0d0e4ffe77eef7d5d`.

| Lane | Worker commit | Integrated commit |
| --- | --- | --- |
| Automatic collection pressure | `e1f5921` | `fc351da` |
| Native Python boundary | `99cdf44` | `8490d43` |
| Notebook record functions | `68d4278` | `42a8d45` |
| Release float32 baselines | `66282e3` | `f33663b` |

Coordinator review found no demonstrated integration defect requiring a code repair. The worker boundary uses bounded nonblocking transport, validates before spawning, checks interpreter/protocol identity and output shape, and kills/reaps failed children. Notebook record annotations publish and consume `type:<name>` dependencies, including unused annotations, so edits/deletions invalidate their users. The automatic model distinguishes reserves before and after the collection safepoint. Benchmark ratio summaries recompute exactly from the committed case summaries.

### Independent verification

Commands ran from the coordinator worktree with Rust 1.98.1, `CARGO_TARGET_DIR=/home/john-riley/projects/lang/target`, and workspace temporary storage. Opted-in PyO3 tests use `LIBRARY_PATH=/tmp/cgx2-wave22-lib`, an existing linker-name workaround; default native builds do not need it. [Verification metadata](evidence/verification.json) records log paths and hashes.

- `cargo test --workspace --all-features --offline`: **730 passed, 5 ignored**, including **19 HTTP tests**. Conditional external tests without supplied dependencies retain their own skip behavior; this total is not proof that every optional external route ran.
- `python3 -m unittest discover -s bench/tests`: **112 passed**.
- `cargo fmt --all --check`, `git diff --check`, and `cargo clippy --workspace --all-targets --all-features --offline -- -D warnings`: passed.
- `python3 bench/readiness.py --build`: pinned toolchain and six public reference-task equivalence checks pass. Compiler subtree and Cargo.lock pins did not change. Human review, paired model traces, and scored cgroup isolation remain absent.
- `CGX2_W23_SEED=0 CGX2_W23_COUNT=2048 CGX2_W23_STEPS=256 cargo test -p cgx2-runtime --features fault-injection --test automatic_pressure_wave23 --offline`: all six tests passed, including the expanded corpus and intentional oracle corruption test.
- Exact default debug/release builds and a workspace all-features build passed the committed boundary verifier. See [debug](evidence/debug-python-boundary.json), [release](evidence/release-python-boundary.json), and [unified features](evidence/unified-python-boundary.json). Both `cgx2` and `cgx2-native` lack libpython ELF dependencies; sampled native maps lack libpython. Scalar output is `5000`; explicit isolated Python output is `9`. Only the explicit Python route launches `/usr/bin/python3.14`.
- The **release real pinned f32 route completed**, returned `[[58.0, 64.0], [139.0, 154.0]]`, and sampled LibTorch mappings without libpython or a Python child. It used `/tmp/cgx2-wave23-boundary-f32`, real CPU Torch 2.11.0, with package hashes retained in the release evidence. Debug and unified-feature coordinator map checks used scalar/Python routes; their real f32 gate was not rerun. The worker also supplies its separate real-path evidence.

### Result and remaining gates

The default native process no longer eagerly loads Python. Existing copied scalar Python calls are isolated, while host arrays and handles remain behind the explicit `in-process` feature. The worker is Linux x86_64/aarch64 only, defaults to `python3`, has an embedded script and a maximum five-second deadline, and is local/unpinned. General packages, persistent sessions, and array/handle source syntax are not added.

Record-typed pure notebook functions now cover local scalar records; imported/effectful/contract-bearing profiles remain unsupported. Memory pressure/failure evidence is finite direct-runtime conformance. The worker's ASan run passes with leak detection disabled; its leak-enabled attempt is blocked by host ptrace restrictions. No leak-freedom or general safety gate is closed.

The frozen-base [f32 report](../../bench/WAVE23-F32-RESULTS.md) is valid: five independent cold samples per route/shape and 30 warm samples. Release source medians are about 6.4 seconds versus C++ about 0.103 seconds. Warm facade medians/p95 are slower than C++ at both 128 and 512. The report correctly observes libpython on its older frozen base; the later integrated boundary evidence does not retroactively change those measurements. No inference or speed superiority target is claimed.

Tracked implementation stays **92%**. Broad semantics/libraries, leak-enabled memory assurance, imported records, live trusted proof, scored isolation, and genuine paired LLM/human evidence remain open. Wave 24 is separately authorized by the user's request and is defined in [its ledger](../wave24/README.md).
