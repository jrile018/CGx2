# Owned verification evidence

Date: 2026-10-23. Branch: `riley/release-docs`. Reviewed source base:
`0f0b0b9c87d817f39830f3e2b311dbae2ae3e0f3`.
Only `docs/first-release/**` and `tools/docs-check/**` are delivered. No root Cargo
files, shared plans, compiler/runtime code or other lane files were edited.
No subagents, installs, model calls, participant studies or benchmarks were run.

## Actual executable acceptance

[reviewed-examples.json](evidence/reviewed-examples.json) records **28 passed,
0 failed, 6 blocked external rows and 8 future reservations**. The pass count
includes exact expected-unsupported regressions, not implemented feature claims.
`acceptance_complete` is false. The checker with `--runnable-only` exited **0**;
the same suite under the default complete-acceptance policy exited **2**. Both
policies retain all rows. Nothing here closes a global release gate.

The supplied Wave 28 artifact directory provided release `cgx2`/`cgx2-native`
and debug `cgx2c`/`cgx2-notebook`/`cgx2-verify`. The native builds use the real
Rust 1.98.1 executable. Exact input hashes, paths, help probes, command arguments,
stdout/stderr, source hashes and diagnostic spans are in the JSON report. These
are observations of supplied artifacts, not an independently reproduced source-
to-binary or dynamic-library attestation. The baseline has no common version
interface; `version: null` is explicit, with banners and native Rust receipts.

To reproduce against those same artifacts if still available (new report path):

```sh
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-docs"
export TMPDIR="$PWD/target/tmp-docs"
export RUSTUP_TOOLCHAIN=1.98.1
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
python3 tools/docs-check/check.py \
  --work-root "$PWD/target/tmp-docs" \
  --report "$PWD/target/release-docs/recheck.json" \
  --runnable-only \
  --tool compiler=/home/john-riley/projects/lang/target/release-plan-review/release/cgx2 \
  --pin compiler=bb4b72d58f0c440d04035f41db526c7bdaaca23d312ad89d60b45132251ad9c3 \
  --tool native=/home/john-riley/projects/lang/target/release-plan-review/debug/cgx2c \
  --pin native=71df213ecb5d75693a7142fc0be7dae7ff1d29f4d4b03f155e46a467edef9211 \
  --tool notebook=/home/john-riley/projects/lang/target/release-plan-review/debug/cgx2-notebook \
  --pin notebook=c5d821d9acfdb3bb78b22d4197d1707b6f75bdf4c043cc89c02aa3ab636b46da \
  --tool package-runner=/home/john-riley/projects/lang/target/release-plan-review/release/cgx2-native \
  --pin package-runner=bac1883043671d7f795fa44ed3c6fffb4412b05c9e396ed3d5d73473772277b4 \
  --tool rustc=/home/john-riley/.rustup/toolchains/1.98.1-x86_64-unknown-linux-gnu/bin/rustc \
  --pin rustc=859254978c0a0402c32f949f6de0d99aee73be8d15f45aac00ae1448aac51e74 \
  --tool verify=/home/john-riley/projects/lang/target/release-plan-review/debug/cgx2-verify \
  --pin verify=6d48b74e9e8e78487b6a51b9acfda649d704058c997bef32a100433e4501843e
```

If the artifacts have been replaced, obtain reviewed replacements and approve
new pins before rerunning; do not weaken the digest check. The primary compiler
can equivalently use `--compiler PATH --sha256 HEX`.

Two complete real runs produced byte-identical reports (`cmp`, exit **0**).
Native temporary binaries embed build roots and showed changing hashes in the
initial repeat check. Their output bytes are therefore **not** attested as
reproducible; deterministic reports hash pinned input tools and record generated
artifact origin/behavior instead. The raw reported observations are otherwise
preserved, apart from documented temporary/tool path substitutions. This is not
timing or reproducible-build evidence.

Native scalar, package and memory executables actually ran. Emitted Rust was
compiled and ran (42); the baseline emitter's unused-helper/argument warnings
are retained in stderr and explicitly expected. Package fixtures exercise the
pinned companion directly. Notebook run/show observed the saved local-loop
result 42. The selected finite proof fixture reports live proof `unknown`;
finite success is not relabeled formal proof. No Python emitter, foreign package,
real LibTorch inference, solver or general parallel/GPU source fixture ran.

## Detached installation smoke

[install-smoke.json](evidence/install-smoke.json) records **9 checks passed**,
exit **0**. A temporary `/tmp/cgx2-docs-install-*` directory held copied `cgx2`,
`cgx2-native`, `cgx2c` and examples. Its cwd and outputs were outside the checkout.
The run checked scalar source, script arguments, actual package wrapper dispatch,
exact negative UTF-8 diagnostic, and compiled/executed scalar and memory native
programs. Each copied executable's hash matches the corresponding input in the
main report. The scratch directory was removed after recording observations.
Rust and ordinary host libraries remained prerequisites. See [INSTALL.md](INSTALL.md)
for the executable operator procedure. No distribution installation was certified.

## Tool checks and deliberate failures

- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools/docs-check -v`:
  **18 tests passed**, exit **0**. [Full log](evidence/unit-tests.txt).
- Missing compiler/tools: actual checker invocation with no `--compiler`/`--tool`
  returned **2**, with **0 passed, 34 blocked, 8 future**.
- Wrong all-zero SHA-256 for the actual supplied `cgx2`: returned **1**, with
  **0 passed, 20 failed, 14 blocked, 8 future**. The tool has no executed banner
  or steps; the checker rejected its identity before launching it.
- Unit tests separately cover malformed/duplicate/unknown expectation fields,
  wrong exits/output/diagnostics, missing named prerequisites, symlink/traversal
  fixture/report paths, existing report preservation, timeouts, output caps,
  deterministic reports and inventory/case coverage. Synthetic test executables
  are harness tests only, never CGx2 acceptance evidence.
- Python AST parse, documentation local-link check and `git diff --check`: pass.
  No Rust files changed, so Rust package tests/Clippy are not applicable to this
  stdlib Python/documentation delivery. No root workspace build was run.

## Remaining gates and ownership

All R01–R15 remain open at release level; see [ACCEPTANCE.md](ACCEPTANCE.md).
Eight named future reservations require reviewed S2/S3 source contracts. Six
external rows require pinned Python packages, full typed direct LibTorch inference,
a trusted live solver, leak-enabled/independent memory/resource acceptance,
genuine human/model/grammar observations, and final release installation/review.
These are blocked/unrun with owner handoffs, not skips that earn passes. Library-
only Python emission and CPU planning remain explicitly separate from source
integration. No quiet-host runtime or resource comparison was attempted.

The commit identifier is reported with the final delivery; this evidence is part
of that commit. The work stops for review without merging or starting S2/S3.
