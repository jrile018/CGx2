# CGx2 architecture review: first execution checkpoint

The review is executing on `riley/lean-architecture-review-execution`. This checkpoint freezes the intended design and unfinished ownership input, audits the implementation, attempts actual Rust-to-Lean translation, and establishes CPU/GPU readiness. It does **not** establish that CGx2 is better than Rust, OCaml or Python. The broader confirmed plan remains open.

## Inputs and evidence

Baseline commit: `4ed17189867b11e2a3c8280f02a05e3017f3ae5a`. The [input manifest](input-manifest.json) pins 518 baseline files and a separate 15-file unfinished ownership overlay. The original worktree and main were preserved. Overlay bytes are private local inputs at the manifest locations; the manifest alone does not make the overlay reproducible on another machine.

Read the [evidence policy](evidence-policy.md) before interpreting results. Source inspection, executable checks, generated-code checking, model proofs and compiler preservation are different claims. Passing an integrity verifier establishes its stated artifact checks, never language superiority.

## Architecture findings

The [architecture report](architecture/report.md) and [22-claim ledger](architecture/claims.json) map the intended complete design and 60 exact source anchors. Unfinished ownership is explicitly unaccepted. General ownership needs an end-to-end connection to admitted source programs; compiler authentication and recompilation do not prove preservation across the different execution and emission routes.

Protected-block rejection can change diagnostics. A whole-state unchanged theorem would therefore be false. The useful contract distinguishes published values, roots/owners and successful charges from permitted diagnostics, temporary reservations and error behavior. Arithmetic proofs need valid-state and machine-width assumptions; logical allocation charges are not resident-memory bounds. Foreign allocations, GPU buffers and asynchronous device work need separate lifetime and accounting contracts.

The collector repeatedly scans slots and reachable edges to a fixed point. With S physical slots, V reachable objects, E reachable edges and P passes, its conservative source-level bound is O(P(S+E)+S), P <= V+1. A worklist could reduce marking to O(V+E), with O(S) initialization/sweep remaining. This is an algorithm analysis, not a timing result. Worklist scratch allocation, retained capacity or intrusive metadata each changes memory costs or failure behavior; no representation change has been selected.

## CPU and GPU findings

The [environment report](environment/report.md) records standalone Rust, native OCaml and isolated CPython/Torch CPU fixtures. A native ATen executable linked against the same CPU wheel and its exercised process maps contained no libpython. This does not prove CGx2 SDK packaging, emitted-program integration or whole-package Python independence.

A real AMD RX 9070 XT executed synchronized Vulkan/WGSL integer and float compute with explicit device selection and exact readback. CPU fallback was prohibited. Mesa reports a non-conformant/testing warning, retained in the logs. PyTorch GPU allocation fails with the installed CPU-only wheel; GPU ML integration remains open. OpenCL has no platform. Vulkan success does not fill those rows.

The coordinator independently replayed `environment/probe.py --output /tmp/cgx2-arch-review-environment/coordinator-replay`: 13 checks passed and one expected PyTorch GPU check was blocked. [Replay receipts](verification/environment-replay/checks.json) and adjacent raw logs are retained. These untimed readiness probes establish neither performance nor comparative memory use.

The [version-specific GPU remedy review](environment/gpu-remedies.md) identifies an isolated AMD Torch2.13/ROCm10.1 user-space experiment that matches the observed OS/kernel/GPU rows. Four targeted artifacts total 1.142 GiB compressed; full dependency closure and peak/unpacked size are unknown. The host driver has not been matched to AMD's listed driver families, and replacement is not established as necessary. No GPU wheel was downloaded and no driver or package was changed. The older Torch2.11/ROCm7.2 wheel alone is 5.79 GiB and exceeds the environment lane's 3 GiB cap. This is a concrete next experiment, not a readiness pass.

## Proof and comparison status

Actual-source translation and kernel-check results are documented in [the proof report](proof/report.md). Aeneas/Charon extracted the runtime, but full-runtime translation encountered control-flow/iterator issues; a full budget-file projection also encountered guard trait-signature issues. A narrower byte-exact selection of the Budget declaration and Session budget implementation generated Lean that kernel-checks inside a purpose-built harness with a reduced Session context. A refinement relation from that context to the full Session has not been proved. Six lemmas about admission arithmetic and generated body steps are checked under explicit invariants and iterator assumptions. The coordinator independently reran extraction, generated-code identity, Lean kernel checking and a source predicate mutation that correctly failed the proof; [fresh replay receipts](verification/proof-replay/checks.json) and logs are retained. This is not a proof of complete iteration, container semantics, guard entry/exit, compiler preservation or the complete runtime. The report records the exact projection, target width, toolchain trust, opaque definitions and theorem dependencies. The earlier fixed-guard natural-number model remains a separate result.

The [comparison protocol](comparisons/protocol.md) and [case matrix](comparisons/matrix.json) cover native systems, data/ML and general/LLM development; CGx2, Rust, OCaml and Python; idiomatic and equivalently policy-equipped versions; CPU and GPU. Correctness, rejection behavior, numerical contracts and foreign/device policy boundaries precede timing. The 160 routes currently contain 44 blocked, 56 inapplicable and 60 unrun rows. Unrun or blocked cells are not comparative passes. No authoring trial was run and no paid services were used.

## What unfavorable results would mean

- **Native systems:** Rust already avoids tracing GC and has established ownership checking. If CGx2 only matches this by adding restrictions, a plausible niche must come from demonstrably easier mixed-memory programming or another measured benefit. That result would not justify a universal superiority claim.
- **Data/ML:** Python and native libraries already provide mature CPU/GPU paths. CGx2 must demonstrate its own package/backend integration and any reduction in copying, deployment dependencies or resource uncertainty. A standalone native smoke is insufficient.
- **General and LLM development:** New syntax and stronger constraints can improve some tasks while increasing repair burden in others. No-spend study preparation gives no evidence of authoring advantage; correct completions, tokens, time and failures must be measured.

The user will choose substantive changes from concrete results. The immediate engineering choices are the source-proof boundary and collector scratch/accounting policy, not a blanket retain/abandon decision. No production optimization, breaking source change, driver change or main merge is included in this checkpoint.

## Reproduction

Run from this execution worktree:

```sh
python3 research/architecture-review/verify.py
python3 research/architecture-review/architecture/verify.py
python3 research/architecture-review/environment/check.py
python3 research/architecture-review/proof/verify.py
python3 research/architecture-review/comparisons/verify.py
```

The proof report specifies its offline source/generated/proof checks and pinned external tool prerequisites. Environment replay requires the isolated runtime paths recorded in its setup files and GPU device visibility; artifact verification alone does not rerun device execution. Large toolchains and binaries are intentionally excluded from Git. Source-overlay verification needs one of the private snapshot paths recorded in the manifest.

Archived raw translator logs preserve terminal progress carriage returns and blank end lines. Full staged whitespace checking flags those original log bytes; checking all other staged files passes. [Whitespace receipt](verification/whitespace.json) records both scopes. Raw evidence was not normalized to hide these warnings.
