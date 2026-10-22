# CGx2 language and toolchain specification

**Status:** design proposal with an integrated scalar preview and standalone memory, notebook, Python, and verification components. Most first-release paths are incomplete. CGx2 uses `.cg` source files and `.cgnb` notebooks. Examples below define intended behavior, but the exact grammar is provisional until the syntax comparison in the validation plan.

## Purpose and boundaries

CGx2 should make memory behavior controllable without forcing ownership syntax on ordinary code. A person and an LLM write the same readable source. The compiler exposes additional structured information rather than a second machine-only syntax. The first usable release is a broad, integrated milestone; internal previews may implement subsets first. Long-term breadth comparable to the OCaml compiler and surrounding platform requires years of ecosystem work and is a separate maturity goal.

The supplied research identifies five classes of LLM coding failures: syntax/compilation, runtime, behavior, integration, and long-term context. CGx2's type and memory checks address some failures; contracts, tests, dependency validation, security analysis, and independent benchmark cases address others. A successful compile is never treated as proof of task correctness.

## Source language

The **starting candidate** uses indentation to delimit blocks and newlines to end statements. There are no routine braces or semicolons. Four spaces is the canonical formatter output; tabs are rejected for indentation so source spans and diffs are stable. Parentheses allow multiline expressions. This choice must beat or match an equivalent brace candidate on verified LLM tasks and human comprehension before the grammar is frozen.

~~~cg
module app.math

pub fn double(x: i64) -> i64:
    x * 2

pub fn total(xs: List<i64>) -> i64:
    let mut answer = 0
    for x in xs:
        answer += x
    answer
~~~

Local values are immutable unless declared `mut`; their types are inferred. Public function parameters, returns, effects, and required capabilities are explicit, so the public API index does not depend on reading function bodies. Nominal records and variants, `Option`, `Result`, generics with named constraints, exhaustive matching, and checked integer arithmetic belong in the first usable release. Numeric and tensor shape refinements can be added when their semantics and diagnostics are tested; they are not implied by ordinary type annotations. One canonical formatter prevents style variants from becoming separate dialects.

A module declares its path and imports names explicitly. The package manifest `cgx2.toml` pins dependencies, target capabilities, entry point, Python environment when used, and compiler/runtime versions. The module graph and API index expose imports, exports, types, effects, contract summaries, and source locations. Cycles are rejected unless an explicitly specified future module feature handles them.

## Memory model: the first release blocker

### Values and regions

Ordinary heap allocations use a default **managed** region. The runtime traces reachable CGx2 objects and reclaims unreachable objects. Managed handles remain valid across collection; raw pointers and implicit escape into foreign runtimes are unavailable in safe CGx2. Managed cleanup is nondeterministic, so file handles, locks, and similar resources use explicit owned types with deterministic release.

A named region has `mode: auto` or `mode: manual`, a byte limit on committed CGx2 heap bytes, and a lifetime. Unreachable objects continue to count against the limit until a collection reclaims them. `auto` may collect at documented allocation safepoints when its threshold is reached; the program may also call `memory.collect(region)`. `manual` collects only on an explicit call. Both modes collect only that region; collection of one region cannot invalidate a handle in another. Regions do not share live objects implicitly. Cross-region transfer is an explicit checked copy or move. Closing a region releases its remaining storage deterministically, and the checker prevents closing it while a live handle could escape; region close is distinct from a collector cycle.

An `owned<T>` value has one owner and is released when its scope ends. It can be borrowed read-only or exclusively mutable without transferring ownership. The compiler rejects use after move, conflicting borrows, and escaping borrows. Converting owned to managed requires an explicit operation that names the destination region. Converting managed to owned requires a checked unique transfer or an explicit copy; it cannot silently steal a shared object. These operations state whether a copy, allocation, or invalidation occurs. The first runtime can use a nonmoving, per-region tracing collector and stable handles; the compiler emits safe Rust for ordinary code while the runtime's internals receive a separate safety audit. Collection strategy may change only if these source guarantees remain intact.

### Choosing when collection runs

A `no_gc(budget: N)` block suppresses **all CGx2 collection in the current managed execution session** until the block exits, including collection requested for another region. The first usable managed runtime has one execution thread per session, so another CGx2 thread cannot pause it with collection. `N` is the maximum cumulative CGx2 heap bytes allocated while the block is active, across all regions and including runtime overhead. Every allocation is charged before commitment; exceeding `N` or a region limit returns `AllocationError` through a `Result` without making that allocation or running collection. Earlier successful mutations are not rolled back. An explicit `memory.collect(region)` inside the block is rejected. Nested blocks charge each active budget, so the first exhausted limit fails. Auto regions may collect after exit at their next safepoint. Manual regions never collect on exit unless code asks them to.

