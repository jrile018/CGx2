# Parallel blocks and coordinator runbook

**Goal:** finish the confirmed CGx2 architecture, source-connected Lean assurance and comparative evaluation plan through independently reviewable worker blocks.
**Architecture:** one coordinator, at most three active workers, one isolated worktree/branch per writer, dependency-driven dispatch and serial integration. Separate worker chats and in-chat subagents use the same block contracts and result format.
**Tech stack:** Git, Rust, Lean, Python, native CPU/GPU libraries; this board uses Python's standard library only.
**Spec:** [confirmed scope](../plan.md), [completed checkpoint](../execution.md), and [checkpoint findings](../../../research/architecture-review/README.md).

This is the **successor** to checkpoint `09447b4343b6cf49a0fc55be4f9c9e733edc3e72`. The checkpoint remains complete as a bounded review, while its full-runtime proof, source ownership, GPU ML and comparisons remain open. Its worker-no-commit rule described the shared-worktree checkpoint only. Future writers follow this runbook: isolated worktree, bounded worker commit, coordinator review/integration, then stop.

## The three files that control work

- [blocks.json](blocks.json): authoritative block IDs, dependencies, owned paths, resource locks, exact deliverables, steps, acceptance and verification. Commands for successor outputs are planned deliverables; their existence here is not evidence that they have run.
- [state.json](state.json): coordinator-only status, attempts, worker/chat IDs, worktrees, branches, commit IDs, receipts and decisions. It is the handoff record for resuming in another coordinator chat.
- [board.py](board.py): read-only graph/state validation, ready-work selection and self-contained Markdown worker packets. It does not spawn chats, edit the ledger, install tools, run workload tests, or integrate Git changes.

The board validates structure and recorded Git ancestry. It does not verify receipt contents, actual worker checkout state, or physical host inactivity. The coordinator must check those and append the resource preflight before sending a generated packet.

The common assigned `plan.md` is [the confirmed plan](../plan.md); a worker also reads its complete block object and generated packet. No worker depends on conversation history to infer its assignment.

## Blocks and parallel waves

The wave numbers are planning bands, **not barriers**. For example, O1 depends on P1 even though both are in band 2. Start a successor as soon as its own accepted predecessors and gates are satisfied; do not wait for unrelated workers. GPU blockage does not stop CPU fixtures or measurements.

| Block | Bounded deliverable | Hard predecessors | Decision/evidence gates |
| --- | --- | --- | --- |
| P0 | Choose an exact runtime proof boundary | Checkpoint | — |
| O0 | Define the ownership source slice and pass contracts | Checkpoint | — |
| F0 | Freeze common fixture interfaces and independent oracles | Checkpoint | — |
| E0 | Resolve the GPU package closure and storage bounds | Checkpoint | — |
| E1 | Validate CGx2 native CPU package integration | Checkpoint | — |
| P1 | Close the selected runtime proof obligations | P0 | proof_scope |
| O1 | Connect the chosen ownership slice to source execution | O0, P1 | ownership_scope |
| C1 | Check the selected compiler preservation relations | O1 | — |
| E2 | Validate isolated GPU and CGx2 backend integration | E0, E1 | gpu_resources |
| FN | Implement native comparison fixtures | F0, O1 | — |
| FM | Implement CPU data/ML comparison fixtures | F0, E1 | — |
| FG | Implement GPU comparison fixtures | F0, E1, E2 | — |
| FA | Implement general application fixtures | F0, O1 | — |
| S0 | Prepare the no-spend authoring study | FA | — |
| MC | Run CPU comparisons | FN, FM, FA | — |
| MG | Run GPU comparisons | FG | — |
| D0 | Present one evidence-based optimization choice | P1, C1, MC | — |
| X0 | Implement one selected optimization | D0 | optimization_choice |
| V0 | Recheck and remeasure the selected candidate | X0, MC | candidate_baseline |
| Z0 | Integrate the verified review and final claim ledger | P1, O1, C1, S0, MC, MG, V0 | — |

