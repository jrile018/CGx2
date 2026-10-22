# Benchmark containment and task review packet

**Goal.** Prevent scored candidate execution unless the whole worker, not merely each process, has verified containment. Prepare a task-quality packet for independent human review; do not create fake model evidence.

**Ownership.** Only `bench/**`. Do not edit compiler/runtime, root Cargo files, shared docs, or other lanes. Private oracle data must stay outside candidate-visible workspaces.

## Deliverable

1. Require an aggregate CPU/memory/process limit, such as a dedicated cgroup v2 subtree with verified controllers and cleanup, in addition to per-process limits. Probe host capability before candidate launch. If unavailable, return a structured `unsupported_isolation` reason and execute no untrusted source. Avoid permissive fallback to trusted smoke or a soft wall timer.
2. Expand adversarial tests for child-process resource multiplication, inherited descriptors, symlink/path traversal, private fixture reads, network access, and cleanup after timeout. On a host without required namespaces/controllers, test fail-closed behavior and label live adversarial results unrun.
3. Produce a concise review packet for the current task candidates: specifications, hidden-oracle boundaries, language equivalence risks, leakage checks, independent reviewer fields, and frozen toolchain/hash pins including Rust. Preserve `unrun` until humans approve tasks and two model families complete paired trials. Do not expose hidden answers to candidate workspaces.

## Verification and handoff

Run `python3 -m unittest discover -s bench/tests -q`, schema/hash checks, isolation probe, and available adversarial fixtures. Report exact host capability, unrun gates, and commit ID. Stop for review.
