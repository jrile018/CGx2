# Reopenable self-contained `.cgnb` export

> Use the attached worktree. Commit only owned paths and stop for user review. Do not delegate, merge, start another chat, or automate follow-up.

**Goal:** Export a native `.cgnb` notebook and its bounded rich assets as one portable file that reopens without sidecars, while keeping saved outputs explicitly cached until executed in the new session.

**Architecture:** Extend the versioned native notebook format with a documented self-contained asset representation or a versioned wrapper that retains ordinary readable code/Markdown cells and stable IDs. Export converts content-addressed sidecars into embedded bytes with exact digest/MIME/length; import verifies all three before exposing them. Reuse the Wave 12 retained-state pure-scalar evaluator unchanged. Rich HTML, SVG/script and active content are inert unless a separate sanitized renderer can prove safety; image assets should use a strict allowlist and size caps. A bad or oversized asset fails atomically, leaving the source notebook and destination untouched.

**Ownership:** Only `crates/cgx2-notebook-format/**`, `crates/cgx2-notebook-session/**`, `crates/cgx2-notebook-kernel/**`, `crates/cgx2-notebook-cli/**`, and `crates/cgx2-notebook-ui/**`. Do not edit `cgx2-cli`, source grammar/checker, runtime, root manifests/lock, docs or plans. Consume frozen Wave 12 pure-scalar semantics only; no new side-effecting cell support.

## Required behavior

1. Fixture-test a notebook with Markdown, two code cells, text output, an allowed image asset, stable IDs and provenance. Export to one file, remove/move sidecars, reopen via CLI and UI, and verify asset bytes/digest/MIME and cell/output text. Reopened saved results must be cached/noncurrent. Restart and Run All still produces fresh scalar results without replaying earlier cells on later `run_cell`.
2. Reject traversal, symlink escape, duplicate asset ID, digest mismatch, invalid MIME, decompression/base64 bomb or over-limit asset, malformed UTF-8 notebook text, and active HTML/script as executable output. Bound per-asset, total asset and notebook file sizes. Preserve existing loopback host/origin/token protections.
3. Document exactly when export overwrites an existing file, temporary file cleanup on failure, version compatibility with ordinary `.cgnb`, and whether old clients can read the exported form. No hidden network fetch or Python/Jupyter conversion.
4. Provide a CLI example and UI path for export/import; test saved source is not modified unless the user explicitly chooses overwrite.

## Verification and handoff

Run `RUSTUP_TOOLCHAIN=1.98.1 LIBRARY_PATH=/tmp/cgx2-python-lib cargo test -p cgx2-notebook-format -p cgx2-notebook-session -p cgx2-notebook-kernel -p cgx2-notebook-cli --offline`, UI tests where loopback sockets are available, format and targeted Clippy `-D warnings`. Commit owned paths and report SHA, roundtrip bytes, security rejections and compatibility limits.

**Acceptance:** A single exported native notebook reopens with its assets and honest cached-output status after its sidecar directory is absent.
