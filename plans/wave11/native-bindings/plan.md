# Pinned native binding package preflight

> Work in the attached worktree, commit only owned paths, and stop for user review. Do not delegate, merge, start another chat, or automate follow-up.

**Goal:** Make an explicit native binding package reproducible and inspectable before execution. Add a bounded offline `bind check` preflight for one C ABI fixture and a LibTorch declaration, without claiming that arbitrary declared signatures are safe to call.

**Architecture:** Extend `cgx2.toml` with a versioned, strict binding section or a separately referenced declaration file, plus a lock containing exact artifact hash, ABI/platform/version, symbol shapes, ownership/release, effects/capabilities and approved route. A new `cgx2-bind` crate/binary checks this lock and installed artifacts without invoking library metadata or user code; the existing `cgx2-ffi-native` API remains unsafe until a separately reviewed adapter can establish physical signatures. Support path confinement and deterministic serialization.

**Tech stack:** Rust 1.98; `cgx2-package`, new `cgx2-bind` crate; offline Cargo. Reuse current SHA-256 and manifest parsing patterns where possible.

**Spec:** `docs/interoperability-design.md` foreign contract v1 and trust boundary; `docs/interoperability-top10.md` explicit `bind add/check`; `docs/validation-plan.md` interoperability and portability gates.

## Constraints

- Own only `crates/cgx2-package/**` and new `crates/cgx2-bind/**`. Do not edit `cgx2-ffi-native`, `cgx2-libtorch`, source grammar/checker/CLI, root manifests/lock, docs/plans. No implicit package install, network access, source import resolution or Python fallback. Do not call `dlopen` during `bind check` of an untrusted artifact.
- Keep v1 scalar local packages valid. Version the new schema explicitly and reject unknown fields, path traversal, symlink escape, hash mismatch, wrong OS/architecture/ABI, unsupported type/layout/calling convention, missing ownership destructor or incomplete effect/capability declaration. On supported Linux artifacts, inspect exported symbol names without loading code; report symbol inspection unavailable elsewhere. The exact hash pins bytes, not a proof that a C symbol has the declared physical signature. A handwritten declaration has untrusted status until reviewed or marked `unsafe foreign`; no ordinary safe `.cg` call may be unlocked by a user assertion alone.
- Demonstrate one tiny compiled C fixture and a LibTorch artifact declaration. If LibTorch is not installed, `bind check` reports unavailable with a stable diagnostic; it must not produce a passing torch result. The binding routes must remain explicit (`native-c`, `libtorch`, `python` distinct) and deterministic in text/JSON API output.

## Tasks and checks

1. Specify frozen v2 manifest/lock fixtures and negative cases. Add tests for valid local scalar package compatibility, byte-identical lock output, offline artifact hash and declared identity, missing symbol where static inspection is available, missing artifact, symlink/path attack, version/platform/ABI mismatch, unknown effects and ownership errors.
2. Implement the preflight and `cgx2-bind check PACKAGE --format text|json` command. If a separate binding manifest is simpler, make its relationship to `cgx2.toml` explicit and test it. Do not construct a safe function pointer from declarations in this wave.
3. Run `RUSTUP_TOOLCHAIN=1.98.1 cargo test -p cgx2-package -p cgx2-bind --offline`, format and targeted Clippy `-D warnings`, plus one CLI fixture and an unavailable LibTorch check. Update owned READMEs and commit only owned paths.

**Acceptance:** The package is pinned and can be rejected offline before any native code executes; this is preflight, not source-level native library use.
