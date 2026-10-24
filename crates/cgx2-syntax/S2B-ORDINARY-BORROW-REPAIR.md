# OM04 nominal borrow repair

Base: `89464a76c9bd84a3b090ec58a14ca6be0043c6ea`.
Branch: `riley/ordinary-borrow-nominal-repair` in the attached `98b9/lang` worktree.
This completes the bounded nominal-resolution repair and its standalone evidence;
the broad ordinary-memory and release gates remain open.

## Behavior and ownership

`cgx2-sema/src/ordinary_memory/mod.rs` adds the private shared predicate
`region_borrow(module, name, args)`. Only an unshadowed `borrow` with exactly one
unnamed argument selects the region-capability shorthand. Local/qualified function
resolution and nominal record/variant declarations take precedence. Both rewriting
and `provenance.rs` use this predicate. Record arguments retain normal field/type
checking and existing fresh-allocation region analysis. No spelling is reserved,
no capability is manufactured, and no public interface or AST/schema changes.

The retained `repros/core-nominal/record-borrow.cg` now checks and returns 42.
The new `cgx2-sema/tests/ordinary_borrow_nominal_repair.rs` has 11 tests covering
local/qualified/imported constructors, inferred/explicit generics, nested heap
fields, existing local/imported functions, genuine region capabilities, exact
field-type and capability diagnostics, and generic named-region escape rejection.

Only the two assigned semantic source files, the new test, this report and
`ordinary-borrow-repair-evidence/**` are changed. Root manifests, historical
evidence, other lane code, and coordinator ledgers are untouched. Serena created
an unrelated untracked `.serena/` directory; it is excluded from the commit.

## Verification

All Cargo work used Rust **1.98.1**, `--offline --locked`, `CARGO_BUILD_JOBS=2`,
`CARGO_TARGET_DIR=$PWD/target/release-ordinary-borrow-nominal`,
`TMPDIR=$PWD/target/tmp-ordinary-borrow-nominal`,
`LIBRARY_PATH=/tmp/cgx2-wave22-lib`, and `PYTHONDONTWRITEBYTECODE=1`.

| Check | Result |
| --- | --- |
| All-feature sema/CLI/package/API-index tests | 425 passed, 0 failed |
| Same four packages, all-target/all-feature Clippy `-D warnings` | passed |
| `cargo fmt --all --check`, `git diff --check` | passed |
| Final `cargo build --workspace --all-features --offline --locked` | passed |
| New source/package parity matrix | 243 passed, 0 failed |
| Unchanged admission matrix | 196 passed, 0 failed |
| Unchanged ordinary-memory corpus, final attempt | 117 passed, 0 failed |
| Unchanged API matrix | 96 passed, 0 failed; see count explanation below |
| External/source/frozen dependency audit | 2,289 passed, 0 failed; 23 standalone programs |

The new matrix contains 14 valid inputs (8 sources, 6 packages) checked and run
through interpreter, native debug, native release and standalone Rust. Eleven
invalid inputs (8 sources, 3 packages) require exact original UTF-8 byte spans and
diagnostic sequences through check/run, and preserve rejected build/emission
artifacts. Valid Python rejections also preserve existing output. The unchanged
196 matrix retains OM01–OM03 and function-resolution coverage.

The API runner's assertions are unchanged. Its baseline 84 consists of 74
non-freeze assertions plus 10 hashes for five artifacts. This common freeze has
11 artifacts, so the identical runner checks 74 + 22 = 96 assertions. Only its
scratch environment is redirected by `replay_api.py`; fixtures and assertions are
unchanged, and the wrapper and original runner are preserved/hashed.

## Actual external standalone evidence

All Cargo checks/builds finished before `borrow-nominal-frozen-01` was created.
There were **no Cargo commands after freezing**. Seven final consumer binaries,
four canonical top-level probe libraries, and all needed dependency libraries and
metadata were copied and hashed; no stale hashed-library selection or mutable
probe-library symlink was used. The freeze manifest records original source and
toolchain hashes. Audit verifies these identities before and after verification.

The unchanged admission matrix was invoked with its actual `--output-dir` at:

```
/home/john-riley/.codex/visualizations/2026/10/24/01a16ea8-819b-7261-a64a-9bf5a9bb1b8f/borrow-nominal-admission-01
```

