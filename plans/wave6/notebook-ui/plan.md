# CGx2 Notebook Local UI Plan

> Implement in the attached worktree, commit only owned paths, and stop for user review. Do not delegate, merge, or create another chat.

**Goal:** Give `.cgnb` notebooks a small, usable browser interface for editing cells, executing the supported pure scalar profile, inspecting results, and explicitly saving the document.

**Architecture:** A new `cgx2-notebook-ui` binary crate owns a single notebook session. It loads a validated `.cgnb` with `cgx2_notebook_format`, uses `cgx2_notebook_kernel::NotebookKernel` for execution, and serves a static HTML/JS page and narrow JSON commands over a local HTTP listener. Use only public Wave 5 APIs and keep all request state in this crate. Opening the UI never changes the source file. Saving is a separate action with atomic replacement and explicit collision behavior.

**Tech stack:** Rust 1.98, std networking or a small already available offline dependency, existing notebook format/kernel crates, no external network service.

**Specification:** `crates/cgx2-notebook-format/README.md`, `crates/cgx2-notebook-kernel/src/lib.rs`, `crates/cgx2-notebook-cli/README.md`, `plans/wave6/README.md`.

## Owned paths and fixed boundaries

- Own only new `crates/cgx2-notebook-ui/**` and this plan. Do not edit the notebook kernel, format, CLI, syntax, root manifests/lock, or docs. Use a standalone crate manifest; coordinator will register it in the workspace.
- Command: `cgx2-notebook-ui open FILE.cgnb [--port PORT]`. Default to an ephemeral loopback port. Print the local URL and a clear stop command. Bind only `127.0.0.1`; refuse non-loopback bind requests. One process serves one explicitly named notebook. Do not add arbitrary file browsing or file paths in HTTP requests.
- Render code/Markdown source as escaped text or DOM `textContent`; display outputs as text, never trusted HTML. Permit editing code cells. If editing Markdown is feasible within the crate, include it; otherwise say clearly that it is read-only in this release. Preserve cell IDs, order, version, and valid asset references.
- Offer `Run next`, `Restart and run all`, and `Save as`. `Run next` follows kernel file order and exposes the kernel's restart requirement after an edit. A failed cell must show its error and must not be presented as successful. Save should use `write_notebook`, preserve outputs already produced in the current session, and require an explicit overwrite action for existing files. Reopening saved output must parse.
- Security: generate an unpredictable per-process token for state-changing requests; check token and `Origin`/`Host` against the loopback listener; reject oversized requests before allocation, unknown methods/routes, invalid JSON, and cross-origin requests. Include a restrictive Content Security Policy, avoid CDN scripts, and set `Cache-Control: no-store`. Bound session lifetime and per-request work enough that a malformed request cannot hang the process indefinitely. Explain local-machine threat limits.
- Only the pure scalar notebook profile is executable. No Python/PyTorch, memory scripts, shell commands, arbitrary `.cg`, persistent side effects, or rich HTML output. Never market this preview as full Jupyter parity.

## Review focus

1. A user opens a two-cell `.cgnb`, sees source, runs the first cell then the second, sees outputs/status, edits a code cell, restarts, and saves a valid `.cgnb` without changing the original unless explicitly requested.
2. Rejected cross-origin, missing-token, oversized, invalid-cell, and path-injection requests cause no notebook or filesystem mutation.
3. Source such as `<script>` and a malicious saved output remain inert text. Browser scripts are local and never fetch external resources.
4. Server reports a useful diagnostic for unreadable/invalid notebook, bind failure, kernel error, and save collision; the process can shut down cleanly.

## Tasks

### 1. Session and endpoint tests

- [ ] Write focused HTTP-level tests using an ephemeral listener for load, run-next, restart-run-all, edit, save, and reopen. Test a failed cell and that unsaved edits leave input bytes unchanged.
- [ ] Write rejection tests for token, Origin/Host, content length, route/method, malformed input, and output escaping.
- [ ] Implement the session state and narrow endpoints. Avoid exposing filesystem paths or a general command endpoint to the browser.

### 2. Browser interface

- [ ] Add a minimal responsive page with code editors, Markdown text, clear execution status, text output, restart warning, and explicit save control. Keyboard shortcuts are optional; clarity is required.
- [ ] Provide a screenshot or concise manual smoke transcript and document browser access, limits, and save semantics in crate README.

### 3. Verification and handoff

- [ ] Run `cargo test -p cgx2-notebook-ui --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-notebook-ui --all-targets --offline -- -D warnings` where the standalone package permits.
- [ ] Commit only owned paths and report exact UI flow, security tests, and profile limits.

**Acceptance:** A bounded local UI supports real pure scalar `.cgnb` editing, execution, and explicit saving with tests for both the workflow and local HTTP protections.
