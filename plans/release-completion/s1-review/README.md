# Stage 1 integrated review and repair frontier

Review date: 2026-10-23, heartbeat at 22:30 UTC. All 12 original chats completed and committed. Permanent IDs were not listed by the app's recent-chat list (their persisted titles are empty); the coordinator resolved IDs by read-only local thread metadata matched to the exact worktree paths, then confirmed each completed turn using wait_threads. No duplicate workers were created.

## Integration

All twelve original commits are staged on riley/release-completion-plan. The root Cargo.lock was reconciled offline, including the new ffi-core/interop-worker crates and existing dependency edges. The first combined suite caught a real integration seam: cgx2c's embedded runtime file list omitted new scope.rs. Coordinator repair b453ea1 adds it; existing detached-memory compilation tests now pass.

The compiler installed acceptance runner gained optional --work-root (default /tmp) so a full /tmp quota does not prevent acceptance in another directory outside the checkout. This changes only test scratch placement.

## Verification

- Full workspace all-feature offline tests: **1013 passed, 0 failed, 6 ignored**. Notebook HTTP tests are included. Ignored/live-optional fixtures are not external acceptance evidence.
- Workspace all-target/all-feature Clippy with warnings denied: pass.
- Workspace formatting and diff checks: pass.
- Benchmark Python suite: **166 passed**; docs harness unit suite: **18 passed**.
- Default-feature release build: pass.
- Installed core acceptance outside checkout: **65 passed**, scratch under /var/tmp due /tmp quota.
- Native/Python/direct-f32 release boundary: pass; native result 5000, explicit Python 9, matrix [[58,64],[139,154]], no native/tensor Python runtime fallback.
- Public compiler readiness: toolchain identity and six public checks pass; blocked isolation/study statuses remain unchanged.
- Integrated executable documentation: **26 passed, 2 failed, 6 blocked, 8 future**. The two failures are stale expectations for now-supported unified build/emit commands. They remain explicit failures until repaired.

Logs and raw current reports: /home/john-riley/projects/lang/target/s1-*. Selected durable reports are in evidence/ below. Build target is the coordinator-private release-plan-review target; TMPDIR is disk-backed cgx2-wave21-tmp. /tmp quota prevented sandbox startup and the initial installed smoke; approved scoped execution and /var/tmp scratch resolved test execution. Worker worktrees were not cleaned or modified.

## Disposition

| Lane | Original commit | Review result |
| --- | --- | --- |
| core | 8fdbd78 | reviewed-bounded-delivery |
| memory | 26f654b | reviewed-bounded-delivery |
| ownership | f6fd925 | reviewed-bounded-delivery |
| native | 4bc27a9 | reviewed-bounded-delivery |
| python | 1d81231 | reviewed-bounded-delivery |
| worker | 055adc6 | reviewed-bounded-delivery |
| execution | b597342 | reviewed-bounded-delivery |
| notebook | ff3e715 | reviewed-bounded-delivery |
| verification | eab9acd | repair-required |
| analysis | 59446db | repair-required |
| evidence | d224bf1 | reviewed-bounded-delivery |
| docs | 9e96ac4 | repair-required |

Nine bounded deliveries are accepted with their documented library/source boundaries and external gates. Verification, analysis and executable documentation have precise follow-up work. All source is on the review branch so repairs can be tested together; this is not a release-ready baseline.

### Blocking verifier defect

scalar_v2/smtlib.rs duplicates nested division terms before solver startup. Actual bounded reproduction:
| Division depth | Source bytes | SMT bytes |
| --- | ---: | ---: |
| 1 | 104 | 3801 |
| 2 | 108 | 23325 |
| 3 | 112 | 158517 |
| 4 | 116 | 1103385 |

Input was a pure function requiring x == 64, ensuring result >= 0, returning x / 2 repeated; CLI domain x:64:64. Runs were limited to 512 MiB and 15 seconds. Solver deadlines do not protect encoder memory. The repair requires compact encoding and limits enforced before growth, with independent parity tests.

### Contract graph gap

effect-index indexes function bodies but omits requires/ensures-only helper edges. Repair parameter/result scopes and deterministic package graphs. No demonstrated capability/taint bypass is claimed.

### Documentation mismatch

Two expected-unsupported command cases need positive build/emit coverage and current installation instructions, not weaker output matching.

## Next ready wave

Three disjoint repair plans: [verifier-size](verifier-size/plan.md), [analysis-contracts](analysis-contracts/plan.md), [docs-current](docs-current/plan.md). Stage S2a remains gated until these AST-consumer changes are reviewed and the combined baseline checks pass. The next heartbeat can then launch the one coordinated language/type owner and any genuinely independent ready work.

## Retained boundaries

ScopedRoots exposes mutable Session access; replacing the borrowed Session is explicitly excluded by its documented guarantee. S2 lowering must preserve session identity for every live scope. Do not claim arbitrary .cg borrowing from the new library API. Python package calls may perform effects before conversion fails; they have no rollback/confinement guarantee. Worker restricted mode presently denies writes/sockets. Native dependency pins, foreign allocator ownership, GPU/autograd exclusions and actual dateutil availability remain explicit.

Trusted live solver, leak-enabled memory/FFI, delegated-cgroup scoring, quiet-host performance, independent safety review and real paired human/model studies remain open. No main merge or publication occurred.