The following is illustrative syntax, not a frozen grammar:

~~~cg
let scratch = memory.region(mode: manual, limit: 64_MiB)
let value = no_gc(budget: 1_MiB):
    build_index(scratch)?
memory.collect(scratch)
~~~

The checker records allocation and collection effects. It rejects impossible static budgets when provable, and the runtime enforces all remaining limits. `cgx2 memory report` should show per-region live bytes, peak bytes, allocation failures, collection count, and pause distribution. Safe code must never have a use-after-free, double-free, or out-of-bounds access. The release gate tests these invariants and zero CGx2 collections anywhere in a `no_gc` section, including error paths and nested sections.

These guarantees cover **CGx2-owned memory**. Python, PyTorch, C libraries, and devices keep their own allocation and collection systems. A `no_gc` section does not promise to stop Python GC or a PyTorch GPU allocation. Foreign calls carry an explicit effect and memory-boundary description. Memory reports show foreign handle counts and best-effort external memory estimates when the host API exposes them, but do not charge that memory to CGx2 region budgets.

## Effects, capabilities, contracts, and proofs

A function's public signature declares observable effects such as file access, network access, allocation, collection, or foreign execution. A capability value authorizes operations such as reading a file or opening a socket. Pure functions cannot acquire such effects through a hidden call. Contracts use `requires`, `ensures`, assertions, and loop invariants; the compiler reports whether a property is checked statically, checked at runtime, or assumed.

The first usable release has a bounded formal verifier for pure integer/Boolean functions, checked arithmetic, array bounds, ownership/region rules, and annotated loops/contracts. Unsupported constructs are diagnosed as outside the proof subset. Verification conditions go to an SMT solver; a proved result requires the negated condition to be unsatisfiable under the modeled semantics. A counterexample is `disproved`; timeout or unsupported reasoning is `unknown`, never `proved`. Python/PyTorch calls and unmodeled effects require explicit assumptions or runtime checks. Memory safety established by the core type/ownership rules and dynamic region checks is documented separately from SMT-proved functional properties.

The security analyzer checks declared effect/capability policy and a limited, specified taint source-to-sink set. The performance analyzer reports measured runtime, allocations, GC pauses, and user-declared budget violations; static cost hints are advisory unless a contract is mechanically enforced. Tests, property tests, fuzzing, and cross-target differential tests complement proofs.

### Automatic execution choices

CGx2 should aim to run supported work efficiently by default. A future execution planner may choose sequential CPU, parallel CPU, or a GPU-backed implementation only where it can preserve the program's declared effects, ownership, memory guarantees, and result contract. `auto` is the default policy; explicit `cpu`, `parallel`, and `gpu` requests are available for measurement and reproducibility. An unsupported explicit request is an error. `auto` falls back to a correct sequential implementation when capabilities or reliable measurements are missing.

Parallel execution requires an analysis that proves independent work or a library primitive with a reviewed parallel contract. Side-effect order, data races, borrow lifetimes, and allocation behavior cannot change silently. One managed runtime session remains single-threaded initially; sharing its region handles across workers requires a separately specified thread-safe runtime. A `no_gc` guard cannot be bypassed by a parallel worker that shares the session. GPU dispatch is limited to a backend that actually supports the operation and device; host/device transfer, kernel launch, synchronization, memory pressure, and numerical differences count in the decision. Foreign PyTorch GPU allocation stays under PyTorch's manager and is reported as foreign memory. The compiler cannot guarantee globally optimal execution because input sizes, devices, contention, and transfer costs vary at runtime; it can compare safe candidates under measured conditions and expose the chosen plan and reason. General automatic GPU kernel generation is a later platform track.

## Compiler, CLI, and LLM interface

The compiler pipeline is source with byte spans → parser → resolved module graph → typed/effect/ownership IR → memory and contract checks → target lowering → Rust or Python source → executable when applicable. The typed IR defines CGx2 semantics; backends cannot reinterpret overflow, errors, ownership transitions, or public APIs. Each pass preserves source maps.

The first usable CLI includes:

| Command | Behavior |
| --- | --- |
| `cgx2 check PATH` | Parse, resolve, type/effect/memory check without target compilation. |
| `cgx2 build PATH` | Emit and compile a native program. |
| `cgx2 run FILE.cg -- ARGS` / `cgx2 FILE.cg -- ARGS` | Run a standalone script and pass program arguments; nonzero exit on uncaught error. |
| `cgx2 test PATH` | Discover and run tests, contracts, and declared properties. |
| `cgx2 verify PATH` | Run bounded proofs and report proved/disproved/unknown. |
| `cgx2 emit PATH --target rust|python` | Emit source when all used features are supported by that target. |
| `cgx2 fmt PATH` / `cgx2 api --json PATH` | Canonical formatting and machine-readable public symbols. |
| `cgx2 notebook open|run FILE.cgnb` | Open the native notebook interface or execute cells from a clean session. |

