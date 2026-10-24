# Default ordinary managed delivery review

Delivery: bc4ba95e710eaed3816291fdc67de1478e0456f5, from reviewed base c9d60229ecf3ed2bf76b29525fec0bdda41d0906.
Staged on coordinator review branch as 7643899037c6fd9522534c7b9125b054aab6088c.
Disposition: **reviewed; repairs required; managed acceptance failed**. This staging commit is a repair base, not an accepted interface for new consumers.

## Ownership and evidence review
Worker worktree is clean and all592 changed files are within owned crate paths.49 implementation hashes and both root manifest hashes match delivery;24 selected worker raw log hashes also match. Root manifests and historical evidence were unchanged. Three independent read-only reviews inspected core/provenance, runtime and consumer/evidence paths.
Worker reports1268 workspace tests and328 product checks, required historical regressions, build/Clippy/fmt. Leak-enabled controls remain host-blocked and no-leak ASan passes do not close leak acceptance.
Coordinator independently rebuilt exact delivery source:1268 workspace tests and328 managed-product checks pass, along with build/Clippy/fmt and every required current regression. The unchanged historical shadow-builtin negative remains a separate exit1; reconciled593 and restored17 checks pass. Results are retained in evidence/verification.json and acceptance.json. These successes do not close the reproduced defects or missing parity evidence. The coordinator's crates/manifests are identical to the tested delivery; documentation-only coordinator commits do not change tested code.

## Confirmed blockers
1. **Source provenance/authentication (P1).** default_managed::compile_module accepts an AST returning42 paired with "this is not CGx2 source". authenticate succeeds and execute returns42 while source_sha256 hashes unrelated text. Existing CLI/Rust AST paths also pass Debug AST text. The repro is in repros/core-managed-auth.rs and its actual output in auth-reproduction.json. Fix authoritative source/package construction and honest AST-only provenance without disabling supported package lowering.
2. **Nominal compatibility (P2).** sema/src/nominal.rs:140 preserves bare AllocationError record precedence but omits user variants. repros/core-allocationerror.cg prints42 on the prior accepted binary and produces CGT003/CGT033/CGT036 on the independently rebuilt delivery. Preserve user nominal identity while maintaining real recovery error typing.
3. **Result equality (P2).** runtime/src/graph.rs equal rejects differing Value.depth before logical case/payload comparison. repros/result-equality.cg compares managed.try_alloc(1) with Result<i64,AllocationError>.Ok(1) and returns false instead of true. The invariant applies to nested values, both operands and ==/!=, including Err carriers.

## Acceptance evidence gaps
4. Real exact allocation/root/nested budget checks and synthetic reserve2..15 failures are present in interpreter-level tests, but required native debug/release/standalone parity rows are absent from the new source runner.
5. The three static negative product fixtures currently accept any exit1, omit exact code/span checks and native-build sentinel preservation. Strengthen the runner so unrelated rejection cannot close a required diagnostic gate.

An additional source-span discrepancy is confirmed by inspection, not execution: CLI injected Session initialization returns CGR090 with span=None while native output includes entry Loc. Core repair must test/fix this existing promised source-location behavior; entry recovery itself remains deferred.

## Independent repair ownership
- core-auth/plan.md: nominal compatibility, authoritative source/package provenance and entry source spans; sema/CLI/emitter/package producers, dedicated new tests/evidence.
- runtime-equality/plan.md: logical Result comparison in graph runtime and dedicated tests/evidence; no source/type/model/API changes.
- evidence-parity/plan.md: bounded native fault/budget parity and precise rejection/artifact assertions through existing frozen test hooks; scripts/new fixtures only.

All repair chats start from the same committed reviewed repair base. Historical attempts are immutable; each repair supplies its own evidence. Coordinator performs combined review/checks before any managed acceptance or dependency frontier advance. No S3/new language consumers launch now.

R03/R04, full ordinary ownership, named regions/transfers/entry recovery, foreign/execution/commands, S3/S4, leak-enabled/external/performance and the complete R01–R15 ledger remain required. No release or main merge.

## Two repairs accepted as bounded deliveries

Runtime53d37c1 and evidence23b050f are independently reviewed and integrated as c287985/1b922e2. Coordinator runtime4+typed2 tests and87target parity pass; evidence14selftests pass. The evidence matrix's670passed/8failed result is intentionally retained: all8 failures await the core Session-entry span repair and combined rerun. Core provenance/nominal delivery is still pending; full managed acceptance remains withheld. Details: repairs-reviewed.json.
