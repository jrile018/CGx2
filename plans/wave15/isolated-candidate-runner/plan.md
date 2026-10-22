# Isolated benchmark candidate runner

Use the attached worktree; read shared Wave 15 references and this plan. Commit only owned paths and stop for review. Do not run untrusted model submissions on the coordinator host.

**Goal.** Wave 14's private oracle CLI rejects submitted workspaces by default because source regex and resource limits are not a sandbox. Build a scored-submission path that either establishes filesystem/network isolation with auditable evidence or refuses to run. Human task review and paired model runs remain separate gates.

**Ownership.** Only `bench/**`. Do not edit compiler/runtime/notebook code, root Cargo files, shared docs, plans, or coordinator ledger. Keep private test cases and reference overlays outside candidate-visible mounts and prompts.

## Deliverable

1. Define a concrete Linux worker profile: read-only compiler/toolchain mounts, a copied candidate workspace and writable scratch, no repository/private fixture mount, no network, fixed CPU/memory/file/output/time limits, dropped privileges, controlled environment, and no writable host path. Implement a launcher using a real OS sandbox when available; on hosts without required namespace/seccomp/Landlock capability, return `unsupported_isolation` before compile or run. An argument filter or `--trusted-local-smoke` is never scored isolation.
2. Separate hidden oracle from candidate process: oracle passes generated inputs and validates captured output outside the sandbox; candidate cannot read expected answers, acceptance source, task cards marked private, or host home. Capture isolation configuration, worker/toolchain identity and hashes in evidence. Verify candidate path/language and immutable task pins before launch.
3. Add adversarial fixtures that attempt private-file read, alternate file APIs, process execution, network connect, symlink traversal, and output/resource exhaustion. Demonstrate each is denied by sandbox rather than source regex. Keep trusted local reference/starter smoke working. If the host cannot run the sandbox, test configuration and fail-closed behavior without claiming live adversarial success.
4. Preserve `unrun` evidence and blank human review fields until independent review and actual paired model trials. Do not call two task candidates a benchmark win.

## Verification

Run `python3 -m unittest discover -s bench/tests -q`, packet schema/hash validators, and an isolation capability probe. On a capable host run adversarial and positive candidate cases; otherwise record `unsupported_isolation` with exact missing capability. Report remaining private-oracle and model-run gates and commit SHA.

**Acceptance.** Untrusted candidate code never executes outside a verified isolation profile; unsupported hosts refuse scoring.
