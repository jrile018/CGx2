# S1-01 release core evidence

Date: 2026-10-23. Worktree: `/home/john-riley/.codex/worktrees/18b8/lang`.
Branch: `riley/release-core`. Reviewed input base:
`0f0b0b9c87d817f39830f3e2b311dbae2ae3e0f3`.
The delivery commit identifier is reported with the review handoff (a commit cannot
contain its own identifier). No merge, publication, follow-on stage, or delegation.

## Implemented surface and owned files

- `crates/cgx2-cli/src/main.rs`: installed `build PATH --out BINARY`,
  `emit PATH --target rust|python --out FILE`, and `api --json PATH` for single
  sources and checked local packages. Preserves `api PATH --json` and prior commands.
- `crates/cgx2c/src/lib.rs`, `src/main.rs`: extract the existing compiler as an
  additive library entry point `run(Vec<OsString>) -> Result<(), CliError>`;
  retain the standalone binary and its old argument forms. `CliError` exposes
  `code`, `message`, `file: Option<PathBuf>`, and `span: Option<Span>` for the
  product diagnostic renderer. Package diagnostics remain relative, including
  canonical-root revalidation after a relative-path build.
- `crates/cgx2c/src/python_archive.rs`: deterministic bounded stored ZIP of the
  existing Python emitter's source map, allowing atomic package replacement.
- Owned Cargo manifests: path dependency `cgx2-cli -> cgx2c`, compiler dependency
  `cgx2c -> cgx2-emit-python`, and test-only `cgx2c -> cgx2-execution-policy`.
  These crates already exist in the workspace; no registry dependency was added.
- `crates/cgx2-cli/tests/release_core.rs` and its checked single/package fixtures:
  installed isolated product, debug/release/native/emitted parity, deterministic
  source/archive bytes, target/type/parse/compiler failures, input/link safety,
  Unicode spans, and source revalidation with artifact preservation.
- `crates/cgx2c/tests/execution_consumer.rs` and its kernel fixture: current
  interpreter/checked-kernel parity against owned-buffer map/zip library calls.
  This is a consumer contract for S2, not collection source lowering.
- `scripts/verify_release_core.py`, `RELEASE_CORE.md`, this report and `evidence/*`:
  executable installed acceptance, usage, limitations, and actual evidence.

No frozen AST/type/value/checked-summary representation changed. The underlying
emitter/package/API/sema implementations are reused. No other track or shared
plan/design/ledger file changed. Cargo's generated root lock changes were discarded
as required; the coordinator must reconcile the three path dependency edges above.

## Final verification

All commands below used Rust 1.98.1, `CARGO_BUILD_JOBS=2`, private
`CARGO_TARGET_DIR=$PWD/target/release-core` and `TMPDIR=$PWD/target/tmp-core`.
No shared targets and no timing/performance measurements. No apt/pip/downloaded
package installation; `cargo install` installs only this local product offline.

| Check | Exit | Actual outcome |
| --- | --- | --- |
| Owned offline/all-feature tests | 0 | **299 passed**, 0 failed, 0 ignored |
| Owned all-target/all-feature Clippy, `-D warnings` | 0 | pass |
| Read-only notebook/advisory regressions | 0 | **229 passed**, 0 failed, 0 ignored; includes 23 real loopback HTTP tests |
| Offline installation of local product | 0 | release `cgx2` and `cgx2-native` installed |
| Installed acceptance outside checkout | 0 | **65 checks passed**, exact commands/outcomes recorded |
| Existing native/Python/real-f32 boundary runner | 0 | scalar 5000; explicit Python 9; real CPU f32 `[[58,64],[139,154]]`; no libpython in native process maps |
| Owned formatting and `git diff --check` | 0 | pass |

The initial restricted consumer run failed all 23 HTTP tests on sandbox socket
binding (`Operation not permitted`). The complete consumer suite passed when
rerun with authorized local loopback access; no production workaround was made.
During fixture development, the map/zip consumer first mixed functions and script
statements, then used addition instead of the actual multiplication zip primitive;
both fixture mistakes were corrected before the final passing run.

