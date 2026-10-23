# Stage 1 command guide repair evidence

Date: 2026-10-23. Branch: `riley/s1-review-docs-current`.
Integrated candidate base: `2b9855c48713794d4cf819bc391813c1260f287b`.
The guide, suite and inventory now describe that command surface. `candidate`
identifies the integrated source input; the final repair commit and report's
`suite_sha256` identify the changed documentation. `baseline` remains the historical
`0f0b0b9c87d817f39830f3e2b311dbae2ae3e0f3`. Original Wave 28 reports and results
are preserved in [historical evidence](EVIDENCE.md); none was regenerated in place.
Inventory requirement descriptions now use `candidate_scope` to avoid conflating
the current surface with the historical baseline.

## Results

- [Reproduced pre-repair suite](evidence/s1-before.json): **26 passed, 2 failed,
  6 blocked, 8 future**, exit **1**. The failures are the original unified
  build/emit unsupported assertions; both actually return `CGXC001` for missing
  `--out`, with file `scalar/answer.cg` and null span.
- [Current candidate](evidence/s1-current.json): **39 passed, 0 failed,
  6 blocked, 7 future**, `--runnable-only` exit **0**. It replaces both stale
  cases and the future Python CLI reservation with installed native build,
  Rust/Python emission and package public API execution. Rust and Python results
  are explicitly 42; source/occupied output protection and force replacement
  distinguish 42 from the [seven fixture](fixtures/scalar/seven.cg)'s 7.
- The default complete-acceptance policy returns **2** with the same report
  bytes/counts. `acceptance_complete` remains **false**. All R01–R15 gates remain open.
- [Missing Python pin](evidence/s1-no-python.json): **37 passed, 0 failed,
  8 blocked, 7 future**, exit **2**. Both Python execution cases stay blocked.
- [Detached installation](evidence/s1-installed.json): **39 passed, 0 failed,
  6 blocked, 7 future**, exit **0**, using copied binaries, suite and fixtures
  outside the checkout at `/var/tmp/cgx2-docs-s1-o27ti7jt`. The full 39-case
  suite ran from copied inputs; tool hashes matched the approved input pins.
  Only this script's private scratch directory was removed after archiving.
- [Harness tests](evidence/s1-unit-tests.txt): **20 passed**, exit **0**.
  The two new regressions first failed ([red log](evidence/s1-regressions-red.txt)):
  current-candidate metadata was rejected and caller-private Cargo target was
  overwritten. They now pass. Original 18 harness tests remain passing.
- Two final integrated runs produced byte-identical JSON (`cmp`, exit **0**).
  Generated native binary bytes are not attested reproducible: they embed scratch
  roots. Their actual output behavior is checked and origin recorded.

## Exact commands and input provenance

The six CGx2/Rust input paths and expected digests below come from the coordinator's
`plans/release-completion/s1-review/evidence/s1-docs-acceptance.json`, not newly
captured outputs. The compatible Python interpreter pin was already recorded in
`bench/results/release-evidence-functional-v1.json` (`raw_observations`, executable
SHA-256 `52e0a13e...`). Its installed bytes match that prior identity. This is local
functional evidence using supplied repository/host tools; it is not independent
binary review, interpreter distribution trust or dynamic-library attestation.
No tools, dependencies or packages were installed. No external studies ran.

From the attached worktree root:

