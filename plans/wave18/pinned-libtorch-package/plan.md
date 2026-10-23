# Copied pinned LibTorch package and direct CPU evidence

**Goal.** Move from a package-preparation recipe to a reproducible, copied deployment of the direct CGx2 C++ facade. The CGx2 adapter talks to LibTorch without a Python interpreter or `libpython` intermediary.

**Ownership.** Edit only `crates/cgx2-bind/**` and `crates/cgx2-libtorch/**`. Do not edit source language, notebook, root Cargo files, shared docs, or other lanes.

## Deliverable

1. Test `prepare_pinned.py` when the input facade and LibTorch libraries are outside the destination. Verify the copied package's schema-2 lock paths, SHA-256s, SONAME/`DT_NEEDED` closure, ABI metadata, symlink policy, and `Torch::open_pinned` through its normal deployment path. Prove idempotent locks for unchanged inputs. Add negative copied-package tests for changed bytes/inode, missing or ambiguous dependency, wrong ABI/version, and unsafe loader search path.
2. If a compatible local CPU LibTorch installation exists, run the exact preparation command and ignored real test against the copied package. Record actual version, compiler, ABI flag, package artifact hashes, command, create/add/shape/copy/release outcome, and mapped-library check for `libpython`. A mock run must be labeled mock and cannot substitute for real evidence. Do not download unpinned artifacts or invent hashes; if unavailable, report the real test as unrun.
3. Bound the facade's output-copy and error-string handling before host allocation where possible. Exercise normal and failed copy, double release, and error-returned nonnull-handle paths. State the trust limit of a malicious ABI-violating binary and that static closure checks cannot rule out later `dlopen`.
4. Update owned README files with exact reproduction steps and distinguish direct LibTorch C++ access from Python-library compatibility; this lane does not wire `.cg` source tensor calls.

## Verification and handoff

Run offline `cgx2-bind` and `cgx2-libtorch` tests, copied-package positive and negative fixtures, formatter, and Clippy on owned packages. Run the real ignored test only if the verified local installation is available, recording a deterministic artifact inventory. Commit only owned paths and no host-specific binaries or absolute lock files. Report commit SHA, file list, real versus mock evidence, exact commands, and remaining foreign-lifetime limits; stop for review.