The default diagnostic display is human-readable. `--diagnostic-format=json` emits JSON Lines with a stable code, severity, message, package-relative file, zero-based half-open UTF-8 byte span, related declarations, violated rule/contract, and optional suggested fix. Proof and test results also have structured output. An agent can query an API before inventing a call and request the smallest source context needed to repair an error. Tooling must distinguish a missing symbol from a missing dependency and a proved result from a runtime test pass.

A single-file script needs no manifest; its imports resolve from the current project or pinned environment. A package uses `cgx2.toml`. The CLI and notebook use the same parser, checker, types, contracts, and runtime semantics.

## CGx2 notebooks

A `.cgnb` file is readable, versioned UTF-8 text with stable cell IDs and code or Markdown cells. Code cells use ordinary CGx2 syntax. A native CGx2 notebook interface keeps a persistent session, executes one cell without replaying earlier side effects, supports cell rerun and **Restart and Run All**, and marks dependent outputs stale after a changed definition. Static symbol dependencies drive invalidation; uncertain side effects conservatively mark later cells stale. Clean execution starts a new process and runs cells in file order.

The proposed text format starts with `#!cgnb 1` and column-zero headers such as `%% code id=load` and `%% markdown id=notes`. A literal leading `%%` in cell text is escaped. Text outputs and execution metadata are saved in the notebook. Large images and other binary outputs live in a content-addressed sidecar referenced by the notebook; export can bundle them into one self-contained file. The viewer supports displayed values, tables, plots, stdout/stderr, and errors, with sanitization for rich HTML. Saved outputs are cache/provenance, not evidence of a fresh run. The exact serialization grammar will be fixture-tested before implementation.

A standard Jupyter kernel or `.ipynb` import/export may be added later; neither replaces the native format. Persistent cell state requires an incremental evaluator/compiler path in addition to the whole-program build path.

## Interoperability and source emission

Native Rust output embeds CPython through a reviewed PyO3 bridge when Python imports are used. A pinned interpreter/environment must contain the requested package. The initial typed `Tensor` layer covers common PyTorch tensor creation, shape/dtype/device inspection, selected math, inference, and conversion; a dynamic `PythonValue` call path reaches unwrapped Python APIs with runtime validation and clear exception translation. The binding states thread attachment, handle lifetime, aliasing, ownership transfer, CPU/GPU copy behavior, and whether zero-copy exchange is supported for each operation. PyTorch tensors remain owned by Python/PyTorch; CGx2 only owns its handle and any CGx2 buffers. The first reference app is a CLI inference/data workflow using installed PyTorch.

Other libraries can be reached through declared C ABI adapters, generated wrappers, or a process/RPC protocol where values can be serialized. An arbitrary library is not automatically type-safe or zero-cost. Rust crate and Node/browser adapters are later ecosystem tracks; this first release must at least provide the generic declared C/process path and the concrete Python bridge.

Rust source emission is the full native path, including runtime calls for managed memory. Python emission targets a documented portable subset of pure/managed application logic and selected typed PyTorch calls. Explicit CGx2 collection regions, `no_gc`, owned borrows, and unsupported effects cause a clear target error instead of quietly changing semantics. Generated Python may use a small CGx2 compatibility library for checked arithmetic and contracts. Both emitters target behaviorally equivalent, readable, editable output within their supported profiles; generated edits do not round-trip into CGx2. CGx2 remains the source of truth. TypeScript is the next source target after Rust and Python. Cross-target fixtures compare outputs, errors, and numeric boundaries. Library access through a bridge is separate from emitting source in that library's language.

## Release contract and longer-term platform

The **first usable release** requires a native CLI; safe managed/owned/manual memory modes; `no_gc` guarantees; modules and canonical syntax; structured diagnostics/API tooling; types, effects, capabilities, contracts, tests and bounded proofs; Python/PyTorch access; native CGx2 notebooks; Rust and Python source output for their stated profiles; and security/performance analysis. Features can arrive in internal previews, but the release label waits for all required capabilities and the validation gates.

The **long-term platform goal** adds the breadth associated with a mature language ecosystem: a standard library, package manager and registry, debugger, REPL, language server, formatter, profiler, documentation, stable ABI, multiple backends, migration tools, and sustained compatibility. Browser UI, distributed execution, GPU kernels, tensor shape proofs, and broad automatic bindings are separate later tracks. Their absence from the first usable release must be stated rather than hidden behind the phrase “universal language.”
