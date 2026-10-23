# Wave 18: observe real roots and turn prototypes into usable workflows

All six coding chats start at the same Wave 18 plan commit on `wave18/integration`, based on the reviewed Wave 17 integration commit. Read `AGENTS.md`, `docs/language-design.md`, `docs/validation-plan.md`, `plans/roadmap.md`, this ledger, and your assigned `plan.md` before coding. Call project knowledge graph overview and query as instructed. Work only in your attached worktree and exclusive paths below. Run your specified checks, commit one bounded deliverable, record project knowledge, and stop for the user's review. Do not merge branches, automate follow-on work, create more chats, edit another lane, or edit the root Cargo manifest/lock or shared design, roadmap, progress, and coordinator ledger. Report actual evidence and unsupported host capabilities.

| Lane | Exclusive write surface | Bounded deliverable |
| --- | --- | --- |
| [Runtime root parity](runtime-root-parity/plan.md) | `crates/cgx2-cli/**`, `crates/cgx2-emit-rust/**`, `crates/cgx2c/**` | Source and native failure reports consume runtime root census, with debug/release parity |
| [Memory state assurance](memory-state-assurance/plan.md) | `crates/cgx2-runtime/**`, `crates/cgx2-memory-conformance/**` | Faulted collector/root lifecycle against a bounded independent state model |
| [Pinned LibTorch package](pinned-libtorch-package/plan.md) | `crates/cgx2-bind/**`, `crates/cgx2-libtorch/**` | Verify copied schema-2 package and run a reproducible real CPU adapter path if available |
| [Notebook function lifecycle](notebook-function-lifecycle/plan.md) | The five `crates/cgx2-notebook-*` crates | File-backed pure-function cells across rerun, deletion, restart, and UI/CLI output |
| [Proof boundary](proof-boundary/plan.md) | `crates/cgx2-verify-core/**`, `crates/cgx2-verify-cli/**` | Executable solver trust boundary and hostile-result tests without broad proof claims |
| [Blinded LLM pilot](blinded-llm-pilot/plan.md) | `bench/**` | Export a public-only packet and produce honest paired-trial evidence or an explicit unrun result |

## Baseline and release gates

Wave 17 provides `Session::root_census()` and bounded scalar arguments to local memory functions, but source/native failure snapshots still count bindings. A pinned LibTorch package recipe exists, but the normal copied package and independently reproducible real CPU evidence are open. Pure notebook functions work in a bounded kernel; output invalidation across lifecycle changes needs stronger coverage. The proof encoding passed supported-fragment finite checks without a live trusted solver result. The corrected syntax pilot contains public `TODO()` starters and private references; zero genuine paired LLM trials have run. Candidate cgroup isolation is unsupported on this host because CPU, memory, and PID controllers are not delegated.

Memory correctness remains the highest release gate. A zero root census establishes only that registered roots were released; it does not prove absence of leaks, use-after-free, safe foreign code, or parallel safety. Real LibTorch, trusted proof, scored isolation, and measured LLM gains require reproducible evidence. Keep `main` unchanged pending review.

## Review result (2026-10-23)

All six worker commits were reviewed and cherry-picked onto `wave18/integration`. Source and native failure snapshots now expose actual runtime root registrations, and a direct-runtime state model exercises roots, cycles, regions, nested `no_gc`, and injected reserve failures. The current checked `.cg` memory subset cannot duplicate a managed root; a direct `Session::root` test covers that distinction while source/native fixtures check census timing and cleanup. These finite tests are not a whole-language memory-safety proof.

The copied, schema-2 pinned LibTorch package passed local preflight and an independently rerun CPU create/add/shape/copy/release test using the Torch 2.11 C++ libraries, with no mapped `libpython`; the real test took 144.51 seconds. The notebook adds function-definition deletion and transitive stale-output handling through kernel, CLI, and UI. The verifier hardens the selected-obligation trust boundary, but no live Z3/CVC5 proof ran. The benchmark can export public-only tasks. Coordinator review additionally bound paired-trial scoring to exact public contexts and exports, rejected reused provider IDs, and changed its output to `validated_paired_traces` rather than a claim of genuine trials. There are still zero genuine paired model traces and scored cgroup isolation remains unsupported on this host.

Combined offline workspace Rust tests, 102 Python benchmark tests, formatter, Clippy with warnings denied, 14 notebook UI HTTP tests with localhost socket permission, six syntax-reference compiler checks, and the real pinned LibTorch test pass. The compiler-source hash in the syntax-pilot toolchain lock was updated for the integrated native compiler. `main` remains unchanged. Memory release assurance, general source types, broad foreign compatibility, effectful notebook workflows, a trusted live solver, and measured LLM/runtime targets are still open.

## Coordinator integration gate

Review exact commits, ownership, tests, and evidence before cherry-picking. Run combined offline workspace tests, notebook UI tests, benchmark tests/readiness, formatter, Clippy, source/native failure parity, and any real foreign-library test that is reproducible on the host. Repair integration defects on the review branch and report remaining blockers without claiming a release.
