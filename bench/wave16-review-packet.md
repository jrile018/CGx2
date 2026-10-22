# Wave 16 candidate task review packet

**State:** `unrun`. These are two candidate tasks, not an approved holdout or
evidence of a model outcome. The independent review fields in
`fixtures/task-candidates/v3/task-cards.json` remain null, `pack.json` has no
registered model runs, and the current host lacks delegated cgroup v2 `cpu`,
`memory`, and `pids` controllers. The scored runner therefore returns
`unsupported_isolation` before reading an oracle or launching candidate code.

## Material and toolchain pins

The following SHA-256 values are the fixed material set for this review packet.
Changing any value requires a new review revision; it must not silently alter
an already approved task or paired trial. All paths are relative to this
repository except the Rust toolchain binary. No model trial is authorized by
this pin set. The trial protocol's still-null fields in `pack.json` require a
separate freeze after independent approval on a capable execution host.

| Material | SHA-256 or version |
| --- | --- |
| Candidate `pack.json` | `ac594071696480c98bac5a62951054d5e2cf3187c89440df8f5a3592157e083d` |
| Reviewer `task-cards.json` | `3ee56b72c0c119de8787b2c60ee9e6deb9ee7c787386d2f84aee47c0f8ad2671` |
| `quote-credit-v3` statement | `40abb7ff94f50ba35f464c0b4aec46cbf056df25a465e34baa973a6ab3748323` |
| `grid-quote-v3` statement | `38badb5a805e41e6e847e1a60dfa3d455b490293d0891b9673d047ae906cebd0` |
| Private acceptance script, reviewer use only | `17242900d12ba4a38862414bf40f17fdf65a6922a2ab73656d3cc777c1c45903` |
| Task-pack schema | `43607e2381e73c4cc1f38c3ef392f5e2259d5cc5f53709042b683535e25ee160` |
| `Cargo.lock` | `974d2ff633c12611f8323ea2eb4761959fad75c14baf7d7e628feccbbef0ed5a` |
| Locally built `target/debug/cgx2c` | `8ef250cd91ed17aa7920015be0a78b06fd0bd425edbd887e77343dcdf49fdce4` |
| Rust compiler binary, `rustup which rustc` | `859254978c0a0402c32f949f6de0d99aee73be8d15f45aac00ae1448aac51e74` |
| Rust compiler version | `rustc 1.98.1 (48a229cea 2026-09-01)`, `x86_64-unknown-linux-gnu`, LLVM 22.1.8 |
| Cargo version | `cargo 1.98.1 (797e8a9bc 2026-08-05)` |

The candidate pack additionally pins each CGx2 and Rust starter snapshot and
each reference snapshot under `repository_snapshots` and `reference_snapshots`.
Run `python3 bench/candidate_packet.py --validate
bench/fixtures/task-candidates/v3/evidence-unrun.json` to check packet hashes.
The Rust binary hash is the actual compiler behind the `rustup` shim; the
`cgx2c` hash is from `cargo build --offline -p cgx2c` in this worktree. They
identify this local review build, not an executable trial environment.

## Specifications and equivalence review

| Task | Public specification | Candidate edit surface | Equivalence risk to adjudicate |
| --- | --- | --- | --- |
| `quote-credit-v3` | Signed i64 `units * unit_price + shipping - credit`, applying credit once; each intermediate is checked. Public four-argument API remains. | `cgx2/src/adjust.cg` or `rust/adjust.rs` | Rust and CGx2 must reject the same overflowing intermediate even when a final mathematical result would fit. Compare failure class, not diagnostic bytes. |
| `grid-quote-v3` | Four charge groups aggregate 14 components once each across a 20-module graph; normalized signed i64 usage; checked intermediates. Public component, group, and total APIs remain. | Four aggregation modules per language, named in `task-cards.json` | A correct public total can conceal incorrect component/group paths; compare each API and overflow class across languages. |

Both conditions use the same task statement and private case family. Rust is
the preregistered baseline. Python, TypeScript, and OCaml are excluded for the
specific fixed-width and starter-equivalence reasons in `pack.json`; reviewers
must confirm those exclusions before trial freeze. The two tasks alone cannot
support a language-level productivity claim.

## Hidden-oracle and leakage boundary

The model-visible material is only a task statement and the selected language
starter. `fixtures/task-candidates/v3/private/` contains the acceptance
script and reference trees; only their hashes and opaque references may enter
candidate-facing manifests. The scored runner checks starter snapshots and
allowed file lists, rejects symlinks and special files, and stages a copied
workspace. The worker may start only after namespace, seccomp, Landlock, and
aggregate cgroup checks pass. The private oracle stays outside the workspace
mount. Reviewers should inspect the prompts and starters for answer leakage,
corpus duplication, and asymmetry between language conditions, and verify
that no private path, file descriptor, network route, or host mount reaches a
candidate. The current host has not run live adversarial fixtures.

## Independent reviewer record (one per task, separate from its author)

Record reviewer ID and UTC time; independence from the task author; ambiguity
assessment; language equivalence and exclusion assessment; difficulty and
substantiveness; leakage and corpus-duplication checks; a specific accept or
reject decision with rationale; and the reviewed pin-set hash. Keep the
existing JSON review fields null until a human reviewer supplies these facts.
After both tasks are approved, freeze prompts, docs, tools, dependencies,
compiler binaries including Rust, hardware, model versions, budgets, run grid,
and order before paired trials from two model families. Keep `run_state=unrun`
and `claim_status=not_assessed` until authenticated trial traces exist.
