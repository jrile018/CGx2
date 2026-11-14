# Actual Rust budget source → Lean feasibility spike

**Result: partial, with a successful bounded source-connected proof.** The official pinned Charon/Aeneas route extracts the actual runtime budget code. A mechanically selected, byte-exact `Budget` declaration and `impl Session` translates to Lean, compiles without warnings, and supports six checked lemmas, including continuation of the generated admission loop body and nonfailure of the generated charging loop body under explicit iterator observations. This is not a proof of the entire runtime, complete Vec traversal, guard lifecycle, ownership system, or compiler.

Reproduce **offline**, using the isolated tools left in `/tmp/cgx2-arch-review-proof`:

```sh
python3 research/architecture-review/proof/verify.py
```

`--evidence-only` validates frozen inputs and saved evidence without claiming a fresh compiler/proof run. `--report /tmp/proof-replay.json` saves fresh commands, exit codes and logs. If isolated tooling is removed, `setup.py --allow-network` describes/recreates the pinned local setup; that optional installer is separate from the offline checker and was assembled from the executed setup commands, not itself rerun end-to-end. The repository verifier is intended to be run from the review worktree.

## Inputs and extraction boundary

Baseline commit: `4ed17189867b11e2a3c8280f02a05e3017f3ae5a`. `pins.json` and `source/kernel-source-map.json` record the actual `crates/cgx2-runtime/src/budget.rs` SHA256, source byte offsets and hashes for each selected span. `verify.py` requires agreement with `research/architecture-review/input-manifest.json`, reconstructs those spans from the current real source, and rejects drift. No crate, Cargo manifest, historical formal package or frozen WIP file was edited.

Three progressively narrower attempts are preserved:

| Attempt | Actual outcome | Coverage |
| --- | --- | --- |
| Unchanged runtime `src/lib.rs`, all modules | Charon exits 0 and produces 47MiB LLBC; Aeneas exits 2 | No checked whole-runtime translation. Actual graph early returns, outer-loop continues and nested-loop returns are unsupported; translation subsequently hits an Iterator signature sanity check. |
| Unchanged complete `budget.rs` through `#[path]` | Charon and Aeneas exit 0; Lean exits 1 | Generated DerefMut and Drop definitions contain extra backward closures incompatible with the bundled trait model signatures. Raw generated file and errors retained. |
| Byte-exact selected Budget + impl Session | Charon, Aeneas and Lean exit 0 | Checked generated arithmetic and loop bodies. No guard trait code is selected. |

The final harness has only `Session { budgets: Vec<budget::Budget>, charged_bytes_v2: u128 }` and a single-constructor `AllocationError::BudgetExceeded`. It keeps the original selected code byte-for-byte, including `limit.saturating_sub(spent)`, `spent += bytes`, and telemetry `saturating_add(bytes as u128)`. It adds no copied replacement implementations. Mechanical selection and source-map verification are part of the trust boundary. Error constructor meaning is projected; full enum layout/discriminants, other Session fields, their invariants and their interaction with failures are not related by a checked refinement theorem.

`generated/BudgetKernel.lean` is untouched translator output. The exact generated source SHA256 and raw/normalized LLBC SHA256 are in `pins.json`. LLBC replay removes only the serialized output-directory metadata for its comparison; generated Lean is compared byte-for-byte. The generated source contains **two opaque axioms**, the shared and mutable Vec `IntoIterator::into_iter` operations with allocator arguments. The full-file projection has a third for `Vec::pop`. They are listed in the artifacts and **are not dependencies of the six checked lemmas**, which quantify their required iterator observations directly. No property of these opaque Vec operations is established.

## Checked claims

`source/Admission.lean` imports the actual generated `BudgetKernel` module. The fresh proof command uses `-DwarningAsError=true` and prints each theorem's axioms.

