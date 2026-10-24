# OM08 ordinary-memory public API repair

Review base: `7bb7a480119826693db67ed503caaf7f23f6abb2`.
Branch: `riley/ordinary-api-visibility-repair`, attached private worktree.
This bounded repair does not accept full ordinary memory, the release, or a
subsequent implementation milestone.

## Result and interface

New `cgx2_api_index::build_public_memory_index_v1(&MemoryProgramV1)` returns
`ApiPublicMemoryIndexV1`, with `render_json() -> String` and
`render_text() -> String`. It authenticates the entire program before parsing
original `Source { text }` or `SourceBundle { sources, entry }` provenance for
metadata. `build_syntax_index` is only the extractor; ordinary semantic
validation is never replaced with syntax-only acceptance or the standard
checker applied to erased types. AstOnly is explicitly rejected with CGT095
after authentication.

Normal file/package `api` now uses this view. Public records/variants retain
qualified identities, generics, constraints, nested types, signatures, original
declaration spans and public contracts. Original public generic functions remain
visible independently of concrete specializations. Private helpers, private type
declarations, bodies and compiler-generated/private specialization identities
are absent. Behavioral effects/capabilities come from authenticated summaries
or explicit declarations; unavailable inference is unknown. Contracts retain
runtime-check status; loop annotations are assumed, not proof results.

File JSON retains the declaration envelope (schema 6), with
`semantics_status: checked-ordinary-memory-v1`. Additive
`public_memory_api_version: 1`, `summary_scope: public-original-declarations`,
`absent_summary: unknown`, provenance kind/version, inputs hash and analysis hash
identify this distinct view. It does not advertise concrete instantiation
summaries. Generic declarations and concrete summaries remain separate.

Bundle JSON has schema 1 and modules containing original source paths, public
APIs and imports, sorted by module identity/path. Types keep original qualified
identities; spans refer to original module sources. CLI package JSON adds
`package.entry`, `manifest_sha256`, `binding_lock` and `source_hashes` from
CheckedMemoryPackageV1 public accessors. Text includes source paths, imports,
checked declarations and manifest hash/lock metadata. This versioned bundle does
not manufacture package-name, topological-order or resolved-call data that the
certificate does not expose. Freshness is rechecked before returning output.

`memory-effects` still uses unchanged build_memory_index_v2/schema 2, including
private/transitive resource summaries and existing AstOnly behavior. Checked
source/package errors preserve existing codes, filenames and available spans.
Explicit `api --syntax-only` remains syntax-only. No producer, package, runtime,
type implementation, root manifest/lock or coordinator source changed.

## Verification

All Cargo commands used Rust 1.98.1, offline/locked dependencies, jobs=2,
private `target/release-ordinary-api-visibility` and
`target/tmp-ordinary-api-visibility`, PYTHONDONTWRITEBYTECODE=1 and
LIBRARY_PATH=/tmp/cgx2-wave22-lib.

| Final check | Result | Evidence under ordinary-api-repair-evidence/evidence |
| --- | --- | --- |
| API-index/CLI all-feature tests | 175 passed, 0 failed, 0 ignored; 42 suites | tests-final-corrected.json.gz |
| New focused Rust tests | 5 API-index + 3 CLI, included above | same log; new ordinary_public_api_repair.rs files |
| All-target/all-feature Clippy -D warnings | both owned crates passed | clippy-final.json.gz |
| Owned rustfmt check | passed | format-check-final.json.gz |
| Final all-feature consumers/libraries | built successfully | build-final-corrected.json.gz |
| New frozen product assertions | 84 passed, 0 failed | product-final-2/report.json |
| Existing API visibility CLI replay | 118 passed, 0 failed | replay-visibility/report.json |
| Existing specialization/public-instantiation replay | 207 passed, 0 failed | replay-specialization-external/report.json |
| Frozen identities | 5 artifacts/compiler unchanged; 915 source inputs unchanged | final-verification.json; frozen-manifest.json.gz |
| Git diff/whitespace check | passed | diff-check-final.json.gz |

The 409 final product assertions include human/JSON source/package declaration
equivalence with non-memory programs; public record/variant/generic metadata;
constraints and uninstantiated generics; region-qualified signatures; private
helper exclusion; compatible private transitive effects; explicit syntax-only
status; invalid private/generic bodies; exact CGT003 source/entry error identity
and UTF-8 byte spans. Historical replays also cover public forwarding functions,
public instantiations and private symbol rejection across checked consumers.
There is no dedicated reexport declaration in current syntax; no reexport
semantics were added.

