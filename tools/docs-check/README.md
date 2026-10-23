# Release documentation checker v1

Run with Python 3.10+ on POSIX. Only the standard library is used. The checker
does not build CGx2, discover a compiler on PATH, install packages, run a benchmark,
or start external studies. See [the guide](../../docs/first-release/README.md).

```sh
python3 tools/docs-check/check.py --help
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools/docs-check -v
```

Supply an independently reviewed executable and its approved lowercase SHA-256:

```sh
python3 tools/docs-check/check.py \
  --compiler /absolute/reviewed/cgx2 --sha256 APPROVED_SHA256 \
  --report "$PWD/target/release-docs/examples.json" \
  --work-root "$PWD/target/tmp-docs" --runnable-only
```

`--tool NAME=/absolute/PATH --pin NAME=APPROVED_SHA256` supplies optional
`native` (`cgx2c`), `package-runner` (`cgx2-native`), `rustc` (the real Rust
1.98.1 compiler executable), `notebook` (`cgx2-notebook`) and `verify`
(`cgx2-verify`). Missing tools block their cases. A supplied tool without a pin
is invalid configuration. A wrong digest fails before that executable's help
probe or fixture command. Symlink executable paths are resolved to the actual
executable and hashed there. Pins identify bytes; the caller still owns review
and distribution trust. A hash computed from an arbitrary download does not
establish trust. Companion subprocesses and dynamic libraries require their own
owner attestations; this checker is not a loader or OS sandbox.

The baseline executables have no common version command. Reports explicitly set
`version: null`, retain their `--help` banners and hash every tool. Native build
receipts additionally preserve `rustc -vV`, target, profile and embedded runtime
hash. The `rustc` argument uses the supplied pin rather than PATH discovery.

`--suite` defaults to `docs/first-release/expectations.json`. JSON must be UTF-8,
at most 1 MiB, schema version 1, with no duplicate or unknown keys or nonfinite
numbers. A suite has `baseline` (full commit), `fixture_root` (relative path)
and 1–200 `cases`. Each case contains:

- `id`, `requirements` (`R01`–`R15`), `scope` (`source`, `cli`, `library-only`,
  `external`), `disposition`, and `steps` (at most 20).
- `implemented` and `expected-unsupported` cases require executable steps;
  unsupported cases must assert at least one nonzero exit.
- `future` and `external` cases require empty steps, `reason` and an actionable
  `handoff`. These never become passing examples.

A step has `tool`, argument-vector `args`, exact integer `exit`, `stdout`,
`stderr`, and optional exact-length `diagnostics` (code, package-relative file,
zero-based half-open UTF-8 byte span or null). Streams each have exactly one
matcher: `equals` string, nonempty `contains` strings, or nonempty `json` object
subset. JSON matching is type-strict. Source diagnostic checks also reject
unexpected additional JSON lines. `{work}` expands to a private directory;
`{tool:NAME}` expands to a pinned tool and adds that prerequisite. `artifact`
executes only the regular file `{work}/program` produced in the same case.

Fixtures are copied into isolated per-case working directories; inputs with
symlinks/special entries or traversal paths are rejected. Commands run with that
copy as cwd so source locations stay package-relative. Environment `CGX2*`
overrides are removed; Rust 1.98.1, two Cargo jobs and the private target are set.
The per-process timeout defaults to 20 seconds, configurable in `(0, 60]`.
Each stdout/stderr stream is capped at 1 MiB; exceeding either cap or timing out
fails the step and kills its process group. This cleanup targets ordinary child
trees, not malicious daemonized executables. Only run reviewed tools/suites.

Reports contain commands, normalized outputs/errors, diagnostic observations,
tool and fixture hashes, statuses and counts. Ephemeral directories are replaced
with `$WORK`, configured tool paths with `$TOOL:NAME`; there are no timestamps or
durations. Stable observations produce byte-identical sorted JSON. Tool-produced
timing or other nondeterminism remains visible rather than being erased. Report
hashes cover pinned input executables. Temporary generated binaries embed build
roots and have no reproducible-byte attestation; their changing hashes are not
part of this deterministic documentation report.
Report
publication requires a new file in an existing directory, refuses traversal and
symlink components, rejects checker/fixture input directories, and uses exclusive
atomic linking. It does not overwrite a competing file. This is protection from
accidental overwrite, not a guarantee against hostile concurrent directory swaps.

Exit 0 means all runnable cases passed with `--runnable-only`, or all cases
passed without it. Exit 1 means a failed assertion or identity. Exit 2 means
invalid configuration, missing prerequisites, or incomplete reserved/external
acceptance. `--runnable-only` changes exit policy only: all blocked/future rows
remain in the report. `acceptance_complete` is always false because this checker
cannot certify all release gates. No expectation update/auto-bless mode exists.

The unit suite uses explicitly synthetic compiler processes to test the harness;
the [owned evidence](../../docs/first-release/EVIDENCE.md) separately records real
reviewed CGx2 executions. No Rust source or manifest is changed by this tool.