Bands: 1 prepares independent contracts and environments; 2 closes selected implementation obligations; 3 builds disjoint fixture/proof outputs; 4 runs measurements and prepares the study; 5 selects and implements one optimization; 6 rechecks the candidate and assembles the review. Detailed ownership is in blocks.json. Every worker owns `research/architecture-review/next/<ID>/`; any production write surface is additionally named there. The coordinator owns this plan/ledger and `next/integration/`.

Initial eligible blocks are P0, O0, F0, E0 and E1. Launch at most three; `ready` ranks the current frontier using relative S/M/L work weights (1/2/3), longest remaining dependency path and contention. These are scheduling estimates, not elapsed-time or speedup claims. P1 precedes O1 because both affect runtime semantics. E1 precedes E2 because both own the native facade. Optimizations follow the evidence they consume. Independent fixture directories can be implemented together.

## Daily orchestration in three commands

Run from the attached execution worktree:

```sh
python3 plans/lean-architecture-review/parallel/board.py check
python3 plans/lean-architecture-review/parallel/board.py ready
python3 plans/lean-architecture-review/parallel/board.py packet P0 --preview
```

`--preview` is a planning brief, not a dispatchable assignment. To generate an executable assignment, supply the actual full base SHA, absolute worker checkout and branch:

```text
python3 plans/lean-architecture-review/parallel/board.py packet P0   --base FULL_40_CHARACTER_COMMITTED_BASE_SHA   --worktree /ABSOLUTE/MANAGED/WORKTREE/lang   --branch riley/lean-p0-a01 --attempt 1
```

Replace uppercase values with inspected values. The base must contain this exact successor block contract, the recorded integration head, and accepted dependency commits. Record it before dispatch; a branch name that may move is insufficient. `state.integration_head` tracks the last accepted source/evidence integration checkpoint; documentation-only commits can follow it. Use the current committed plan HEAD as the first worker base, after checking ancestry. No placeholder command above has been executed as a worker assignment.

## Dispatch and return loop

1. Run `check` and `ready`; choose one eligible block and its next attempt. Never dispatch a second attempt while the old worker can still write.
2. Reuse a suitable managed checkout or create a fresh one from the pinned accepted integration base. Use `riley/lean-<block>-a<attempt>` branches. Confirm clean state, exact HEAD and assigned branch; never reset another worker's dirty checkout. Generate the executable packet while the block is still pending. Then reserve it in state with `worker_kind` (`subagent` or `chat`), the ready `worker_id`, absolute `worktree`, `branch`, exact `base_commit`, `attempt` and `status=running` before sending the coding assignment. A new worker may bootstrap its checkout before this reservation, but must not begin block work. Shared source/manifest changes remain coordinator-owned serialized prerequisites.
3. Give the complete packet to one leaf worker. For bounded subtasks use `collaboration.spawn_agent` with isolated context and the explicit worktree. When the user asks for separate persistent chats, use the app's `list_projects`/`create_thread` worktree flow and record the returned **ready threadId** (not a pending clientThreadId). A new chat may bootstrap its clean checkout first; dispatch its complete packet once the checkout path/base are verified. Existing chat assignments are continued with `send_message_to_thread` when user authorization covers that worker coordination. Use `read_thread` to recover context and `wait_threads` for completion; avoid repeated status polling. Reuse existing authorized workers for focused corrections rather than creating duplicate chats.
4. A worker reads AGENTS.md, the common plan, this runbook, its block, design/validation/roadmap and predecessor reports before coding; calls MegaMemory overview/query; verifies all inputs and availability; performs only its assignment; writes its result/evidence; commits its own changed paths; records knowledge; returns the final SHA and stops. It does not edit this ledger, merge another branch, launch successors, delegate, or send unsolicited cross-chat messages.
5. On return, set `status=review` and record `worker_commit` plus `receipt`. Inspect the actual commit diff and checks. Reopen the same worker for bounded fixes when needed. A terminal DONE message is a request for review, not a completion transition.
6. Integrate and verify using the procedure below. Only then set `status=integrated`, `integration_commit` and `integration_receipt`. Re-run the board and fill the next safe slot. If evidence fails, preserve it, set `blocked` or keep `review`, and keep independent ready blocks moving.

