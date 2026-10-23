# cgx2-sema

`cgx2-sema` checks the `cgx2_syntax::Module` AST. Parse first with `cgx2_syntax::parse_module`, then call `check_module(&module)`. Success returns a `TypedModule`; failure returns all collected `TypeDiagnostic` values. Each diagnostic has a stable `code`, a `message`, and a zero-based, half-open UTF-8 byte `span` into the original source.

For static local packages, call `collect_public_signatures(&module)` on each parsed source before checking bodies. Put fully qualified `module.function` signatures, listed declared modules, and private function names into `ImportedSignatures`, then call `check_module_with_imports(&module, &environment)`. Only explicit `module.imports` grant access. Imported calls accept `i64`, `Bool`, `String`, and `Unit` signatures; `List<T>` imported calls receive `CGT010`. The standalone `check_module` rejects any source import with `CGT011`, so existing CLI and native build paths cannot silently ignore imports. No imported body is executed or lowered.

`TypedModule.functions` is a `BTreeMap<String, FunctionSignature>` keyed by function name. It includes the reserved preview builtins `arg_count() -> i64` and `arg(i64) -> String`, which have synthetic `Span { start: 0, end: 0 }`. `FunctionSignature` exposes `public`, ordered `parameters`, `return_type`, and declaration `span`. Private functions without a return annotation infer their return type from the final statement. Unannotated recursive return inference is unsupported; annotate those functions. `TypedModule.expressions` is a `SpanTypes` index: `get(span)` retrieves an expression's `Type`, and `iter()` yields `(Span, &Type)` pairs in byte-offset order. Failed expressions have no entry.

The checked subset has `i64`, `Bool`, `String`, `Unit`, and nested `List<T>` types; local `let` inference and annotations; lexical scopes in `for` bodies; immutable and mutable assignment; `i64` arithmetic and scalar comparisons; calls; explicit returns; and tail-expression returns. A `for` iterator must be `List<T>`, and its binding has type `T`. Public functions require an explicit return annotation. Integer literals must fit signed `i64`; arithmetic overflow remains a runtime error. The checker recognizes only the two argument builtins above.

`TypedModule.effects` records conservative local-call closure for each function. `effects()` means explicitly pure; omitted effects are inferred for legacy source. `caps()` means no capabilities; omitted capabilities are inferred. `arg` and `arg_count` require `ReadArgs` plus `ArgsRead`; assignment contributes `Mutate`. Declared local Python calls require explicit `Foreign` and `ForeignCall` on the binding and each caller. The only source foreign signature types are copied `i64`, `Bool`, and `String`; runtime identity is explicitly `local:cpython`. `torch` and dynamic evaluation/import targets are rejected. Known `memory.collect` and other foreign-qualified calls are classified but remain outside scalar function execution. Imported or unresolved calls contribute `Unknown` in explicit signatures and cannot pass as pure. Declarations do not grant capabilities. Contracts must be pure Boolean expressions over `i64`/`Bool` parameters and an annotated scalar return; `result` is available only in `ensures`. Effectful, unknown, or recursive contract calls are rejected. No contract is labeled proved.

| Code | Meaning |
| --- | --- |
| `CGT001` | Unknown or duplicate name |
| `CGT002` | Unknown or unsupported type |
| `CGT003` | Incompatible types, including arithmetic or call arguments |
| `CGT004` | Assignment to an immutable binding |
| `CGT005` | Wrong call arity |
| `CGT006` | Missing public return annotation |
| `CGT007` | Integer literal outside signed `i64` |
| `CGT008` | Qualified call without an explicit import |
| `CGT009` | Imported function is private |
| `CGT010` | Imported function has a non-scalar signature |
| `CGT011` | Standalone check cannot execute/import package modules |
| `CGT012` | Effect outside an explicit declaration or unknown call |
| `CGT013` | Required capability omitted from an explicit declaration |
| `CGT014` | Invalid or effectful contract |
| `CGT015` | Effect analysis bound exhausted |

