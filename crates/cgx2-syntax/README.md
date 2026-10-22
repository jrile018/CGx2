# CGx2 syntax candidate

This standalone Rust crate parses the **provisional indentation candidate** for `.cg` source and prints one canonical four-space style. The syntax comparison in `docs/validation-plan.md` has not happened; this crate is not evidence that indentation beats braces.

```rust
use cgx2_syntax::{format_module, parse_module};

let module = parse_module("pub fn double(x: i64) -> i64:\n    x * 2\n").unwrap();
let canonical = format_module(&module);
```

Run this crate alone with `cargo test --manifest-path crates/cgx2-syntax/Cargo.toml --offline`.

## Supported slice

```cg
pub fn clamp_nonnegative(x: i64) -> i64 effects() caps():
    requires x >= 0
    ensures result >= x
    x
```

The clauses appear between the return type and `:`, and contract lines appear before body statements. `effects()` and `caps()` are explicit empty sets. `requires` runs before the body; `ensures` sees the returned `result`.

A Python-only binding is a distinct function item, for example `foreign python fn isqrt(x: i64) -> i64 effects(Foreign) caps(ForeignCall) = math.isqrt runtime "local:cpython" ownership copied`. `Function.foreign` preserves the target, route, local runtime identity, and copied ownership for the checker and for later emitter rejection. The declaration has no body. This syntax does not enable implicit imports or the direct LibTorch route.

- Optional `module app.math` declaration, followed by zero or more unique top-level `import app.base` declarations, then public/private `fn` items with typed parameters and optional return type, or a top-level script. Imports after executable items and nested imports are rejected. This candidate rejects mixing function items and top-level script statements because the current `Module` AST stores them separately and would otherwise lose source order.
- Indentation-delimited `for` blocks, `let`/`let mut`, simple type annotations, `=` and `+=` assignment, `return`, and expression statements. A final expression remains an ordinary expression statement; later semantic analysis may give it tail-value meaning.
- Dotted names, generic type syntax such as `List<i64>`, integer and quoted string literals, Booleans, calls, parentheses, and binary `+`, `-`, `*`, `/` with arithmetic precedence. Calls may use named arguments such as `memory.region(mode: manual, limit: 4096)`; positional arguments may precede named ones, but a positional argument after a named one is rejected. Duplicate names are rejected. Types are syntax trees, not resolved types.
- Structured `no_gc(budget: 256):` blocks, including nested blocks. The AST uses `StmtKind::NoGc { budget, body }`; call arguments use `CallArg { name, value, span }`. Every span remains a half-open UTF-8 byte range.
- Function headers may append `effects(ReadArgs, ManagedAlloc, Collect, Mutate, Foreign, Unknown)` and `caps(ArgsRead, MemoryCollect, ForeignCall)` after the return type. Either list may be empty; omission means inferred. Unknown or duplicate names and repeated list clauses receive `CGS110`. Before executable statements, an indented prelude may contain `requires EXPR` and `ensures EXPR` lines. The formatter emits all `requires` lines before `ensures` lines. Scalar comparisons `==`, `!=`, `<`, `<=`, `>`, and `>=` bind below arithmetic.
- Blank lines and comment-only lines have no indentation effect. Parentheses permit multiline expressions. Leading tabs are rejected, including on continuation lines. CRLF and Unicode names retain byte-accurate locations.

The formatter emits four spaces per block level, spaces around binary and assignment operators, import lines in source order, a blank line between the declaration/import header and executable items and between functions, and a final newline. Comments and extra blank lines are not retained. `format_module` accepts a successfully parsed AST; formatting and reparsing preserve the AST structure apart from source spans.

The grammar remains incomplete. Glob, alias, and relative imports, records, variants, matching, postfix `?`, floating-point literals, and other future forms receive syntax diagnostics rather than implied semantics. Exact rules for those forms and whether blocks remain indentation-based await design and benchmark decisions.

## Public API and diagnostics

`lex(&str)` returns `Vec<Token>` or lexing diagnostics. `parse_module(&str)` returns `Module` or `Vec<SyntaxDiagnostic>`. `format_module(&Module)` returns canonical source. `Module` exposes `name`, `imports`, `functions`, `statements`, and `span`; `Function`, `Stmt`, `Expr`, `TypeSyntax`, and `Path` expose syntax and spans for a later compiler adapter. Each `Span { start, end }` is zero-based, half-open UTF-8 byte offsets in the original input. A `SyntaxDiagnostic` contains `code`, `message`, and `span`. The first-slice codes are:

| Code | Meaning |
| --- | --- |
| `CGS001` | Tab in leading indentation. |
| `CGS002` | Dedent does not match a previous indentation level. |
| `CGS003` | Unterminated string literal. |
| `CGS004` | Unexpected character. |
| `CGS005` | Unclosed parenthesis. |
| `CGS101` | Missing block colon. |
| `CGS102` | Unexpected indentation boundary or block end. |
| `CGS103` | Expected token/name/expression or invalid statement form. |
| `CGS104` | Missing indented block. |
| `CGS105` | Function items mixed with top-level script statements. |
| `CGS106` | Duplicate named call argument or invalid `no_gc` label. |
| `CGS107` | Positional call argument after a named argument. |
| `CGS108` | Duplicate import declaration. |
| `CGS109` | Import after a function or script statement. |
| `CGS110` | Unknown or duplicate effect/capability declaration. |

These codes are stable for this slice. Future compiler integration should consume the exported AST and diagnostics directly; it may need a source-order item representation if mixed module/script files become valid.


The parser and formatter retain direct local calls such as `build()` inside `no_gc` blocks in function bodies. The checked memory-function subset (zero arguments and `Unit` result) is imposed by `cgx2-sema`, not by the grammar.

## Checked object-reference spelling

The parser and formatter preserve direct calls `memory.alloc_object(region, payload_bytes, reference_slots)`, `memory.set_reference(owner, index, target)`, `memory.clear_reference(owner, index)`, `let found = memory.reference(owner, index)`, and `memory.use_reference(found)`. These are ordinary call expressions in the AST; `cgx2-sema` enforces the bounded profile. `payload_bytes`, `reference_slots`, and `index` must be nonnegative integer literals in that profile. No record, pointer, optional-value expression, or general heap syntax is added.

## Direct CPU tensor source spelling

The bounded direct tensor call uses the existing `unsafe foreign native` declaration and a single call in `main`. Its two inputs are quoted, one-dimensional float64 vector literals such as `"[1.0, 2.0]"`. This intentionally keeps vector syntax inside the quoted literal for this preview; the checker accepts only finite decimal or exponent float64 elements, 1–1024 values per vector, equal lengths, and no nested arrays. The parser and formatter preserve that one call and quoted literal form. See `cgx2-cli/tests/fixtures/tensor-cpu/src/main.cg` for the exact source spelling. There is no general `Tensor` value or numeric tower in this grammar.
