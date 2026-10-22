# CGx2

CGx2 is a proposed programming language and toolchain for memory-safe native programs, short human-readable source, and fast LLM-assisted development. It combines automatically managed memory with explicit owned values and programmer-controlled collection regions. It also aims to make program structure, effects, contracts, and compiler feedback easy for both people and coding agents to inspect.

**Status: runnable preview, incomplete language.** The `cgx2` CLI interprets a small scalar `.cg` subset; a separate semantic checker, memory runtime, notebook session, Python bridge, benchmark harness, and bounded verifier exist. The CLI does not yet use all of them together. There is no native compiler, live notebook UI, real PyTorch verification, or measured CGx2 benchmark yet. CGx2 uses `.cg` source files and `.cgnb` notebooks.

Read the [language design](docs/language-design.md), [validation plan](docs/validation-plan.md), and [implementation roadmap](docs/superpowers/plans/2026-10-22-first-compiler.md).

The [delivery roadmap](plans/roadmap.md) defines the repeated review-and-integration loop. The [first-wave contracts](plans/README.md) and [second-wave ledger](plans/wave2/README.md) record completed component plans; the [third-wave ledger](plans/wave3/README.md) assigns the next independent coding chats. Each chat uses a separate Git worktree and stops for user review after its bounded deliverable.

## What the first usable release must do

- Run a source file from the command line with `cgx2 run program.cg -- args` or `cgx2 program.cg -- args`; build native executables and inspect generated Rust.
- Use automatic memory management by default, with named automatic or manual-only collection regions, explicit owned/borrowed values, and budgeted `no_gc` sections that never silently collect.
- Check types, ownership, effects, capabilities, and contracts; run tests; prove a clearly bounded subset of properties; distinguish **proved**, **disproved**, and **unknown**.
- Call real Python packages, including PyTorch, through a typed common API and an explicit dynamic escape hatch. Python and PyTorch retain their own memory managers.
- Open and run CGx2's own readable `.cgnb` notebooks with Jupyter-like cells, rich output, reruns, and clean-session execution.
- Emit readable Rust and Python source for explicitly supported programs. Other ecosystems use declared adapters or process interfaces; source translation and foreign-library access are separate capabilities.
- Expose structured diagnostics and an API index for LLMs while keeping one readable source syntax for everyone.
- Meet the memory correctness gates and demonstrate the proposed LLM and runtime benefits in controlled benchmarks before making performance claims.

The first usable release is intentionally a broad milestone. Smaller internal previews will be runnable earlier, but they do not satisfy the full release contract. Reaching a mature platform comparable in breadth to OCaml and its tooling is a longer-term goal, not a description of the first release.

## Build order

1. Freeze an executable specification for memory, source syntax, diagnostics, and the benchmark harness. Compare indentation and brace syntax with actual LLM tasks before committing to the final grammar.
2. Build the parser, module system, typed intermediate representation, native Rust emitter, and small memory runtime.
3. Add managed/manual regions, owned conversions, contracts, effects, capabilities, testing, and bounded proofs; gate release on memory conformance.
4. Add Python/PyTorch bindings, CGx2-native notebooks, Python source emission for the supported subset, security and performance analysis, and a coherent CLI/editor workflow.
5. Run the full benchmark protocol in [the validation plan](docs/validation-plan.md), fix failures, and publish measured limits.

This design is based on the supplied October 2026 research proposal and the subsequent design interview. Its core hypothesis is that explicit memory behavior, concise interfaces, structured feedback, and independent verification can help coding agents reach correct results with less time and fewer tokens. That remains to be measured.