```sh
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/repair-docs-current"
export TMPDIR="$PWD/target/tmp-docs-current"
export RUSTUP_TOOLCHAIN=1.98.1
export PYTHONDONTWRITEBYTECODE=1
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
python3 tools/docs-check/check.py \
  --work-root "$TMPDIR" \
  --report "$CARGO_TARGET_DIR/reproduce.json" \
  --runnable-only \
  --tool compiler=/home/john-riley/projects/lang/target/release-plan-review/debug/cgx2 \
  --pin compiler=4be93bfdee099a736a26ba4a131aa2333291177c916ff08b5c71b38ab7c38ce4 \
  --tool native=/home/john-riley/projects/lang/target/release-plan-review/debug/cgx2c \
  --pin native=be987606ba5873d586141951071454a844da43402804933332740566976b5bca \
  --tool notebook=/home/john-riley/projects/lang/target/release-plan-review/debug/cgx2-notebook \
  --pin notebook=8ad5e677d86f249d52d68ae144be88498d7dcc1057544889fdafad8597c702f5 \
  --tool package-runner=/home/john-riley/projects/lang/target/release-plan-review/debug/cgx2-native \
  --pin package-runner=d87c4f21c8ab940bb98c8646162e253df11e03470dda9133001d142bfd8ac462 \
  --tool python=/usr/bin/python3.14 \
  --pin python=52e0a13e60a981d8c4b6478be2ba5176f69da07948a056bf49cf6f077e30cb41 \
  --tool rustc=/home/john-riley/.rustup/toolchains/1.98.1-x86_64-unknown-linux-gnu/bin/rustc \
  --pin rustc=859254978c0a0402c32f949f6de0d99aee73be8d15f45aac00ae1448aac51e74 \
  --tool verify=/home/john-riley/projects/lang/target/release-plan-review/debug/cgx2-verify \
  --pin verify=1aa2b5912f233691dc88b095c85c0091f023722489d514da32dc35bfbb40389e
python3 -m unittest discover -s tools/docs-check -v
python3 docs/first-release/evidence/s1-detached-smoke.py
```

The [exact detached driver](evidence/s1-detached-smoke.py) copies only approved
CGx2 binaries and guide inputs, retaining Rust/Python as pinned host prerequisites.
It invokes the same checker with `--suite` pointing outside the checkout and saves
its report to `target/repair-docs-current/detached.json`. It deliberately uses
`/var/tmp`; this host's `/tmp` quota prevents sandbox startup. Normal scoped
execution approval was used; no unrelated temporary data was removed.

For the complete-policy run, execute the same checker command with a new report
path and omit `--runnable-only` (expected exit 2). For missing-Python verification,
use a new report path and omit both `--tool python=...` and `--pin python=...`
(expected exit 2). For deterministic replay, use a new report filename with all
pins, then `cmp` it against the prior report. Inputs and expectations are reviewed
explicit assertions; no capture/update/auto-bless mode was added.

## Scope, review and limits

Only `docs/first-release/**` and `tools/docs-check/**` changed. Guide/installation,
command and acceptance inventories, suite/fixture, raw evidence, checker and
regressions are included. The additive optional full-commit `candidate` field
preserves old suites. `CARGO_TARGET_DIR` now honors caller isolation, defaulting
to `target/release-docs` only when absent. Checker output bounds, process cleanup,
identity checks and immutable publication remain unchanged.

Author self-review covered the diff, every new case, requirement links, exact
negative diagnostics and retained history. Delegation is excluded by this repair
plan, so no fresh-agent review was requested. The coordinator still reviews this
candidate. AST parse, local Markdown links, inventory linkage and `git diff --check`
passed. No Rust files changed; Cargo tests, Rust formatting and Clippy do not apply
to this Python/documentation repair. No unrelated workspace suite was rebuilt.

Six external rows remain blocked: Python-only package workflow, full typed
LibTorch inference, trusted live solver, leak-enabled/independent memory/resource
acceptance, genuine paired human/model/grammar studies, and final release review.
Seven future reservations retain their S2/S3 handoffs. Unified test/verify/notebook/
bind integration is absent. This guide establishes small admitted examples, not
full language/type/memory/foreign semantics, broad target parity or readability.
The verifier's known nested-division growth case was not exercised; compiler and
verifier semantics were not edited. No merge, new chats, delegation or Stage 2 work.

The final commit identifier is reported with delivery; this evidence is part of
that commit. Work stops for coordinator review.
