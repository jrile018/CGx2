# Ordinary-memory harness finalization repair

Bounded harness repair for coordinator review; ordinary-memory and release acceptance remain false. The attached worktree started at reviewed base `9f0804feac1a3c549ed879a8738e0152b370ed4a`. Only parity-harness Python, new `finalization-evidence/`, and this report are changed.

## Behavior

`validation.sentinel_status(row, path, expected, code)` consumes the actual `cgx2c build/emit` result: one complete human diagnostic on stderr, empty stdout, exactly the expected primary code, normal exit 1, and consistent retained stream bytes/hashes. Duplicate, mixed, prose-only, malformed, missing and abnormal observations fail. Exact sentinel preservation is checked independently. Main rejection and package Python rejection use this same production helper; separate JSON `check` results cannot validate an emission.

`Journal.run` persists dispatch before executable hashing or spawning. Real dispatches select strict completed-command validation, retain executable and both stream observations, and remain required even when a later dynamic inventory changes. `Journal.finish` rejects timeout, signal, overflow, spawn/invocation exception, malformed terminal fields, stripped observations, failed required toolchain setup, incomplete child reports and missing identity. Normal expected nonzero rejections remain valid observations; genuine product failures remain red with complete coverage when all observations exist. Failed setup and missing coverage produce incomplete reports and nonzero exits.

`provenance.py` separates compiler build identity from the observing harness. `freeze.py` requires explicit `--build-source-commit` and `--build-invocation`; it verifies the recorded successful offline/locked build, every compiler input against both recorded source hashes and the actual Git commit, original artifacts/dependency membership, original harness snapshot, and toolchain identity before constructing a new view. Original manifests and frozen bytes are preserved. `candidate.json` binds original invocation/source/report/input references to a separately recorded checkout, dirty diff, and harness snapshot. Product audits run before and after verification. The historical helpers are separately frozen and snapshotted.

## Exact identities

- Compiler build: `ded3768480c745ce544d33c56aea44d4fc0bfa5a`; supplied build has clean initial source, Rust 1.98.1, jobs 2, incremental off, dev/test debug 0, and offline/locked Cargo flags.
- Compiler snapshot: `b20762f59667ba168f4d1dddd4b06d8c9fd045668bfe994fe7ecf744ddf57d6c` over 214 relative source/build-input hashes. All match this checkout and Git objects at the build commit.
- Final observing harness snapshot: `fb6abc3d6a75b5de83f01b1f5ebd11af868c78259a57a7b38d8ba90586863e66` over 39 harness inputs. It records HEAD `9f0804f` plus the actual dirty source snapshot; the delivery commit is reported separately. These binaries were not rebuilt by this chat.
- Frozen view: 1,382 artifact-tree entries, including seven binaries, five canonical probe libraries and 1,368 dependency files; 214 compiler inputs, 39 harness inputs, 68 toolchain inputs and 67 unchanged helper inputs. Original build/toolchain and all 536 coordinator archives were verified before reuse.

The post-build checkout changes are retained by name in the candidate record. Compiler implementation bytes are unchanged. The recovery runner difference is exactly the reviewed assertion accepting one old four-root or corrected six-root formula; replacement and budget/runtime assertions remain unchanged.

## Verification and retained attempts

Final evidence is under `ordinary-parity-repair-evidence/finalization-evidence/`. Its root `manifest.json` inventories **2,053 gzip archives and 15 metadata/source files including the root manifest: 2,068 files total**. All compressed and decompressed hashes are verified; 1,848 entries also match complete inner manifests. The remaining 205 archives are the retained partial export, independently inventoried by the root manifest. Repeated attempts remain repeated files, not deduplicated checks. `acceptance.json` separates all source identities, outcomes, expected exits and limitations.

| Final verification | Passed / failed | Disposition |
| --- | --- | --- |
| Unchanged selftests | 48 + 2 + 8 / 0 | All 58 existing test sources match reviewed base exactly |
| New adversarial selftests | 19 / 0 | Actual dispatch, production validators and `Journal.finish`; total selftests 77 |
| Retained observation/archive audit | 12 / 0 | 766 prior files, 19 hardening reports/422 commands, 14 diagnostics/sentinels, 536 coordinator archives |
| Exact candidate freeze | 6 / 0 | Original compiler build and actual observing harness separately bound |
| Fresh main | 964 / 1 | Complete; only `dependency-negative-exact-file` fails |
| Fresh Auto | 34 / 0 | Complete |
| Fresh package parity | 45 / 0 | Complete |
| Fresh OM01–08 review | 21 / 0 | Complete |
| Unchanged historical corpus | 135 / 1 | All 18 regression commands executed; managed-product remains failed |
| Nested unchanged managed repair | 670 / 8 | Graph-root reserve 8 and guard reserve 9 are unreached in interpreter/debug/release/standalone |
| Original owned-Result negative | 523 / 1 | Explicitly expected nonzero regression; separate from managed 670/8 |
| Historical wrapper | 10 / 1 | Complete coverage; raw child failure remains red |
| Final quality | 11 / 0 | All 77 selftests, Python syntax, diff, historical/source/artifact integrity |
| Independent final audit | 51 / 0 | Exact inventories/identities/counts, hashes, external cwd, unchanged test sources, no Cargo |
| Final driver | 8 / 2 | Complete; main and historical semantic failures remain red; nonzero result |

