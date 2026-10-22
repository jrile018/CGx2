# Notebook captured-output cells

Use the attached worktree; read shared Wave 14 references and this plan. Commit only owned paths and stop for review.

**Goal.** Extend the native `.cgnb` experiment workflow so a cell can produce captured text output, save it, reopen it, and selectively rerun changed dependencies without replaying unchanged cells. Keep effects local to notebook capture; external files, network calls, and arbitrary host commands are outside this bounded deliverable.

**Ownership.** Only `crates/cgx2-notebook-format/**`, `crates/cgx2-notebook-session/**`, `crates/cgx2-notebook-kernel/**`, `crates/cgx2-notebook-cli/**`, and `crates/cgx2-notebook-ui/**`. Read the frozen Wave 13 CLI/session API. Do not edit it, root manifests, shared docs, plans, or another lane.

## Deliverable

1. Define a versioned captured-output record with MIME `text/plain`, bounded UTF-8 bytes, cell ID, execution revision, source/dependency digest, and provenance. The output is display data produced inside the kernel, not arbitrary HTML or executable script. If current `.cg` syntax has no output-producing primitive, use the existing pure expression result as captured text and document the exact restriction.
2. Persist captured output in `.cgnb` and portable export; reopen must reproduce recorded display without executing cells. A selective rerun updates only the changed cell and transitive dependents, invalidates stale output, and preserves unchanged execution revisions. Broken dependency cycles, oversized output, and invalid MIME fail with clear diagnostics.
3. Expose the same behavior through notebook CLI and loopback UI. Add a complete two-cell fixture: run, edit upstream, rerun affected cells, save, reopen, inspect output and provenance. No browser-side execution of notebook content.
4. Update owned README with exact size limits, rerun rules, and external side-effect limitation. Do not claim Jupyter ecosystem/plugin parity.

## Verification

Run offline tests for the five owned notebook crates, format, targeted Clippy with warnings denied, CLI round-trip fixture, and loopback UI tests where socket permission is available. Report unchanged-cell revision evidence, recorded versus executed behavior, limits, and commit SHA.

**Acceptance.** A saved notebook's output is inspectable on reopen without replay; selective rerun has deterministic invalidation and bounded stored data.