Frozen cgx2 SHA-256:
`effe844ca96c0a64ed9632feb656080d5b58639e8ca52e126a423b45f637cd61`.
Actual rustc SHA-256:
`859254978c0a0402c32f949f6de0d99aee73be8d15f45aac00ae1448aac51e74`.
The manifest records consumer/library/compiler/runtime source identities.
No Cargo builds occurred after freezing. Historical debug/release correctness
compilation used standalone rustc through frozen products. Canonical top-level
API-index/CLI libraries were frozen; no external library probe or arbitrary
hashed dependency library was used.

## Failed attempts and evidence retention

Each supervised attempt has unique compressed JSON containing exact argv/cwd,
environment, exit/signal/reason, bounded stdout/stderr, byte counts/hashes,
duration and limits. The existing read-only supervisor enforces 1500 s wall,
1200 s CPU/process, 16 GiB address space/process, 256 MiB/file and 2 MiB/pipe.
Focused product children use 35 s wall, 30 s CPU, 2 GiB address space,
16 MiB/file and 1 MiB/pipe. These are per-process, not aggregate guarantees.

| Attempt | Failure and disposition |
| --- | --- |
| tests-final.json.gz | Added fixture incorrectly omitted required public return annotation; CGT006 rejected it. Corrected fixture; final 175 tests passed. |
| build-final.json.gz | Cargo exit 101: cgx2-native is a cgx2-cli binary, not a package. Corrected package selection; final build passed. |
| product-final-1.json.gz and full report/sources | 58 assertions passed, 3 harness assumptions failed: comparison included concrete instantiation text; schema-1 modules omit empty records/variants; expected error code was wrong. Corrected comparisons/CGT003 expectation; same frozen artifacts passed all 84 final assertions. No implementation change after freeze. |
| replay-specialization.json.gz | Historical runner rejected assigned in-repository TMPDIR at outside-repository cwd assertion, before product work. Traceback retained. |
| replay-specialization-external.json.gz | Directory-only retry passed 207 assertions in private external scratch under this chat's writable visualization directory. Compiler TMPDIR stayed assigned/private. No host /tmp scratch was created. |

The replay wrapper changes only historical hardcoded /tmp working-directory
placement; original scripts remain unchanged and hashed in final verification.
Generated native artifacts and original emitted sources are hash-verified and
retained in private target/retained-ordinary-api-replays. Portable emitted
sources are committed as gzip archives. Final verification indexes original
paths, hashes, sizes and retained locations. No failed evidence was overwritten
or promoted into a passing result.

Initial branch creation hit read-only shared Git metadata and succeeded using
authorized sandbox escalation. Exploratory reads included nonexistent guessed
paths; these discovery errors were not acceptance checks. Pre-existing
untracked .serena/ is outside this delivery.

## Remaining failed, blocked and unrun scope

- OM08 is repaired at this lane boundary. Combined integration/coordinator
  review remain pending; no combined repair acceptance is claimed.
- OM01–OM07 are untouched and remain with their assigned lanes; baseline
  failed findings are neither waived nor relabeled by these API results.
- Static dependency-file attribution remains a known failed requirement.
  Existing attribution is preserved; exact source/entry spans were verified.
- General aggregate ownership, projected loans/partial moves/reborrows/recursive
  ownership, source copy_owned/promote with same-owner failure continuation,
  complete authenticated v3 summaries and remaining fault/cleanup/parity are
  incomplete and assigned to the later sole producer.
- Full workspace/runtime/conformance/ASan suites, all ordinary-memory product
  regressions, foreign/GPU/solver/isolation studies, release performance and
  human/model studies were not run for this bounded repair.
- No final assigned API check is blocked. The standalone-directory failure has
  a successful retained directory-only retry. Full memory/release remain unaccepted.

## Owned files and reproduction

Implementation: crates/cgx2-api-index/src/{memory_v2.rs,lib.rs}, and only API
routing/rendering in crates/cgx2-cli/src/main.rs. New tests:
crates/cgx2-{api-index,cli}/tests/ordinary_public_api_repair.rs.
Evidence/sources/wrappers/raw reports: ordinary-api-repair-evidence/.

Run commands through `python3
crates/cgx2-syntax/ordinary-api-repair-evidence/command.py LABEL COMMAND ...`
from the repository root. Choose fresh labels/output directories every time.
Build/test before a fresh freeze; no Cargo during frozen verification.
`freeze.py NEW_DIR` copies final consumers/canonical libraries;
`run.py --binary-dir FROZEN --output-dir NEW_OUTPUT` runs focused assertions.
`replay.py SCRIPT ...` replays unchanged historical scripts; specialization also
needs `--standalone-root NEW_PRIVATE_EXTERNAL_PARENT`. Exact argv is retained
in the command records. `finalize.py` is this delivery's one-shot archive/export
and deliberately refuses to replace evidence.
