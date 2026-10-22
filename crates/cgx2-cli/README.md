# CGx2 CLI execution preview

This crate provides a checked command-line preview for `.cg` scripts. The `check` command uses the semantic and ownership checkers; `run` checks before executing scalar or bounded memory scripts. The memory path calls the real `cgx2-runtime::Session`. This is not the native compiler or proof system. The public `cgx2_cli::run_source(source, args)` keeps the scalar interpreter's low-level behavior but checks every memory script before execution. `run_checked_source` checks either profile and returns all static diagnostics; `run_source_with_report` returns an actual runtime memory snapshot for memory scripts.

## Commands

From the workspace root:

```sh
cargo run -p cgx2-cli --offline -- check program.cg
cargo run -p cgx2-cli --offline -- run program.cg -- first "second argument"
cargo run -p cgx2-cli --offline -- program.cg -- first
cargo run -p cgx2-cli --offline -- fmt program.cg
cargo run -p cgx2-cli --offline -- check program.cg --diagnostic-format=json
cargo run -p cgx2-cli --offline -- memory report program.cg
cargo run -p cgx2-cli --offline -- memory report program.cg --format=json
```

`check` parses and checks names, scalar/list types, function calls and returns, mutability, explicit effects/capabilities, pure scalar contract expressions, and a bounded memory-source script profile through ownership operations. It does not prove contracts, resolve package dependencies, or check runtime memory budgets. A semantically valid `for` loop can pass `check` while the scalar interpreter rejects it with `CGR009`. `run` and `memory report` perform the static gate before any memory operation. `memory report` accepts only memory scripts and prints `Session::memory_report()` after successful execution, in versioned text or JSON. It never reports Python, PyTorch, or other foreign allocations as CGx2 memory. Failures exit nonzero without a result or report; `CGR013` is an allocation failure and `CGR014` is another runtime memory error.

`--` separates program arguments from CLI arguments; `arg(0)` reads the first argument after it. `--diagnostic-format=json` may appear before the `--` delimiter on `check` or `run`; after the delimiter it is a program argument. `fmt` writes canonical source to stdout and does not change the file. A successful run writes the final non-`Unit` value and one newline to stdout. Errors go to stderr and exit nonzero without a success value. Parser diagnostics retain `CGS` codes, semantic diagnostics use `CGT` codes, and runtime diagnostics use `CGR` codes. Human and JSON Lines diagnostics share the same message and source location. Each JSON line has `code`, `severity`, `message`, `file`, and `span` fields; source spans are zero-based, half-open UTF-8 byte offsets. File and command errors without a source span use `null`.

## Executable subset

- A script runs its top-level statements in order. A function module calls exactly one zero-argument `main`. Missing or duplicate `main` definitions are errors.
- Values are `i64`, `Bool`, `String`, and `Unit`. Supported expressions are literals, local names, calls, `+`, `-`, `*`, `/` on `i64`, and scalar comparisons. Integer literals may contain `_`. Arithmetic is checked; division by zero and overflow are errors. Strings support `\n`, `\r`, `\t`, `\\`, and `\"` escapes.
- `let`, `let mut`, `=`, `+=`, expression statements, and function `return` work. The last expression in a script or function body supplies its value; other final statements yield `Unit`. Function calls have local parameter and binding scopes. Annotations may name only the four scalar types and are checked at runtime; unannotated mutable bindings also retain their initial value type. `arg_count()` returns the number of program arguments; `arg(i64)` returns an argument or errors on a negative or missing index. Those names are reserved.
- Execution stops after 1,000,000 statements or expressions, or at a call depth of 64. The depth guard also protects the host stack from recursive programs.

For scalar functions, `requires` is evaluated before the body and `ensures` after a successful return, with `result` bound to that return value. Both checks share the existing 1,000,000-step and 64-call-depth limits. Failure returns `CGR015` or `CGR016` at the contract expression span, exits nonzero, and prints no success value. Contracts are runtime-checked, never proved. Memory values, imports, and recursive contract calls are unsupported. `for` loops and generic/non-scalar types remain unsupported by the scalar interpreter. It does not provide native performance.