The coordinator holds the shared DAG, interfaces, user decisions, integration and final claims. Workers receive one bounded deliverable. Even in separate chats, they do not become independent orchestrators.

## Handoff inputs and stable interfaces

Every assignment carries:

- checkpoint and base commit identities, dependency integration SHAs, and pinned receipts containing artifact hashes;
- the exact block goal, owned paths, exclusions and verification commands;
- required environment/tool pins and a coordinator-supplied resource preflight: scratch location `/tmp/cgx2-<ID>-a<attempt>/`, storage cap and build concurrency allowance, recorded before dispatch;
- observed user decisions and pending gates, including the selected source/guarantee scope;
- the receipt template and allowed status vocabulary.

P0 owns `next/P0/proof-contract.json`; O0 owns `next/O0/source-contract.json`; F0 owns `next/F0/contract.json`, `result.schema.json`, runner and independent oracles; E0 owns `next/E0/environment-contract.json`. These are outputs to implement, not already-existing contracts. Downstream workers consume exact accepted hashes and request changes through their own `change-request.md`; they never edit a predecessor's interface or oracle. The coordinator sequences an interface-owner correction, records a new version/hash and invalidates affected dependent receipts before restarting them.

The frozen 15-file ownership overlay currently lives at manifest-recorded private paths. Verify its availability/hash in each worker context. Same-host worker chats may read the pinned snapshot. A different host/cloud worker must receive an authorized portable snapshot and verified tool setup, or remain blocked; a Git manifest is not the overlay contents. Do not commit that private overlay or shared environment by accident.

Checkpoint evidence under `research/architecture-review/{architecture,proof,environment,comparisons}` and its input manifest stays immutable. New source candidates receive their own manifest and proof/evidence under `next/<ID>/`. If runtime source changes, historical integrity checks run on a reconstructed/isolated checkpoint checkout. Do not loosen historical hashes to make a candidate pass. Checkpoint `.tmp` tool paths are local prerequisites, not portable installation guarantees.

## Shared resources and user decisions

- At most three workers run alongside the coordinator. Production path overlap must be ordered by dependencies. Worktree isolation protects edits but does not make overlapping contracts independent.
- Hold `runtime_writer`, `compiler_writer`, `native_sdk_writer` or `proof_toolchain` locks where declared. Shared Cargo manifests/lockfiles, design documents and integration checks remain coordinator-owned; record a change request instead of broadening a worker's write permission.
- Timed MC/MG/V0 runs hold `measurement_host` exclusively. Suspend all other local build/install/proof/device workloads and record the actual environment before timing. GPU work also holds `gpu_device`. State presently assumes one host; multi-host scheduling requires explicit host identity and per-host locks before use.
- A gate is an evidence/decision record, not an automatic new permission request. Reuse existing user authorization for routine and semantics-preserving work. The coordinator records proof/ownership scopes and bounded GPU resource feasibility. Ask the user only about concrete substantive tradeoffs, a real environment/system/resource scope increase, or a later paid trial.
- X0 waits for the user's specific optimization choice. If the candidate needs a different write set, update the block contract and serialize overlaps **before** dispatch. GPU-dependent optimization requires MG in the relevant dependency chain before dispatch; `candidate_baseline` cannot be approved without the matching baseline.
- Keeping the current design is a legitimate user outcome. In that case the coordinator explicitly replaces X0/V0 with a documented retain-current decision block and updates Z0 dependencies; do not mark unexecuted optimization work integrated. The graph is reviewed again after the change.

## Worker receipt and simple integration

Use [result-template.json](result-template.json) in the owned `next/<ID>/result.json`. The committed receipt contains the base/dependency/source identities, checks and artifacts. The final worker commit SHA is returned **outside that commit** and stored by the coordinator; putting a commit's own hash inside itself is circular.

Minimum fields: `block_id`, positive `attempt`, `status` (`DONE`, `DONE_WITH_CONCERNS`, `NEEDS_CONTEXT`, `BLOCKED`), `base_commit`, `dependency_commits`, `scope`, `changed_paths`, `artifacts` with repository-relative paths/SHA256, `checks` with command/cwd/expected and actual exit/status/evidence, `claims` with exact evidence scope, and `blockers`/`decisions_used`. Each receipt must distinguish what was proved, tested, inspected, blocked and unrun. A block-specific `verify.py` must reject missing inputs/artifacts, false success rows and violated acceptance contracts; artifact hashes alone cannot establish workload or proof correctness.

