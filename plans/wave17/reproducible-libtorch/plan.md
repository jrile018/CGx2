# Reproducible pinned LibTorch evidence

**Goal.** Wave 16 reported a successful local LibTorch 2.14 CPU run, but its temporary schema-2 package and exact pins were not committed. Make the direct native route reviewable and repeatable without using Python as a runtime intermediary.

**Ownership.** Only `crates/cgx2-bind/**` and `crates/cgx2-libtorch/**`. Keep source-facing `Torch::open_pinned(root, binding_id)` compatible. Do not edit CLI, compiler, notebooks, root Cargo files, or shared docs. Do not vendor large LibTorch binaries or embed machine-specific absolute paths as universal pins.

## Deliverable

1. Provide a deterministic, bounded per-host preparation command or checked script under owned paths that takes an exact LibTorch installation and facade build, resolves the static ELF `DT_NEEDED` closure, records canonical paths, SONAMEs, dependency names, sizes and SHA-256s, rejects ambiguous/missing/symlink-substituted entries, and produces schema-2 binding/closure locks. Repeat generation against unchanged inputs must yield identical locks; different hosts may legitimately yield different hashes. Record facade SHA, compiler, `_GLIBCXX_USE_CXX11_ABI`, `TORCH_CXX_FLAGS`, torch version, platform, RUNPATH and exact command. The generated locks must pass existing schema-2 preflight and `Torch::open_pinned`; make no automatic network download.
2. Add a reproducible positive CPU create/add/shape/bounded-copy/release command using the generated package and negative ABI metadata/version/ambiguous SONAME/changed inode or closure fixtures. Check absence of mapped `libpython` during the direct path. A mock-only pass must be labeled mock; a real run must include command, artifact hashes and outcome. If no compatible LibTorch is available, mark real test unrun. Static closure verification does not cover later runtime `dlopen` or prove foreign behavior.
3. Audit error-returned nonnull handles and duplicate aliases. Define what can be rejected before foreign release and what remains impossible to protect against in a malicious ABI-violating binary. Preserve bounded host copies and accurate CGx2-versus-foreign memory attribution.

## Verification

Run offline tests of both crates, formatter, targeted Clippy with warnings denied, mock negative fixtures, and real compatible LibTorch command only when available. Report reproducibility artifact, remaining constructor/TOCTOU/hostile-code limits, and commit ID. Stop for review.