This crate does not execute code or check runtime overflow or memory limits. Function values and qualified names are outside the scalar preview. The syntax parser has no list literal, so list iteration is checked from typed parameters or other future expressions.

## Bounded memory-source profile

For a standalone script containing `memory.*` or `no_gc`, `check_module` uses a conservative source lowerer in `src/memory.rs` and invokes `cgx2_ownership::check_ops`. The reserved `memory` namespace accepts `memory.region(mode: auto|manual, limit: N)`, `memory.owned_bytes(N)`, `memory.move(owned)`, `memory.alloc_bytes(region, N)`, `memory.promote_owned(region, owned)`, `memory.copy_to_owned(managed)`, `memory.borrow(value)`, `memory.borrow_mut(value)`, and statement calls `memory.use(value)`, `memory.drop(value)`, `memory.end_borrow(token)`, `memory.collect(region)`, and `memory.close(region)`. Promotion consumes its live, unborrowed owned source and creates a managed result in an open region; copying creates a distinct owned result and leaves the managed source live. Both conversions require direct named handles in a `let` binding. `N` and `no_gc(budget: N)` require nonnegative integer literal bytes that fit `usize`; this is syntax/static validation only, and runtime limit enforcement is still required. Region, owned, managed, and borrow names have distinct source kinds and cannot be passed interchangeably. All memory bindings remain tracked until an explicit drop/end/close; scope cleanup is not modeled yet.

The lowerer emits exact source spans and preserves ownership diagnostic codes `CGO001`–`CGO005`. `CGM001` reports constructs the source lowerer does not model, including dynamic calls, aliases, assignment, control flow, and returns in a memory script. `CGM002` reports wrong or missing intrinsic arguments, handle kinds, and nonliteral/overflowing byte counts. Ordinary scalar checking retains its `CGT` diagnostics; named arguments in scalar calls are currently rejected with `CGT005`. A successful memory script returns an empty `TypedModule` summary. This profile does not execute memory operations or prove budgets safe at runtime.


## Bounded memory functions
A memory function module requires zero-argument `main()`, explicit `-> Unit`, declared effects, and `caps(MemoryCollect)` wherever collection is required. Other direct acyclic local functions may declare distinct `i64` parameters. A parameter may be forwarded in a positional call or used as the byte-size argument of `memory.alloc_bytes(region, size)` or `memory.owned_bytes(size)`. `no_gc` budgets, region limits, object shapes, slots, and indices remain literal-only. The checker validates call arity/types, keeps handle bindings private, rejects recursion and capture, and propagates effects/capabilities through calls. Local regions must close before return. `CGM001` marks unsupported function forms; `CGM002` marks invalid scalar arguments or byte-size uses; `CGO004` rejects collection through a call inside `no_gc`. Runtime budgets and limits remain dynamic checks.

## Bounded managed object references

`memory.alloc_object(region, payload_bytes, reference_slots)` creates a rooted local object handle. The payload is at most 1,048,576 bytes and the number of reference slots at most 256, matching the frozen Wave 12 runtime limits. Arguments are direct names or nonnegative integer literals. `memory.set_reference(owner, index, target)` and `memory.clear_reference(owner, index)` require an object owner and a literal index below that owner's declared slot count. Targets must be object handles in the same region. `let found = memory.reference(owner, index)` creates a local optional reference; only `memory.use_reference(found)` may consume it. An empty or stale reference is checked dynamically. Object handles are rooted until `memory.drop(handle)`; links keep targets reachable after their local roots are dropped. `memory.close(region)` still requires all rooted handles dropped. Collection may reclaim unrooted cycles.

The checker rejects wrong handle kinds, duplicate or aliased bindings, dynamic shapes and indices, cross-region edges, dropped handles, use of an optional reference after region close, and attempts to pass handles through calls, return them, store them globally, or use them in unsupported control flow. It does not claim typed records, concurrent collection, raw pointers, foreign tensor ownership, or native graph compilation. `CGM001` covers unsupported source forms, `CGM002` covers shape, index, kind, and region errors, and existing `CGO` codes cover ownership and `no_gc` violations.
