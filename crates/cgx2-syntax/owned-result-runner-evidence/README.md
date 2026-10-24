# Owned Result runner repair evidence

Repair base: `3e5be4d4907debad1ae5fa8bdc836208255ddb4d`. This Python-only
repair changes the two owned evidence runners and this new directory. Rust,
fixtures, expected/positive JSON and historical evidence remain unchanged.

`supervisor.py` drains nonblocking pipes using selectors. Each read requests
at most the stream's remaining allowance plus one detection byte; only the
allowance is retained. Output never goes through temporary capture files.
Overflow or wall timeout immediately kills the spawned process group and
waits for the direct child. Leader exit also kills remaining group members,
including descendants holding output pipes. Pipe cleanup has a one-second
fallback for descriptors held outside the group. A supervisor exception also
kills the group and waits for its child.

The inner runner retains at most 1 MiB of raw bytes **per stream**, with a
120-second wall limit, 60-second per-process CPU limit and 2 GiB per-process
address-space limit. The outer verifier uses a single combined 8 MiB stream,
900-second wall/CPU limits and 4 GiB per-process address space. The outer log
is written only after bounded capture finishes. `verify.py` honors private
`CARGO_TARGET_DIR` and `TMPDIR`, retaining its previous worktree-local defaults.
These are process-group and per-process limits, not aggregate cgroup isolation;
descendants that deliberately escape the group are outside this guarantee.
Grandchildren are killed, but only the direct child is waitable by the runner;
orphan zombies must be reaped by the host's init process.

Command records preserve the actual exit (negative for signals), reason
(`exit`, `signal`, `output_overflow`, `timeout`, `spawn_error`, or
`pipe_cleanup_timeout`), signal when applicable, overflow stream, byte counts,
hashes and duration. The inner runner also saves the expected exit/diagnostic,
and appends failure evidence before asserting. Only a normal `exit` can satisfy
an expected program error. UTF-8 is decoded with replacement for display;
invalid raw output is also retained exactly as base64. JSON encoding expands
raw bytes by a bounded factor. Outer logs retain the original raw bytes.

Reproduce from the worktree root:

```sh
python3 crates/cgx2-syntax/owned-result-runner-evidence/validate.py
```

The driver uses Rust 1.98.1, `CARGO_BUILD_JOBS=2`, private
`target/owned-result-runner` and `target/tmp-owned-result-runner`,
`LIBRARY_PATH=/tmp/cgx2-wave22-lib`, and offline locked dependencies. It bounds
each validation command to 900 seconds, 4 GiB per-process address space and
8 MiB combined output. Exact argv, environment, exits, reasons, durations and
raw log hashes are in `results/checks.json`; logs are losslessly gzip-compressed (mtime zero).
Input/script/binary/evidence hashes
are in `results/hashes.json`. The product report preserves all 584 original
checks and their observed diagnostics/accounting under `results/product.json`.

Results on this repair:

- Ten executable synthetic test methods pass, including both separate-stream
  and combined-stream success/exact-cap, nonzero errors, stdout/stderr overflow,
  simultaneous output, timeout, normal/signal termination, descendant cleanup,
  invalid UTF-8, missing executable and failure-report persistence. Children
  attempt at most 128 KiB output and one fork each, self-terminate after two
  seconds, and have CPU/address-space/file-size limits. Reporting tests mock
  the command execution only; live supervision cases use real children.
- Private offline all-feature workspace build passes.
- All-feature syntax tests: 45 passed, zero failed; doc tests pass.
- Unchanged owned Result product suite: 584 passed, zero failed.
- `cargo fmt --all --check` and `git diff --check` pass.

The first staged diff check found a trailing blank line in the exact Rust test
log. Logs are stored as lossless gzip bytes so verification does not require
altering raw diagnostics to satisfy source whitespace rules. The final staged
diff check passes.

Full workspace tests and Clippy were not repeated for this Python-only repair;
the coordinator's historical results in the review ledger are separate evidence.
No compatibility repair from the parallel lane is included. Full ordinary
memory, aggregate containment, leak-enabled audit, and release acceptance remain
open. No packages, host security settings or external services were changed.
