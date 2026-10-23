# Acyclic scalar package translation to Python

**Goal.** Make CGx2's existing explicit local modules transcribable into readable Python modules while preserving the checked scalar behavior and source-level API boundaries. This is source output, not a Python intermediary for direct LibTorch calls.

**Ownership.** Edit only `crates/cgx2-package/**` and `crates/cgx2-emit-python/**`. Read but do not edit syntax/sema/CLI, root Cargo files, or other lanes. Keep existing `emit_python(&Module, &TypedModule)` backward compatible.

## Deliverable

1. Add an API and/or crate-owned command that takes `load_checked_package(root)`, verifies unchanged manifest/source bytes before and after emission, and generates deterministic readable `.py` modules plus a runnable entry point for the existing acyclic public scalar-call subset. Preserve explicit import resolution, privacy, package entry choice, arguments, checked `i64` arithmetic, contracts, string/Bool output, error class, and source-located diagnostics where supported.
2. Produce a contained output tree without executing generated code during emission. Reject path traversal, symlink substitution, output collisions, unrepresentable Python identifiers, unsupported `memory`, foreign, tensor, `no_gc`, and imported nonscalar signatures with file and source span. Do not auto-import arbitrary Python libraries. If Wave 19 scalar `range` loops land concurrently, either support their exact semantics after integration or reject them clearly; do not assume that other worker's uncommitted feature exists.
3. Add positive two- and three-module fixtures with public/private calls, import order, argument forwarding, and checked arithmetic edges; execute emitted Python and compare observable output/exit with the current native package on the same inputs. Add cycle, stale source, bad import, and malicious path negative tests. Generated source should be understandable without a CGx2 runtime package when using only the supported scalar subset.
4. Document the explicit profile and what translation does not preserve (for example native runtime step limits, memory regions, Python/PyTorch foreign allocation). Generated edits do not round-trip to `.cg`.

## Verification and handoff

Run offline `cgx2-package` and `cgx2-emit-python` suites, Python syntax check and cross-target fixtures, formatter, and Clippy on owned packages. Commit only owned paths. Report SHA, file list, exact commands/results, generated example, and unsupported targets; stop for review.
