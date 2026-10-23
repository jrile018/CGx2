# Wave 28 integrated review

Reviewed on `riley/release-completion-plan`, 2026-10-23, in the attached coordinator worktree. Main and the original checkout were not merged or edited.

## Integrated deliveries

- Package checked imported effects: original `21fd123edf46bb7d5042994aadf2f2702618e772`, integrated `8cbc393`.
- Notebook local accumulators and literal ranges: original `a4a71be2a98c04bd60e53b0be548856273801a77`, integrated `8d71368`.

Read-only planning reviewers found no concrete production defect in their limited static review. Coordinator repaired an obsolete notebook test which expected an explicitly pure checked imported wrapper to reject. The test now executes the wrapper, verifies both module provenance entries, and runs a notebook local accumulator loop through the transitive wrapper (result 18), including saved provenance round-trip. Private/effectful/duplicate import rejection remains tested.

## Integrated verification

- Full workspace offline/all-feature tests: **878 passed, 0 failed, 5 ignored**, including **23 HTTP tests**. The ignored tests remain ignored; no release claim is based on them.
- Workspace all-target/all-feature Clippy with `-D warnings`: pass.
- `cargo fmt --all --check` and `git diff --check`: pass.
- Python benchmark tests: **122 passed**.
- Public compiler readiness: **6/6 checks pass**, toolchain identity pass.
- Fresh default release CLI build: pass.
- Real release boundary: native result **5000** with no Python loaded; explicit Python child result **9**; direct real f32 matrix **[[58,64],[139,154]]**, no Python child or libpython in native/tensor maps. See [boundary evidence](evidence/release-boundary.json).

Environment: Rust 1.98.1; private Cargo target `/home/john-riley/projects/lang/target/release-plan-review`; `LIBRARY_PATH=/tmp/cgx2-wave22-lib`; private TMPDIR `/home/john-riley/projects/lang/target/cgx2-wave21-tmp`. HTTP tests required loopback-enabled execution. Initial restricted runs could not bind sockets/write worktree-local fixture directories; those environment failures were rerun with the required access. No production workaround weakened checks.

Exact commands:
```sh
cargo test --workspace --all-features --offline
cargo clippy --workspace --all-targets --all-features --offline -- -D warnings
cargo fmt --all --check
git diff --check
python3 -m unittest discover -s bench/tests
python3 bench/readiness.py --compiler /home/john-riley/projects/lang/target/release-plan-review/debug/cgx2c
cargo build --release --offline -p cgx2-cli
python3 crates/cgx2-cli/scripts/verify_python_boundary.py --binary-dir /home/john-riley/projects/lang/target/release-plan-review/release --real-f32-package /tmp/cgx2-wave23-boundary-f32 --output /home/john-riley/projects/lang/target/wave28-review-release-boundary.json
```

Compiler-source provenance lock updated to `154ba81fc573d7a67f04576ffa6b33fc14e35dfb` (HEAD:crates/cgx2c). Local test/build/readiness logs are in the original checkout's ignored target directory under `wave28-*`. Persisted evidence records the real runtime artifacts and pins.

## Remaining release gates

This closes two bounded implementation deliveries, bringing Waves 22–28 to **20 reviewed deliveries**. It does not close the entire release. Full type/ownership/foreign/notebook/proof and empirical requirements remain in the [finite completion plan](../release-completion/README.md). Leak-enabled host assurance, trusted live solver, scored isolation and genuine paired studies remain external gates.

