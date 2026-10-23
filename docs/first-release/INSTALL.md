# Installation and detached smoke

The first-release distribution is not yet certified. This procedure checks the
reviewed native preview outside its checkout without installing packages or
changing host policy. Required inputs: approved CGx2 bundle and SHA-256 list,
compatible Rust 1.98.1 for native builds, a compatible pinned Python for emitted
`.py`/`.pyz` execution, an existing writable scratch directory, and
ordinary system dynamic libraries. The default CLI route needs no CPython
development/linker library. Explicit Python source calls need a separate Python
runtime; direct LibTorch needs its own pinned native artifacts.

To build the default binaries from the reviewed source, in an isolated checkout:

```sh
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/release-docs"
export TMPDIR="$PWD/target/tmp-docs"
export RUSTUP_TOOLCHAIN=1.98.1
mkdir -p "$CARGO_TARGET_DIR" "$TMPDIR"
cargo build --offline --locked -p cgx2-cli -p cgx2c
```

This build is a smoke prerequisite, not a reproducible-release attestation. No
all-feature build is required for the Python-free CLI. Test-only or in-process
Python features have separate prerequisites. If offline dependencies or Rust are
absent, record blocked and ask the distribution owner for reviewed artifacts;
do not install them as a side effect of an import or docs check.

Set `REVIEWED_BIN` to the approved bundle directory (`cgx2`, `cgx2-native`,
`cgx2c`). A bundle assembled from mixed build profiles needs explicit source and
hash provenance for each artifact. Then, from the checkout root:

```sh
export REVIEWED_BIN=/absolute/reviewed/bundle
export DOCS_ROOT="$PWD/docs/first-release"
export SMOKE_ROOT=/var/tmp  # existing writable directory outside the checkout
export SMOKE_DIR=$(mktemp -d "$SMOKE_ROOT/cgx2-docs-install.XXXXXX")
export RUSTC=/absolute/approved/rustc
export PYTHON=/absolute/approved/python3
mkdir "$SMOKE_DIR/bin"
cp "$REVIEWED_BIN/cgx2" "$REVIEWED_BIN/cgx2-native" \
  "$REVIEWED_BIN/cgx2c" "$SMOKE_DIR/bin/"
cp -R "$DOCS_ROOT/fixtures" "$SMOKE_DIR/examples"
sha256sum "$SMOKE_DIR/bin/"*   # compare against the independent approved list
cd "$SMOKE_DIR/examples"
../bin/cgx2 check scalar/answer.cg
../bin/cgx2 run scalar/answer.cg                    # 42
../bin/cgx2 scalar/arguments.cg -- detached          # detached
../bin/cgx2 run package                             # 42
../bin/cgx2 api --json package                      # checked public index
../bin/cgx2 build scalar/loops.cg --out "$SMOKE_DIR/program" --rustc "$RUSTC"
"$SMOKE_DIR/program"                               # 42
../bin/cgx2 build package --out "$SMOKE_DIR/package-program" --rustc "$RUSTC"
"$SMOKE_DIR/package-program"                       # 42
../bin/cgx2 build memory/manual.cg --out "$SMOKE_DIR/memory" --rustc "$RUSTC"
"$SMOKE_DIR/memory"                                # exit 0, empty stdout
../bin/cgx2 emit scalar/answer.cg --target rust --out "$SMOKE_DIR/answer.rs"
"$RUSTC" --edition=2024 --cap-lints allow "$SMOKE_DIR/answer.rs" -o "$SMOKE_DIR/rust-program"
"$SMOKE_DIR/rust-program"                          # 42
../bin/cgx2 emit scalar/answer.cg --target python --out "$SMOKE_DIR/answer.py"
"$PYTHON" -I "$SMOKE_DIR/answer.py"                 # 42
../bin/cgx2 emit package --target python --out "$SMOKE_DIR/package.pyz"
"$PYTHON" -I "$SMOKE_DIR/package.pyz"               # 42
```

From the same cwd, run negatives separately when using `set -e`:

```sh
../bin/cgx2 --diagnostic-format=json run negative/unknown.cg
# exit 1, CGT001, negative/unknown.cg, byte span [26,33).
../bin/cgx2 --diagnostic-format=json build scalar/answer.cg
# exit 1, CGXC001: --out required; file scalar/answer.cg, null span.
../bin/cgx2 --diagnostic-format=json emit package --target python --out "$SMOKE_DIR/wrong.py"
# exit 1, CGXC001: package output requires .pyz.
```

Keep the receipts,
actual exits and hashes, including Rust identity. `cgx2-native` must be colocated
with `cgx2`; otherwise package dispatch is blocked (`CGN001`). The native compiler
embeds runtime source and should not need the original checkout. Rust itself
must remain available; pin it explicitly with `--rustc /absolute/rustc` for a
release reproduction. A copied `cgx2c` is not a freestanding compiler bundle.

To repeat the full outside-checkout suite, copy `expectations.json` beside a
copy of `fixtures/`, supply it with `--suite`, and use copied bundle paths with
the original approved tool digests. Keep `--work-root` outside the checkout and
the report in a new output file. Rust and Python remain pinned host prerequisites;
copying Rust alone may omit toolchain libraries. The [Stage 1 evidence](CURRENT-EVIDENCE.md)
records this full detached run and its limits.

Run the full [checker](../../tools/docs-check/README.md) against copied artifacts
and approved pins to catch cases beyond this smoke. A missing notebook, solver,
foreign environment or shared library is blocked acceptance. Do not substitute
mock runtimes. Archive the scratch receipt/report before removing only the
scratch directory you created. Final packaging also needs a support matrix,
offline rebuild/target-drift checks and all R01–R14 gates; this smoke closes none
of those automatically.