Exact verification commands (with the environment above exported):

```sh
cargo test --offline --all-features -p cgx2-cli -p cgx2c -p cgx2-emit-rust -p cgx2-emit-python -p cgx2-api-index -p cgx2-package -p cgx2-sema
cargo clippy --offline --all-targets --all-features -p cgx2-cli -p cgx2c -p cgx2-emit-rust -p cgx2-emit-python -p cgx2-api-index -p cgx2-package -p cgx2-sema -- -D warnings
cargo test --offline --all-features -p cgx2-notebook-format -p cgx2-notebook-kernel -p cgx2-notebook-session -p cgx2-notebook-cli -p cgx2-notebook-ui -p cgx2-effect-index -p cgx2-security
cargo install --offline --path crates/cgx2-cli --root "$TMPDIR/install" --force
python3 crates/cgx2-cli/scripts/verify_release_core.py --binary "$TMPDIR/install/bin/cgx2" --output "$PWD/crates/cgx2-cli/evidence/release-core-installed.json"
python3 crates/cgx2-cli/scripts/verify_python_boundary.py --binary-dir "$TMPDIR/install/bin" --real-f32-package /tmp/cgx2-wave23-boundary-f32 --output "$PWD/crates/cgx2-cli/evidence/release-core-python-boundary.json"
cargo fmt -p cgx2-cli -p cgx2c -p cgx2-emit-rust -p cgx2-emit-python -p cgx2-api-index -p cgx2-package -p cgx2-sema --check
git diff --check
```

Local build/test logs remain under `target/tmp-core/{owned-tests,owned-clippy,
consumer-tests,consumer-tests-restricted,install}.log`. Persisted JSON reports
record candidate binary SHA256s, exact argv, exit codes, stdout/stderr, ELF loader
dependencies, actual native/worker mapped files, and real package lock/evidence
hashes. Temporary installed fixtures were removed after execution. CPU f32 evidence
is conformance of the existing pinned package, not a new throughput, GPU, or
general foreign lowering claim.

## Explicit boundaries and handoff

The output destination is explicit and must have an existing parent. Python
package output must end in `.pyz`; individual `.py` members remain inspectable and
editable after extraction. Source/archive emission is deterministic; native build
receipts include compiler/profile/source identity. Native output depends on rustc
and platform; byte-identical binaries across machines are not claimed.

`test`, `verify`, `notebook`, and `bind` still need their S2 product integrations,
as detailed in `RELEASE_CORE.md`. Variants, generics, full memory/foreign lowering,
source collection map/zip calls and capability enforcement remain S2 dependencies.
Memory scripts retain their existing native build profile; unsupported Python
memory/foreign emission and unsupported package constructs fail explicitly.

| Later acceptance | Status here | Executable handoff / required input |
| --- | --- | --- |
| Real pinned CPU float32 workflow | passed | Rerun the boundary command above with the actual reviewed package path; do not substitute absent artifacts |
| Third-party Python-only package/source integration | unrun | Requires the S1 Python pins and reviewed S2 lowering; run installed `cgx2 check /ABS/PINNED_PACKAGE` and `cgx2 run /ABS/PINNED_PACKAGE` on the downstream acceptance fixture |
| Additional native/tensor package routes and devices | unrun | Prepare reviewed pinned artifacts first, then run the boundary script with `--real-f32-package`; device-specific outcomes need the applicable real device fixture |
| GPU selection / source map-zip lowering | unrun; S2 dependency | `cargo test --offline -p cgx2c --test execution_consumer` is the current executable consumer contract; wire the documented reviewed APIs after S2 contract freeze |
| Leak-enabled assurance, live trusted proofs, isolated/calibrated runtime studies, genuine human/model studies | unrun; outside S1-01 | Require the S1 owner runners and S4 candidate/host/tool/task/reviewer inputs; this command suite supplies no surrogate pass |

Broader host/study gates are owned by their finite-plan lanes and must use their
reviewed executable runners after integration. No new speculative host/study CLI
is invented here. Only passed results above count as evidence of this bounded
delivery; they do not declare CGx2 release readiness.