1. `saturating_sub_value`: the generated branch's machine-sized saturating subtraction has Nat value `limit.val - spent.val`.
2. `admission_equivalence`: with reachable guard invariant `spent.val ≤ limit.val`, admission `¬ bytes > saturating_sub limit spent` is equivalent to `spent.val + bytes.val ≤ limit.val`. The reachable invariant is required: when `spent > limit`, zero bytes can pass saturating subtraction while the guard remains invalid.
3. `generated_admission`: if the library iterator's `next` returns this guard and successor iterator, the **actual generated one-step check body** returns `.ok (.cont successor)` for admitted bytes.
4. `admitted_addition`: the identical guard/byte pair's checked machine addition succeeds, yields the ordinary sum, and remains within its limit. This bounds it below the machine maximum; debug overflow cannot change control flow for this operation under these assumptions.
5. `target_max`: conditional on `System.Platform.numBits = 64`, the library Usize maximum is `18446744073709551615`. The bit-width constant is opaque in Lean, so no new axiom or native-decision proof was introduced. The executable separately evaluates it to 64; target correspondence is an explicit environmental assumption.
6. `generated_charge_no_overflow`: if mutable iterator `next` returns the **same reachable guard and bytes used for admission**, the actual generated one-step charging body satisfies the library total-correctness/nonfailure predicate. This does not establish the complete loop or reconstruction of a Vec.

The proof depends only on Lean's standard `propext`, `Classical.choice`, and `Quot.sound`, as individually printed. It has no `sorry`, `admit`, custom axiom, `sorryAx`, `Lean.ofReduceBool`, or Vec axiom dependency. The translator's generated module itself intentionally declares the two opaque Vec axioms: compilation does not turn them into proved implementations.

A fresh source-level negative control changes the selected actual predicate from `>` to `>=`. Charon, Aeneas and Lean still succeed, but the unchanged admission theorem fails (`omega` supplies the exact-boundary counterexample). The offline verifier reruns this sensitivity check and requires the proof failure. Failed exploratory proof scripts/logs are retained as failures; only the final six-lemma file is claimed checked.

## Machine model and trust

The extraction target is explicitly `x86_64-unknown-linux-gnu`, little endian, pointer size 8 bytes, with `-C overflow-checks=yes`. The LLBC records `overflow_checks: true`, `ub_checks: true`, and `contract_checks: false`. `evidence/target.json` preserves target information and `evidence/versions.log` includes rustc's full target data layout. Arithmetic is Aeneas fixed-width `Std.Usize`/`Std.U128`, not an unbounded-Nat replacement for Rust arithmetic. Nat-valued lemmas prove bounds for those fixed-width values. `u128` telemetry remains the actual generated saturating expression; its full end-to-end value relation has not been proved here.

Pins:

- Aeneas official `nightly-2026.11.13-0d21a9e`, source `0d21a9ec7bbbb110110cacc14464a47fd71c8705`.
- Bundled Charon 0.1.315, source `454586e9b770231e66d2062993e632dfc06463e2`, matching the pinned official `charon-pin`.
- Rust `nightly-2026-09-17`, rustc `1.100.0-nightly (923c95cdf 2026-09-16)`; isolated minimal install plus `rustc-dev` and `rust-src`.
- Lean `4.36.0-rc2`, compiler commit `b35cb10cf2ced51a2fd1880cfdbf75c71ab7a35a`.
- Mathlib `9d581dfff2e41de989072ea63abf3daf69311520`; all transitive package source pins are in `evidence/aeneas-lake-manifest.json`. Official compiled Aeneas/Mathlib caches were used.

Trusted stages/assumptions remain: Rust compiler MIR correctness and configured target/panic/overflow semantics; Charon's MIR-to-LLBC extraction; Aeneas's symbolic borrow translation and LLBC-to-Lean generation; Aeneas scalar/iterator/loop library models; the byte-selection/context projection; Lean kernel plus trusted imported compiled artifacts/toolchain and their provenance; correspondence of the observed 64-bit Lean platform with the Rust target. Executable/archive hashes and official source pins improve reproducibility and provenance; they are not a verification proof of the translators, compiler, caches or host ABI. Host allocation failures, unsafe external behavior and foreign heaps are outside the proved slice.

