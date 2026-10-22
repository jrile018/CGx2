# CGx2 Notebook File Command Plan

> Implement this plan in your attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, or launch another chat.

**Goal:** Give a user a command to execute a `.cgnb` notebook from a clean session and save fresh outputs, using the Wave 4 pure-scalar kernel.

**Architecture:** Add a standalone `cgx2-notebook` executable crate (`crates/cgx2-notebook-cli`) that reads a versioned `.cgnb`, invokes `NotebookKernel::open` and `restart_and_run_all`, and writes a new `.cgnb` to an explicit output path. Provide `inspect` to list cell IDs, kind, and status without executing. Use the existing format serializer so the file remains editable text. A later UI can use the same kernel; this lane does not add a browser server or pretend to support side-effecting cells.

**Tech stack:** Rust 1.98, existing notebook format/kernel crates, standard library, no network dependencies.

**Specification:** `docs/language-design.md` notebook section, `crates/cgx2-notebook-kernel/README.md`, `plans/wave5/README.md`.

## Owned paths and fixed boundaries

- Own only new `crates/cgx2-notebook-cli/**` and this plan. Do not edit the existing `cgx2` CLI, notebook kernel/format/session, root manifests/lock, or shared docs.
- Commands: `cgx2-notebook run INPUT.cgnb --out OUTPUT.cgnb` and `cgx2-notebook inspect INPUT.cgnb`. Require `.cgnb` extension. No implicit overwrite: if `OUTPUT` exists, fail unless an explicit `--force` is supplied. Never modify `INPUT` merely by inspecting or running.
- A failed cell exits nonzero, identifies cell ID and kernel/source code, and does not write a successful output file. Preserve any existing output file on failure. Use a temporary sibling file and atomic rename where the platform permits; avoid deleting user files on an error path.
- Parse file size and asset errors through the format crate. Rich assets remain preserved but this pure-scalar runner writes only `text/plain` outputs. Saved outputs in the input are stale cache and must be discarded before a clean run.

## Review focus

1. A two-cell notebook with `let x = 3` and `x + 4` saves `7` in the second code cell with stable ID, MIME, and ordinal. Markdown source and asset references remain unchanged.
2. `inspect` is read-only and reports cell order/kinds; it never runs code.
3. Failed source, an unsupported cell, missing asset, output collision, and I/O failure exit nonzero without replacing input or prior output.
4. Explicit `--force` replaces only the named output after a fully successful run, and same input/output path requires an explicit flag plus safe atomic replacement.
5. Help text plainly says pure scalar and clean run only; no Jupyter-like live UI or individual side-effecting reruns yet.

## Tasks

### 1. Command and file workflow

- [ ] Add red integration tests for `run`, `inspect`, output collision, unknown command, extension validation, and invalid `.cgnb` bytes.
- [ ] Implement command parser, format/kernel integration, diagnostics, and explicit output path behavior. Keep command errors distinct from code-cell errors.
- [ ] Add a checked-in example `.cgnb` and README invocation that can be copied by a new user.

### 2. Failure atomicity and serialization

- [ ] Test that stale input outputs are cleared and new outputs serialize and reopen correctly. Markdown and assets must survive.
- [ ] Test failed cell, output directory denial, and forced replacement behavior using temporary directories. No empty or partial output should be left as success.
- [ ] Document the pure-scalar, sequential replay limits inherited from the kernel.

### 3. Verification and handoff

- [ ] Run `cargo test -p cgx2-notebook-cli --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-notebook-cli --all-targets --offline -- -D warnings`.
- [ ] Commit only owned paths and report a command transcript on the example.

**Acceptance:** A user can run a `.cgnb` file from the command line and reopen the fresh result, with no silent overwrite or claim of full notebook UI semantics.
