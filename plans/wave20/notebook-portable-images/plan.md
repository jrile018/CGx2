# Portable markdown images in `.cgnb` experiments

**Goal.** Give native notebooks a deliberate, portable image-reference contract so clean parameter sweeps do not leave markdown figures pointing at a missing original sidecar.

**Ownership.** Edit only the five `crates/cgx2-notebook-*` crates and their owned fixtures/examples. Do not edit shared design docs, runtime/compiler, root Cargo files, or other lanes.

## Deliverable

1. Define one narrow markdown image form `![alt](asset:sha256:<64 lowercase hex>)` for PNG/JPEG/GIF/WebP bytes already validated by the notebook asset system. Treat arbitrary relative/absolute file links as ordinary untrusted markdown links, not implicitly portable assets. Validate digest, MIME, byte and count limits, and referenced/embedded set equality. Keep the file format version stable only if old readers can reject or ignore the new form safely; otherwise document a versioned migration in owned docs.
2. Extend open/import/export so digest-referenced markdown images load from verified sidecar assets or self-contained embedded records. A missing, corrupt, extra, or symlinked asset fails before publication. Never fetch network URLs or follow arbitrary notebook paths. Avoid silently rewriting user markdown to a host-dependent path.
3. Sweep must retain referenced markdown assets in every published trial even though it deliberately discards stale cell outputs and reruns code in clean kernels. Produce portable trial `.cgnb` files that reopen without the original input directory. Test two distinct sweep values, two images, malformed/missing digests, size limits, output collision, and a failed trial with preserved provenance. Keep atomic publication behavior.
4. Review UI rendering for the new reference form, using safe image data or a sandboxed asset route without script execution. Record the remaining concurrent symlink-component race if it is not eliminated by descriptor-relative operations; do not claim a path-based precheck is race free.

## Verification and handoff

Run offline tests for all five notebook crates, including loopback UI tests where localhost binding is available; formatter and owned Clippy `-D warnings`. Commit only owned paths; report SHA, example notebook, security boundary, commands/results, and remaining unsupported rich outputs. Stop for review.
