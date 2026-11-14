# How to read this checkpoint

This review distinguishes desired behavior, code that exists, tests that ran, and proofs of particular semantics. These are different kinds of evidence and are never interchangeable.

| Label | Meaning | Does not establish |
| --- | --- | --- |
| Intended | A specified property or design requirement | An implementation exists |
| Implemented | Inspected source contains the relevant route | Whole-program correctness or accepted release support |
| Implemented, unaccepted | Frozen unfinished source contains a candidate | Source integration or acceptance |
| Historically tested | Earlier evidence recorded a result under its exact pins | Current environment compatibility |
| Reference-model proved | Lean checks a theorem about the declared model | That the actual Rust/compiler follows that model |
| Implementation-connected | An explicit source subset/translation/context and checked theorem are connected by recorded provenance and assumptions | Correctness of excluded context, unsupported passes, host allocation or the whole compiler |
| Freshly tested | A recorded command completed with the stated observations | Universal behavior, general safety or comparative superiority |
| Blocked/not run | A prerequisite or required execution is missing | Passing, failure of the language design, or evidence that another platform would behave identically |

Actual-source translation has its own trusted boundary: pinned Rust/MIR frontend, extraction/translation implementation, library models, Lean kernel/foundations, and any opaque host operations. A source-including projection harness uses real function bodies but changes the surrounding context; the projection and linkage to the full Session must be stated separately. Generated source identity is not itself a semantic-preservation proof.

A failed allocation may alter diagnostics and temporary host reservations. The useful protected-block theorem concerns published handles/payloads and committed logical charges, with permitted observations defined explicitly. It must not hide counter overflow if that can panic or otherwise alter control flow. A fixed logical metadata charge does not establish a process-RSS bound. Excluding CGx2 collection does not establish a total real-time latency bound or control foreign heaps.

For comparisons, correct equivalent behavior precedes timing. Rust without tracing GC already avoids that collector's pauses; other languages may implement equivalent budget policies. A built-in policy can improve convenience without being a unique mathematical guarantee. Shared Torch kernels primarily measure shared backend work unless copies, dispatch, transfer and synchronization are accounted for separately.

GPU device enumeration, successful raw GPU compute, Python GPU library execution, native LibTorch GPU execution, and CGx2 source-to-GPU execution are separate rows. A pass in one row does not close the next. Software fallback must not count as hardware GPU success.

No authoring-productivity result exists merely because a task suite or scoring harness has been prepared. New externally funded model trials are outside this initial no-spend execution.
