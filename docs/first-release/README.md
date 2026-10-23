# CGx2 executable first-release guide

This guide describes the reviewed **0f0b0b9** preview and maps the complete
first-release contract. It does not declare CGx2 release-ready. Start with the
[command inventory](COMMANDS.md), [acceptance map](ACCEPTANCE.md),
[installation smoke](INSTALL.md), and [foreign routes](FOREIGN.md).
[inventory.json](inventory.json) maps every R01–R15 requirement and command to
[machine-readable examples](expectations.json). The [evidence](EVIDENCE.md)
states exactly what was run.

## First program and package

Source files end in `.cg`, notebooks in `.cgnb`, package manifests are
`cgx2.toml`, and the user-facing command is `cgx2`. Indentation is four spaces;
tabs are rejected. Local bindings are immutable unless declared `mut`.

From this directory's `fixtures` directory, with the reviewed tools on PATH:

```sh
cgx2 check scalar/answer.cg
cgx2 run scalar/answer.cg                       # 42
cgx2 run scalar/arguments.cg -- "hello docs"     # hello docs
cgx2 scalar/arguments.cg -- "hello docs"         # same script shorthand
cgx2 run scalar/loops.cg                        # 42
cgx2 run scalar/record.cg                       # 42
cgx2 fmt scalar/arguments.cg                    # formatted text on stdout
cgx2 api scalar/answer.cg --json
cgx2 check package
cgx2 run package                               # 42; needs sibling cgx2-native
```

[answer.cg](fixtures/scalar/answer.cg) uses explicit `i64` signatures,
`effects() caps()`, runtime `requires` and `ensures`. The loop example uses
checked ascending, end-exclusive `range(start, end)` and mandatory `else`.
The record example constructs an immutable nominal scalar record with named
fields. This does not enable variants, exhaustive matching, Option/Result,
generic applications, general list iteration, or tensor-valued expressions.
Those have named reservations awaiting reviewed S2 syntax.

The [three-module package](fixtures/package/cgx2.toml) lists exact relative
source files, an entry and a schema version. Imports name modules explicitly;
cross-module calls require public declarations and direct imports. Checked
transitive wrappers can declare empty effects/capabilities only when their
resolved bodies justify them. Do not infer package support from standalone
`check_source` library behavior or add ambient modules outside the manifest.
Schema-1 local packages are not a foreign-environment lock or registry.

## Memory rules to consult before writing code

The release design calls for ordinary default managed values, deterministic
owned resources, checked borrows, named auto/manual regions, explicit transfers
and fallible allocation. The current runnable source profile is an explicit,
bounded memory script/function profile; ordinary application aggregates and
general function lifetimes remain unfinished.

[manual.cg](fixtures/memory/manual.cg) shows the admitted operations:

```sh
cgx2 check memory/manual.cg
cgx2 run memory/manual.cg
cgx2 memory report memory/manual.cg --format=json
cgx2 run memory/no-gc.cg
```

`memory.owned_bytes(N)` creates owned bytes. `memory.promote_owned(region,
value)` consumes the owner on success; `memory.copy_to_owned(managed)` copies
without stealing a shared value. `memory.drop` releases/unroots the named value.
Borrow tokens from `memory.borrow`/`memory.borrow_mut` end explicitly with
`memory.end_borrow`. Handles cannot escape a closed region. The explicit manual
region does not collect until `memory.collect`; auto regions may collect at
allocation safepoints. Scope/failure cleanup and general application ownership
are different acceptance obligations.

`no_gc(budget: N)` suppresses all CGx2 collection in the execution session across
regions. Allocation charges every active nested budget before commitment; a
failed allocation does not run collection, enlarge the budget, or roll back
earlier mutations. Explicit collection in a guard is rejected (`CGO004`).
Allocation failures use `CGR013` in this preview rather than first-release
source `Result<_, AllocationError>` handling. One managed session is initially
single-threaded. These guarantees cover CGx2 memory only; Python, LibTorch and
device allocators are foreign and retain their own policies.

The checker executes admitted memory examples, including a native build. The
runtime owner corpus, counter parity, cancellation/unwind, leak-enabled host
checks and independent safety audit remain required; this small guide is not
that corpus. See [the normative memory rules](../language-design.md#memory-model-the-first-release-blocker)
and the [conditional memory profile](../../crates/cgx2-sema/CONDITIONAL-MEMORY.md).

## Effects, diagnostics and agent repair

Public effects and required capabilities must be explicit. Body-derived package
summaries check transitive effects and reject dishonest pure wrappers. A
declaration is not a general runtime resource grant. Broader enforceable file,
network and foreign capabilities are still release work; advisory security
findings cannot supply them.

```sh
cgx2 --diagnostic-format=json run negative/hidden-effect.cg
cgx2 --diagnostic-format=json run negative/unknown.cg
cgx2 --diagnostic-format=json run negative/contract.cg
```

These intentionally exit 1. JSON Lines on stderr have `code`, `severity`,
`message`, `file` and `span`. Spans count UTF-8 bytes, zero-based and half-open;
the Unicode comment fixture ensures byte offsets differ from character offsets.
The checker runs from a package-relative cwd and asserts exact file/code/span
tuples. Human diagnostics are for reading; parse JSON when repairing. Runtime
overflow (`CGR005`) and failed preconditions (`CGR015`) remain errors even after
successful static checking. `--diagnostic-format=json` after `--` is an ordinary
program argument.

For an agent: query the API, inspect the smallest source span, distinguish a
missing name (`CGT001`) from package/dependency resolution failures, fix the
declared boundary, then rerun the original command and the independent examples.
Do not invent an API because a library crate exists. A clean check or runtime
contract pass does not establish a formal proof.

## Native output, proofs and notebooks

The current native compiler is a companion executable:

```sh
cgx2c build scalar/loops.cg --out /absolute/new/program
/absolute/new/program
cgx2c emit-rust scalar/answer.cg --out /absolute/new/answer.rs
cgx2-verify scalar/proof.cg --domain x:0:2 --format json
cgx2-notebook run notebook/local.cgnb --out /absolute/new/executed.cgnb
cgx2-notebook show /absolute/new/executed.cgnb result
```

Use new outputs; compiler outputs do not overwrite inputs. Native builds require
compatible Rust 1.98.1, including for a copied installed `cgx2c`. Memory builds
embed the runtime sources from the compiler build. Rust and Python target
profiles are described in [COMMANDS.md](COMMANDS.md); Python emission currently
has a Rust library API, with no installed CLI target. Unsupported memory/foreign
features must reject translation rather than change semantics.

The proof example performs a finite source/core/SMT-term audit. With no trusted
live solver its `proof.kind` remains `unknown`, even when the finite verdict says
`proved`. Formal functional proof, dynamic contracts, and ownership/memory
safety are distinct. Solver timeout or unsupported constructs remain unknown.

The notebook example retains pure function definitions and uses bounded local
mutation/range loops. Saved outputs are cached provenance, not a fresh run.
Selected reruns and edits invalidate readers; clean run/restart starts fresh.
Earlier pure cells may be replayed under the preview's admission rules. External
effects are rejected (`CGNK011` in the included fixture). General persistent
effects, exactly-once selected dispatch, uncertain-outcome recovery and contract
parity await S3; a worker library alone does not implement them.

## Recheck the documentation

Use [the checker instructions](../../tools/docs-check/README.md), provide reviewed
tool hashes, and preserve the JSON report. Reserve future fixtures by name until
their owner supplies reviewed syntax; never turn a skipped example into a pass.
Every release row remains open until its complete acceptance evidence is reviewed.
