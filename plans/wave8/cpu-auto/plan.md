# CGx2 Measured CPU Auto-Selection Plan

> Implement in the attached worktree, commit only owned paths, and stop for user review. Do not delegate, merge, create another chat, or automate follow-up work.

**Goal:** Turn the checked i64 map policy prototype into an opt-in, locally calibrated CPU selector that chooses parallel execution only when conservative measurements justify it and correctness remains identical.

**Architecture:** Extend `cgx2-execution-policy`'s raw calibration tool with a versioned sample/profile pipeline. Validate raw sequential/parallel samples for the same machine, toolchain, worker count and input-size range; use separate calibration and held-out samples to construct conservative cost intervals and expiry. Add a profile loader that produces `Measurements` for `Policy::Auto` only after validation. `execute_map` still constructs its own pure `Workload` and falls back to sequential when missing/stale/noisy. No source-language optimizer or GPU backend is introduced.

**Tech stack:** Rust 1.98, standard library, existing policy/calibration crate; offline local CPU measurements.

**Specification:** `crates/cgx2-execution-policy/README.md`, `docs/validation-plan.md` parallel correctness/latency gates, `plans/wave8/README.md`.

## Owned paths and limits

- Own only `crates/cgx2-execution-policy/**` and this plan. Do not edit syntax, CLI, runtime, benchmark harness, root files, or other lanes. Preserve existing `execute_map`, `Policy`, `Capabilities`, and explicit-backend behavior or make a clearly versioned compatible API addition.
- Profile fields include schema version, workload identifier, OS/architecture/CPU or equivalent machine fingerprint, Rust/tool version, worker count, calibrated element range, sample counts, created/expiry times, raw-sample hash, conservative sequential and parallel cost intervals, and held-out regret. Reject a profile when stale, malformed, from a different worker count/workload/machine, outside size range, or lacking enough valid held-out samples. Do not invent an interval from one run.
- Preserve exact checked arithmetic, first-overflow index, and result ordering for sequential and parallel paths. No managed `Session` handles, aliases, observable effects, `no_gc`, floating-point reductions, or arbitrary `.cg` code may be fed into the reviewed pure map path. `Auto` chooses sequential on uncertainty or lack of a validated speed advantage; explicit unsupported `gpu` remains an error.
- Measure end-to-end time including thread launch/join and decision overhead. Calibrate on one set of input sizes and validate on separate sizes or repetitions; report raw samples and regressions. A local win is workload/machine specific, not global optimality or proof that CGx2 programs run on GPU.

## Tasks

### Profile validation

- [ ] Add red tests for empty/noisy/stale/wrong-machine/wrong-worker/out-of-range profiles and deterministic conservative interval construction. Inject clock/machine inputs in tests rather than relying on wall time.
- [ ] Implement versioned profile generation and loading from bounded raw samples, with schema/hash/provenance checks. Keep calibration opt-in and never run it implicitly for each `execute_map` call.
- [ ] Add an explicit command or example that reads a saved local profile and runs `execute_map` in `Auto`, printing decision, reason, cost status and result hash.

### Correctness and measured benefit

- [ ] Differential-test sequential, explicit parallel and calibrated Auto on varied input sizes, worker counts, overflows near chunk boundaries, and adversarial small tasks. Assert Auto fallback and no result divergence.
- [ ] Run one local release-profile calibration/holdout if the host has at least two CPUs; record sample path, machine, selected backend per size, regressions and uncertainty. If unavailable, report no real selection claim.
- [ ] Update README with profile trust, expiry and GPU absence.

### Verification and handoff

- [ ] Run `cargo test -p cgx2-execution-policy --offline`, `RUSTUP_TOOLCHAIN=1.98.1 cargo fmt --all --check`, and `RUSTUP_TOOLCHAIN=1.98.1 cargo clippy -p cgx2-execution-policy --all-targets --offline -- -D warnings`.
- [ ] Commit only owned paths; report exact measured scope and explain why general automatic parallel/GPU execution remains open.

**Acceptance:** Validated local measurements can enable the reviewed CPU parallel primitive when conservative intervals show benefit; incorrect, stale or missing profiles fall back safely.
