# First-release implementation progress

The estimate measures tracked implementation scope, not release readiness, a safety proof, or benchmark superiority. Credit changes only when an area's remaining acceptance gate closes.

| First-release area | Weight | Credit now | Evidence and largest gap |
| --- | --- | --- | --- |
| End-to-end safe memory | 30 | 29 | Wave 22 adds a four-seed region graph oracle, reserve-failure cases, nested-guard retry, and source/native transfer/escape parity. Automatic collection under pressure, leaks, broad source/foreign/parallel ownership, and the full memory release gate remain open. |
| Core language and native toolchain | 20 | 19 | Immutable scalar records have interpreter/native parity; bounded float32 and float64 matrix source calls run. Conditionals, variants, richer typed object IR, broader signatures, and complete semantics remain open. |
| Modules and code understanding | 10 | 9 | Acyclic public scalar package calls run natively and in notebooks. Record APIs across modules, richer source graphs, and ecosystem resolution remain open. |
| Library access and source translation | 10 | 9 | Direct native float32 source numerics and standalone Python record emission pass. The default CLI still eagerly links libpython; general libraries, tensor types, and portable foreign packages remain open. |
| Native notebook | 10 | 9 | Local immutable record cells retain state, invalidate dependents, save/reopen and rerun through CLI/UI. Record-typed pure functions, imported records, effectful workflows, and broader notebook execution remain open. |
| Effects, contracts, proof, security | 10 | 9 | Bounded effects/contracts/security checks and solver trust validation exist. Live trusted Z3/CVC5 proof and full memory/foreign proof remain open. |
| LLM and runtime evidence | 10 | 8 | Real float64 CPU warm measurements cover 128x128 and 512x512 with 20 samples per C++/facade route. Cold source runs use debug binaries and one sample per shape. General runtime/inference targets, optimized source comparisons, genuine paired model trials, human task review, scored isolation, and source parallel/GPU lowering remain open. |
| **Total** | **100** | **92** | Memory correctness and other first-release acceptance gates still block release. |

Five Wave 22 commits were integrated on `riley/wave22-review` from plan base `5815b4b`. Coordinator commit `c978c48` fixes Python record receiver collisions and notebook Unicode record transport, with regression tests, and refreshes the public compiler tree pin. Combined offline Rust tests, targeted post-fix emitter/kernel tests, the UI suite including 18 HTTP tests, 108 Python benchmark tests, six readiness reference checks, formatting, and workspace Clippy pass. A copied pinned Torch 2.11 CPU float32 source call passed numeric checks in 198.27 seconds; the executable still links libpython at startup, so Python-free source execution is an explicit Wave 23 gate. See [Wave 22 review](wave22/README.md) and [Wave 23 plan](wave23/README.md).