## Explicit Python-only source calls

```cg
foreign python pub fn isqrt(x: i64) -> i64 effects(Foreign) caps(ForeignCall) = math.isqrt runtime "local:cpython" ownership copied

fn main() -> i64 effects(Foreign) caps(ForeignCall):
    requires true
    ensures result == 9
    isqrt(81)
```

Save this as `example.cg`, then run `cgx2 check example.cg` and `cgx2 run example.cg`. The declaration names an exact CPython module and callable, a copied scalar signature, a `Foreign` effect, a `ForeignCall` capability, and the explicit `local:cpython` runtime identity. This Wave 10 source preview accepts public `math` callables and `builtins.str`/`builtins.bool` only; broader targets need reviewed effect and capability contracts. That identity uses the locally linked CPython installation; it does not pin a package version or make the result reproducible across hosts. No Python import occurs during `check`. The caller must explicitly permit the foreign effect and capability. The host validates types and arity before import. `requires` in `main` runs before the foreign call; `ensures` runs after a successful typed return.

Only `i64`, `Bool`, and `String` parameters and results with `ownership copied` are supported. Borrowed/mutable buffers, handles, keyword arguments, callbacks, other Python targets, and `torch` on this route are rejected. A foreign `main` is also rejected; use a local `main` that calls a binding. Callable lookup, exceptions, and return conversion produce `CGR019`–`CGR021` errors at the `.cg` call span, including JSON diagnostics. Python owns its allocations; they are outside CGx2 region and `no_gc` accounting. This is the Python-only CPython route, separate from the direct native LibTorch route. Native and Python source emitters have not been adapted for this AST marker and must reject it during integration.

## Bounded memory scripts

Memory scripts use only top-level `let` bindings, direct reserved `memory.*` calls, direct value-use statements, and nested `no_gc(budget: N)` blocks. `memory.region(mode: auto|manual, limit: N)` opens a region; auto mode starts with a 1 MiB committed-byte trigger checked at allocation safepoints, while manual mode collects only on `memory.collect(region)`. `memory.owned_bytes(N)` allocates a unique owned buffer; `memory.move(value)` transfers it without cloning. `memory.alloc_bytes(region, N)` allocates and roots a managed handle until `memory.drop(value)` unroots it. `let m = memory.promote_owned(region, owned)` moves the owned buffer into that open region without copying its payload, consumes `owned` on success, and roots `m` until `memory.drop(m)`. `let copy = memory.copy_to_owned(m)` allocates distinct owned storage; `m` remains live and can be dropped independently. Both require direct named handles. `memory.borrow(value)` and `memory.borrow_mut(value)` create statically checked tokens ended by `memory.end_borrow(token)`. `memory.use(value)` checks liveness, and `memory.close(region)` closes a region after all handles are dropped. An explicit collection inside any `no_gc` block is a checker error. All active guard budgets charge CGx2 allocations across all regions, including runtime overhead; exhausting one returns `CGR013` without collection. The region's runtime limit is also enforced.

The runtime charges promotion's destination region for the payload's reserved capacity plus 64 bytes of slot metadata. If promotion happens inside `no_gc`, only the 64 new metadata bytes are charged to each active guard because the owned payload was charged when allocated. A managed-to-owned copy charges its actual reserved byte capacity plus 32 bytes of owned metadata to each active guard, with no new region charge. A failed promotion returns its original owned buffer to the evaluator binding; execution then stops with no success report and no second owner. These logical charges exclude foreign allocations. The source checker rejects assignments, functions, control flow, returns, dynamic sizes, aliases, and unsupported operations in memory scripts. The evaluator has no implicit cross-region transfer, foreign memory, or scope cleanup. Memory bindings must be explicitly dropped, ended, or closed; an execution error discards that session without printing a success report. This is a bounded profile rather than a whole-language memory-safety proof.
