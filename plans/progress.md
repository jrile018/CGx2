# First-release implementation progress

The estimate measures tracked implementation scope, not release readiness, a safety proof, or benchmark superiority. Credit changes only when an area's remaining acceptance gate closes.

| First-release area | Weight | Credit now | Evidence and largest gap |
| --- | --- | --- | --- |
| End-to-end safe memory | 30 | 29 | Wave 23 automatic-pressure/failure oracle and 2,048-seed fault corpus pass. Leak-enabled instrumentation is host-blocked; broad source/foreign/parallel ownership and the full safety gate remain open. |
| Core language and native toolchain | 20 | 19 | Immutable scalar records have interpreter/native parity; bounded float32 and float64 matrix source calls run. Conditionals, variants, richer typed object IR, broader signatures, and complete semantics remain open. |
| Modules and code understanding | 10 | 9 | Acyclic public scalar package calls run natively and in notebooks. Record APIs across modules, richer source graphs, and ecosystem resolution remain open. |
| Library access and source translation | 10 | 9 | Native debug/release and unified-feature binaries are Python-free; explicit scalar Python calls use a bounded child. Real f32 numerics and Python record emission pass. General libraries, tensor types, and portable foreign packages remain open. |
| Native notebook | 10 | 9 | Local record-typed pure functions retain annotation dependencies, invalidation, save/reopen, and CLI/UI execution. Imported records, effectful workflows, and broader notebook execution remain open. |
| Effects, contracts, proof, security | 10 | 9 | Bounded effects/contracts/security checks and solver trust validation exist. Live trusted Z3/CVC5 proof and full memory/foreign proof remain open. |
| LLM and runtime evidence | 10 | 8 | Real release f32 runs have five cold and 30 warm samples; source startup remains costly and the warm facade is slower than C++. General runtime/inference targets, genuine paired model trials, human review, and parallel/GPU evidence remain open. |
| **Total** | **100** | **92** | Memory correctness and other first-release acceptance gates still block release. |

## Latest reviewed evidence

Wave 23 integrated four owned deliveries on `riley/wave23-review`. Combined all-feature Rust tests (730 passed, five ignored, including 19 HTTP tests), 112 Python benchmark tests, six readiness reference checks, formatting, Clippy, and expanded memory pressure replay pass. Independent default debug/release and workspace-unified ELF/map checks confirm a Python-free native process while explicit Python calls execute in an isolated child. A real pinned release f32 call returned the expected matrix. No code integration repair was required. Leak detection remains host-blocked and performance/model release targets remain unscored. See [Wave 23 review](wave23/README.md) and [Wave 24 plans](wave24/README.md).