All nine valid source/package standalone compile/run pairs used that directory's
`outside/` as their resolved cwd. The new 14 standalone cases likewise used
`borrow-nominal-product-01/outside/` under the same private visualization root.
Both are physically outside every Git repository. `external-host.json` records
host-level ancestor inspection and `git rev-parse --show-toplevel` exit 128 after
removing inherited Git environment settings. This independently distinguishes
actual host directories from empty synthetic sandbox `.git` guard mounts.

The audit replays Rust dependency discovery for all 23 exact emitted sources in
their external cwd. Each depfile lists only its standalone `.rs` source. Original
compiler argv has no `--extern` or repository `-L` argument; the wrapper only caps
linker threads. `ldd` records show no CGx2/Python/LibTorch shared library dependency.
Source, compiler, executable and depfile SHA-256 values and resolved cwd are in
the audit; exact generated Rust and raw diagnostics are archived. Standard Rust
toolchain/OS libraries remain normal prerequisites. These are semantic execution
checks, not performance measurements.

## Attempts, failures, preservation

`ordinary-borrow-repair-evidence/checks/` retains bounded raw command records:
argv/cwd/environment, exit/reason/signal, stdout/stderr bytes/hashes and limits.
Development commands have 1,800-second wall / 1,200-second per-process CPU / 8 GiB
address-space / 1 GiB file / 8 MiB per-stream retained-output limits. Product and
audit children have 180-second wall / 120-second CPU / 4 GiB address-space / 512 MiB
file / 2 MiB per-stream limits. These are per-process bounds, not aggregate host
isolation. Product reports save incrementally through the unchanged parity engine.

Every failed attempt remains failed:

- Initial Rust invocation failed because the new test used nonexistent
  `Span::new`. Corrected baseline: 2 passed / 8 failed, including the retained
  nominal defect and test mistakes. First repair invocation failed Rust borrowing
  of `args`; next: 7 passed / 3 failed. Corrected tests: 10/0, then 11/0 in the
  complete 425-test run. Test corrections cover named-only record payloads,
  character rather than UTF-8 byte string length, supported single field access,
  and explicit `try_in` rather than assuming ordinary allocations become named
  region allocations. The first formatting failure is retained too.
- Ordinary-memory attempt 01: **116/1**, solely missing adjacent `frozen.json`.
  A compatible manifest of the same existing frozen hashes was then supplied.
  Attempt 02: **117/0**. No compiler change or runtime charge adjustment.
- Audit 01: **2,266/23**. All 23 failures were the audit incorrectly treating
  empty sandbox `.git` guard mounts as real repositories, despite Git discovery
  already returning 128. Host inspection proves the visualization root has no
  `.git` directory. Audit 02 preserves the guard observations, permits only empty
  guard directories, additionally requires Git exit 128, and passes **2,289/0**.
- The unnecessary `/tmp` fallback's setup assertion failed on an empty `/tmp/.git`
  guard directory. Its product/admission attempt 02 then hit real quota failures.
  Both remain incomplete/failed; admission retained a partial incremental report,
  and the new-product report was truncated during its quota-failed rewrite. Raw
  outer tracebacks and exact remaining partial bytes are preserved; unpersisted
  commands from the truncated tail cannot be reconstructed or counted as passes.
  `tmp-recovery.json` verifies every one of 45 files against the retained tar.gz
  before removing only this chat's exact private `/tmp` directory. No other
  worker's scratch was deleted. Sandbox startup and a shell heredoc also hit quota;
  administrative failures are recorded separately. Final packaging/read-only host
  verification used authorized escalation because sandbox startup was unavailable.

`exports.json` maps original hashes to gzip archive hashes; `verify.py` validates
both, raw stream lengths/hashes, frozen libraries/dependencies/source/toolchain,
the 45-file quota archive, and retained executable hashes. Final successful
attempts, earlier failed reports, exact generated sources, fixture inputs, depfiles
and wrappers are retained. Large executable/frozen artifacts remain at their
inventory paths in the private visualization root. No prior inside-checkout
historical attempt is relabeled as external acceptance.

To verify this delivery without Cargo:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 crates/cgx2-syntax/ordinary-borrow-repair-evidence/verify.py
```

## Remaining scope

This is OM04 nominal compatibility plus actual external standalone evidence.
Reserve-index 10/11 recovery, harness hardening, exact static dependency filename
attribution, general aggregate ownership/transfer, source `copy_owned`/`promote`
with same-owner failure, authenticated v3 summaries, and the full ordinary-memory
and R01–R15 release requirements remain open. No leak/GPU/live-solver/scored
isolation/performance/human/model evidence is claimed. Existing source path/span
boundaries outside the tested primary diagnostics are unchanged.
