# Owned symbol compatibility repair

Reviewed base: `3e5be4d4907debad1ae5fa8bdc836208255ddb4d`. This bounded repair
restores ordinary owned-source names `Result`, `AllocationError` and `try_owned`
without changing the runtime, frozen public APIs or authenticated v2 analysis.
See the symbol policy in `../S2B-OWNED-RESULT.md`.

Authoritative outcomes and attempt reconciliation: [summary.json](summary.json).
Exact commands, environment, exit codes, counts and original log hashes are in
[evidence/verification.json](evidence/verification.json), supplemental command
records and [the permitted workspace rerun](evidence/workspace-host-rerun/verification.json).
Logs are lossless gzip: `gzip -dc evidence/NAME.log.gz`. Product JSON retains
individual commands, diagnostics, outputs, source/runtime hashes and cleanup
observations. `SHA256SUMS` inventories this delivery's files and producer inputs.

Results:

- 195 compatibility product checks across 12 fixtures: interpreter, native debug,
  native release and independently compiled Rust. Ownership Python rejection
  preserves the prior artifact; ordinary copied user-function Python output is 42.
- 575 unchanged Result checks pass. The original 584-check runner was also run
  unchanged and fails at its now-obsolete `shadow-builtin` negative. Its nine
  rejection/artifact assertions require review under the compatibility repair;
  they are explicitly excluded in `replay_result.py`, never counted as passes.
  A Rust regression proves that exact historical source now compiles through
  both APIs and executes its frozen result 0.
- Ordinary-owned 256; seven foundation suites 159/278/207/118/88/31/104;
  legacy graph 99, native guards 518, report layouts 265: all pass.
- Full workspace: 1,237 passed, zero failed, six ignored, 266 suites. The first
  sandbox run had 34 existing HTTP/worker permission failures; the permitted
  host rerun passes with identical offline locked test arguments.
- All-target/all-feature Clippy with `-D warnings`, formatting and diff checks pass.
  No dependency or root-lock reconciliation was required.

The sema regressions also preserve v1 model/evidence projection, copied nominal
parameter/results, old source expansion bounds, resolved user calls inside a
real guard module and real builtin allocation with a nominal `Result` payload.
Ambiguous explicit Result error types still reject, and actual new source still
rejects at the schema-1 entry. Existing move/loan/provenance negatives pass.

Reproduce from this worktree:

```sh
python3 crates/cgx2-syntax/owned-symbol-compatibility-evidence/verify.py \
  --output-dir "$PWD/target/tmp-owned-symbol-compatibility/reproduction"
```

`verify.py` sets Rust 1.98.1, two Cargo jobs, private Cargo/temp directories,
`LIBRARY_PATH=/tmp/cgx2-wave22-lib`, offline locked dependencies and two linker
threads. `--only NAME` replays one named check. Full workspace tests need the
host's existing loopback/worker execution permissions. The parent bounds wall
and CPU time to 900 seconds, address space to 4 GiB, sampled aggregate log output
to 8 MiB, sampled descendant stdout/stderr files to 1 MiB and the process tree
to 128. Samples occur every 50 ms; the inherited 512 MiB file-size hard ceiling
also bounds overshoot between samples while allowing compiler artifacts. This
is bounded validation, not a claim that the independently owned historical
runners have acquired a strict live 1 MiB output contract.

Historical files remain unchanged. Failed attempts are retained and explained
in `summary.json`; generated specialization/visibility binaries stay under
private `target/`, with exact artifact hashes exported here. The scripts do not
install dependencies or change security settings.

Full ordinary memory, R01-R15, S3/S4, external leak-enabled acceptance,
independent audit and performance/human/model studies remain open.
