# Notebook Output Provenance Implementation Plan

> Implement in the attached worktree, commit only owned paths, and stop for user review. Work directly in this chat. Do not delegate, merge, create another chat, or automate follow-up work.

**Goal:** Make saved `.cgnb` outputs auditable and honestly stale after source or dependency changes, while preserving a portable native notebook file.

**Architecture:** Add optional, versioned execution provenance for each code cell: source hash, ordered prefix hash, evaluator profile identifier, execution ordinal, and result status. The kernel produces it only after a real run; format round-trips it; the local UI and CLI distinguish cached, current, stale, and failed outputs after open, edit, rerun, save, and export. Existing version 1 notebooks without provenance still parse and their saved outputs remain cached, never current by assumption.

**Tech stack:** Rust 1.98, existing `.cgnb` format/session/kernel/UI/CLI crates; local loopback HTTP tests.

**Spec:** `docs/language-design.md` “CGx2 notebooks”; `crates/cgx2-notebook-format/README.md`; `plans/wave9/README.md`.

## Global constraints

- Own only `crates/cgx2-notebook-format/**`, `crates/cgx2-notebook-session/**`, `crates/cgx2-notebook-kernel/**`, `crates/cgx2-notebook-ui/**`, `crates/cgx2-notebook-cli/**`, and this plan. Preserve old version 1 files, stable cell IDs, all current HTTP security checks, save/overwrite behavior, and pure-scalar execution limits.
- Specify one deterministic hash algorithm and byte serialization for source and ordered prefix context. Include the executable prefix, not Markdown. A source or earlier definition edit invalidates claimed current state; a saved output is only a cached observation after reopen until the kernel actually executes the matching cell in this session. No timestamp alone establishes currentness.
- Provenance must bind output to a cell ID and ordinal and use a documented `schema_version`. Malformed/duplicate/mismatched records are parse errors rather than silently trusted. Avoid embedding source or secret values in provenance. Respect 16 MiB notebook and 8 MiB asset limits.
- A selected rerun remains pure-scalar and bounded. Side-effecting cells, Python, shell, and arbitrary assets do not become executable. UI must escape output text and keep Host/Origin/token/loopback protections.

## Review focus

- Old notebooks reopen with outputs marked cached, never current.
- Editing an earlier cell must mark later dependent outputs stale even if their source bytes are unchanged.
- A failed rerun must not retain a previous success as current.
- A Unicode/escaped `%%` body must hash and round-trip exact bytes.
- Self-contained asset export/import must preserve valid provenance and reject corrupt hashes.

## Tasks

### Task 1: Format and compatibility

**Files:** `crates/cgx2-notebook-format/src/{lib,parse,write}.rs`, tests/fixtures and README.

- [ ] Add failing fixtures for legacy files, valid provenance round-trip, malformed/duplicate records, Unicode and escaped headers, oversized records, and portable asset export/import.
- [ ] Implement the optional provenance record in version 1 without changing existing cell/output grammar; document exact header fields, hash inputs and validation.
- [ ] Run `cargo test -p cgx2-notebook-format --offline`.

### Task 2: Kernel and session state

**Files:** `crates/cgx2-notebook-{session,kernel}/src/**`, their tests.

- [ ] Add failing tests for run/edit/rerun/reopen/restart with source and dependency edits, failed rerun, repeated runs and ordinal behavior.
- [ ] Generate provenance only after a successful bounded execution; invalidate correctly on changed definitions or uncertain dependencies. Treat imported cached records as observations, not proof of a live environment.
- [ ] Run `cargo test -p cgx2-notebook-session -p cgx2-notebook-kernel --offline`.

### Task 3: UI, CLI, and handoff

**Files:** `crates/cgx2-notebook-{ui,cli}/src/**`, tests and READMEs.

- [ ] Add HTTP/CLI tests for cached/current/stale/failed display, save and reopen, bad provenance, token/Origin failures, HTML-like output and self-contained export.
- [ ] Show provenance and status in the local UI and `inspect` output without claiming a cached result is fresh. Retain explicit save and restart behavior.
- [ ] Run UI HTTP tests with loopback permission, fmt, targeted Clippy `-D warnings`; commit only owned paths and report browser smoke plus unsupported side effects.

**Acceptance:** Native notebook outputs retain inspectable provenance through save/export, and the user can reliably tell cached, fresh, stale and failed results apart.
