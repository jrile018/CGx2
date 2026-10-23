# Installation and detached smoke

The first-release distribution is not yet certified. This procedure checks the
reviewed native preview outside its checkout without installing packages or
changing host policy. Required inputs: approved CGx2 bundle and SHA-256 list,
compatible Rust 1.98.1 for `cgx2c`, an existing writable scratch directory, and
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
export SMOKE_DIR=$(mktemp -d /tmp/cgx2-docs-install.XXXXXX)
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
../bin/cgx2 --diagnostic-format=json run negative/unknown.cg
# Expected: exit 1, CGT001, negative/unknown.cg, byte span [26,33).
../bin/cgx2c build scalar/loops.cg --out "$SMOKE_DIR/program"
"$SMOKE_DIR/program"                               # 42
../bin/cgx2c build memory/manual.cg --out "$SMOKE_DIR/memory"
"$SMOKE_DIR/memory"                                # exit 0, empty stdout
```

Run the negative command separately when using `set -e`. Keep the receipts,
actual exits and hashes, including Rust identity. `cgx2-native` must be colocated
with `cgx2`; otherwise package dispatch is blocked (`CGN001`). The native compiler
embeds runtime source and should not need the original checkout. Rust itself
must remain available; pin it explicitly with `--rustc /absolute/rustc` for a
release reproduction. A copied `cgx2c` is not a freestanding compiler bundle.

Run the full [checker](../../tools/docs-check/README.md) against copied artifacts
and approved pins to catch cases beyond this smoke. A missing notebook, solver,
foreign environment or shared library is blocked acceptance. Do not substitute
mock runtimes. Archive the scratch receipt/report before removing only the
scratch directory you created. Final packaging also needs a support matrix,
offline rebuild/target-drift checks and all R01–R14 gates; this smoke closes none
of those automatically.