Seventeen of the 18 regressions match their specified exit; the managed-product command returns 1 where 0 is required. The expected owned-Result historical nonzero is not converted to a pass count for that original report. All before/after compiler, harness, helper, dependency, artifact and toolchain audits pass for the final run. All 105 modern generated native program hashes and 35 standalone source hashes match retained artifacts. All 70 modern standalone build/run commands used three actual outside-repository working directories, verified independently with Git.

The supplied, independently identity-audited build's **1383/0/6** workspace result, Clippy/fmt/build success, focused recovery **515/0**, and separate reached-reserve reconciliation **133/0** are attributed to the supplied candidate and archives; they were not rerun or relabeled as a new build. Original recovery **463 checks + exception/nonzero** remains archived unchanged. No compiler/runtime rebuild was necessary.

The first audit attempt mistakenly selected CGE006 for four CGT095 Python-emission type rejections. Its failed report and hash-verified exact harness snapshot are retained; the corrected audit passes all 14 original rejection diagnostics and all 14 original sentinel files. All 766 files in prior `evidence/` (393) and `hardening-evidence/` (373) remain immutable. Nineteen retained command reports containing 422 commands are revalidated; original incomplete attempts and genuine failed checks stay distinct from validator vulnerabilities.

Identical actual adversarial outputs demonstrate that the old validator accepts duplicate/mixed/prose diagnostics and timeout/signal/overflow finalization while the new production paths reject them. A further actual command-stripping reproduction is retained; the final repair rejects stripped invocation/stream/executable observations and missing dispatched rows.

Earlier product observations belong to their recorded harness snapshots. The intermediate pass was retained when the final command-structure fix changed the harness; its later integrity checks must fail rather than relabel those observations as final-harness validation. The final pass uses a fresh frozen view, output directories, private build/TMPDIR pair and external standalone root. No Cargo runs during frozen product verification. Infrastructure quota/startup failures, the first wrong audit expectation, and every partial/failed attempt remain retained.

The first independent audit retained **46/5** because its test-count regex was overescaped and it selected the owned-Result report instead of the nested managed report. The corrected audit is **51/0**, with no product/harness change. The exporter then encountered the deliberately stripped reproduction's absent `argv`; its failed gate and partial output remain immutable. A separately hashed opaque packager in the new evidence directory archives those raw files without interpreting or modifying rows. The validated product harness remains exactly `fb6abc3…`; the general exporter still requires ordinary command rows with `argv` for its source-discovery pass. Both packaging gates, all raw attempts, and the packager source are retained.

Changed root-level harness Python files under `ordinary-parity-repair-evidence/`: `auto.py`, `common.py`, `export.py`, `freeze.py`, `integrity.py`, `package_parity.py`, `quality.py`, `replay.py`, `review.py`, `run.py`, `validation.py`, `verify.py`, and new `finalization_audit.py`, `finalize.py`, `provenance.py`, `test_finalization.py` (16 files). Together with this report and 2,068 new evidence files (including the separate raw packager source), the delivery changes 2,085 files. Existing `test_harness.py`, supplemental/historical tests, all fixtures/probes, previous reports/evidence, compiler/runtime files and coordinator records are unchanged.

## Remaining boundaries

Static dependency filename attribution remains failed. Historical/recovery raw failures and separately reviewed reconciliations remain separate; none are normalized or rewritten. This repair changes no compiler/runtime implementation, probe, fixture, diagnostic, span, charge, root count or expected runtime result. There are no performance measurements, installs, external contacts, main merges or publication.

R01–R15, general aggregate ownership/partial moves/reborrows/recursive/generic/package ownership, source `copy_owned`/`promote` with owner-preserving failure, authenticated v3 summaries, and broader external acceptance remain open. This delivery stops for coordinator review.