Integration is one block at a time on `riley/lean-architecture-review-execution`:

1. Fetch/inspect the worker's exact commit if needed. Verify its base is the assigned SHA, its dependency artifacts match the receipt, and every changed path belongs to the block. Review all commits in `BASE..WORKER_SHA`; no hidden history or unrelated files.
2. Check the source/evidence hashes, exact commands, expected-failure controls and required positives. Independently rerun the block's mandatory checks from its candidate checkout. Concrete proof/correctness concerns receive an independent reviewer; ordinary documentation does not require ceremonial extra agents.
3. Ensure all predecessor integration commits are in the current integration base. Apply the reviewed worker commit(s) in their original order with cherry-pick onto a clean isolated integration branch. Keep one reviewable commit per coherent block where practical; do not squash away evidence of failed attempts. A conflict stops that integration: preserve the worker checkout and assign a focused correction using the new exact base. Do not resolve an unreviewed semantic conflict by accepting one side wholesale.
4. Rerun affected checks against the **combined** tree, including consumers when an interface/source dependency changed. Record `next/integration/<ID>-a<attempt>.json`: worker/base/dependency commits, resulting integration commit, changed paths, fresh commands/results, scope accepted, open concerns and the reviewer. Cherry-picking changes SHA; retain both worker and integration identities.
5. Mark integrated only when the promised bounded acceptance criteria pass. Publish the worker/integration branches to `jrile018/CGx2` under the already authorized GitHub scope and verify attribution. Archive a managed worktree only after its commits and required nonignored/ignored evidence are preserved and integration is verified. Main is not updated by this runbook.

A diagnostic block can finish with a documented limitation where its acceptance calls for a finding. A capability block that requires actual source acceptance, proof closure or GPU execution cannot be integrated merely because it documents failure. Block acceptance, established scientific claims and overall objective completion are three separate facts.

## Re-orchestrate after interruption or in another coordinator chat

Read this runbook, the common plan, blocks.json, state.json and MegaMemory. Inspect the recorded workers/worktrees/branches and their latest receipts before sending anything. Reconcile active workers, completed commits and the integration branch; never infer completion from a chat title or absence of activity. Validate the graph, decisions and current commits, then run `ready`.

For reassignment: stop or obtain a terminal acknowledgement from the old writer, preserve its branch and evidence, increment `attempt`, create a clean checkout from the current accepted integration base, and generate a new packet. Include the old attempt's receipt and unresolved issue. Retain previous attempt records in `next/integration/attempts/`; do not silently overwrite them. Downstream packets pin the corrected dependency versions. Running/review workers continue to reserve their paths until reconciled.

If prerequisites or scope change, the coordinator edits the graph and records why, then revalidates cycles, ownership and affected receipts. When an integrated predecessor changes, mark affected descendants for review/blocking and revalidate their evidence; stale integration SHAs do not automatically authorize reuse. Stop after each worker's bounded deliverable; the coordinator alone starts successors under the user's authorization.

## Review focus and completion

Watch for reduced-Session proofs relabeled as full runtime, source positives downgraded to negatives, stale oracle/dependency hashes, GPU import/fallback counted as execution, shared-library work attributed to language speed, and authoring preparation counted as productivity results. Each corresponding block has an explicit acceptance check.

The final Z0 report requires all applicable accepted receipts and preserves unresolved proof/GPU/source/LLM rows. Actual model trials remain a later resource decision; S0 completion proves only preparation. A user can revise scope explicitly, but the report must retain that revision and cannot advertise the original unfulfilled objective as complete.

## Board verification

Run `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s plans/lean-architecture-review/parallel -p test_board.py`. The 26 tests cover graph revisions, invalid dependencies/cycles, missing decisions, ownership and resource conflicts, assignment validation, and Git preflight including stale contracts and missing integration ancestry. These checks validate orchestration behavior; they are not new Lean or language-comparison results.