## Setup evidence and actual blockers

The official documented route is `charon ... --preset aeneas`, then `aeneas -backend lean FILE.llbc`, then Lean using the Aeneas library. Pinned official documentation and `charon-pin` are archived under `docs/`. Initial unprivileged network resolution failed; the authorized isolated network fetch succeeded. GitHub's `/releases/latest` returned 404 because the usable Aeneas artifacts are prereleases; querying `/releases` found the pinned official binaries. Thus missing PATH executables were not treated as the final finding.

The official bundle supplied Aeneas, Charon and charon-driver, avoiding OCaml installation. Isolated rustup installed the required compiler, then returned 1 on a final self/proxy check because the isolated Cargo home lacked a rustup binary; the compiler itself was installed and subsequently used successfully. Recreated setup copies the existing rustup executable only into that isolated Cargo home. Charon warns that optional Miri/full-MIR sysroot setup is unavailable and falls back to rustc's default sysroot. This spike uses that recorded fallback rather than claiming a Miri/full-MIR extraction.

An initial lean-only installation without `.olean.private` files failed to import Init; adding the official private files fixed it. A first interpreted Mathlib cache invocation lacked an external native implementation; building the official cache executable fixed it. Cache and generated-file root/import errors were resolved before the passing final checks. All command outputs, including these failed attempts, are preserved. Setup/build concurrency was bounded with `LEAN_NUM_THREADS=2`/`CARGO_BUILD_JOBS=2` for the final setup/replay; early small Cache-library compilation used Lake defaults before that bound was applied. Final isolated footprint was approximately 6.6GiB after hashing/removing download archives, within the coordinator's extended 8GiB cap. No shared opam/Python/Rust/Lean environment or root installation was changed.

Concrete remaining choices:

- Keep product code unchanged and repair/upstream the Aeneas guard trait/backward-closure mismatch; add proper Vec iterator models for the exact allocator-aware Rust signatures. This keeps API semantics but requires translator/library work and trusted correspondence audits.
- Introduce an explicitly reviewed pure budget-admission/update kernel or slice-based implementation for future verification. That can reduce dependence on Vec internals, but changes product structure and requires a real contextual refinement/integration proof. This spike does not perform that redesign.
- Retain the source-selected slice as a documented proof boundary while proving the iterator/Vec observation relation, full-loop induction, and contextual projection separately. It establishes useful arithmetic facts now but leaves runtime invariants open.
- Explore a different Rust verification route (e.g. an SMT-based Rust verifier) for runtime safety obligations. That changes proof/trust tooling and does not itself satisfy Rust-to-Lean extraction.

## Unproved obligations carried forward

Reachability `spent ≤ limit` must be established for actual Session construction, nested guard insertion, all charge sites, and RAII exit. Full Vec traversal must read and update exactly the same guard set with identical bytes and unchanged limits. There is no unchecked mutation allowed between admission and charge in that proposed argument; current lemmas merely assume the relevant observation.

`Session::try_no_gc` additionally includes budget-table capacity checks, `try_reserve_exact`, potentially larger actual capacity, outer guard bookkeeping charges and failure paths. Compatibility `no_gc` has distinct historical policy. Guard drop uses `pop`. None of these is proved by the selected kernel. Allocation failure diagnostics/counters may mutate, and projecting counters out is unsafe if an overflow could alter control flow: this result only excludes overflow in the guarded addition under its explicit preconditions. Logical slot charges (including the runtime's 64-byte slot charge) are not RSS bounds.

The original `formal/memory-guarantees` package remains the historical 13-theorem Nat/fixed-guard reference model. Its source identity is checked; its Lean4.19 proofs were not rerun as part of this source spike and are not relabeled implementation proofs. The unfinished ownership overlay is context only and supplies no accepted source-proof coverage. Ownership/compiler/foreign/GPU stages remain separate work.
