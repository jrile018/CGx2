# Release-build float32 CPU matrix baselines

**Goal.** Measure committed f32 source/direct routes with optimized build provenance. Wave 22 cold source observations used debug Cargo binaries against Release C++; warm direct C-facade timing excluded Rust loader checks.

**Ownership.** Edit only `bench/**`. Do not edit compiler, bridge, binding, LibTorch, notebook, root Cargo files, shared docs, or hidden LLM holdout. Use the common Wave 23 base; do not depend on concurrent Python isolation.

## Deliverable and contract

1. Add explicit build-profile selection, defaulting newly built source/binder binaries to release. Record flags/profile, executable hashes, C++ Release flags, toolchain, package/closure hashes, dtype, threads, device, shapes, warmup, repetitions, deadlines, and exclusions. Externally supplied binaries require verifiable provenance or an unverified-profile label; filenames do not prove optimization. Preserve historical JSON and f64 harness compatibility.
2. Add explicitly selected f32 using the frozen v3 facade, `cgx2_torch_matmul_f32`, and source profile. Require a matching pinned real Torch 2.11 CPU f32 package. Keep cold source shapes within 32x32/1,024 elements; label larger 128x128/512x512 warm direct-facade/C++ cases separately. Use distinct dtype-aware create/copy functions and equivalent input creation, matmul, copy, release, threads, and layout. Never relabel f64 samples or compare precision modes as equivalent workloads.
3. Before timing, round inputs consistently to f32 and compare each output with an independent scalar reference using fixed documented `atol=1e-5`, `rtol=1e-5`; compare routes within the same tolerance. Retain raw output hashes as provenance. Tolerance agreement does not require identical raw or quantized hashes. Nonfinite, wrong-shape, or out-of-tolerance results invalidate the report; do not loosen tolerances after failure.
4. Preserve raw wall/CPU/RSS samples, failures, stopped runs, and unavailable routes. Require 20 completed warm samples for p95 and five for the existing bootstrap median interval; cold claims require independent process repetitions. Label source validation, loader, and adapter/copy phases measured or unmeasured; do not subtract incomparable whole-process runs. Record the base CLI's actual Python linkage, currently present, and distinguish direct C-facade timing from full pinned Rust source execution.
5. Compare release/debug cold startup only with identical inputs, package, toolchain, and controls, and enough samples for stated uncertainty; otherwise report observations without a speed claim. Optional Python Torch requires a working matching interpreter/build and remains separate. This work does not establish general inference, GPU, source parallelism, or LLM targets.

## Verification and handoff

Run all benchmark tests, wrong-dtype/mismatch/timeout/tolerance/profile-provenance cases, and public syntax readiness. Run a real local f32 benchmark if the pinned package exists; otherwise record unavailable with an exact rerun command. Commit bench files with preregistered settings and raw report. Report build provenance, sample counts/intervals, numeric checks, missing routes, and unscored general targets. Stop for review.
