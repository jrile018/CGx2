# Runtime root census

**Goal.** Wave 16's `final_unwind` zero counts are derived from cleared source bindings. Add an independent runtime observation that can later be consumed by interpreter and native failure reports. Memory correctness remains the highest priority.

**Ownership.** Only `crates/cgx2-runtime/**` and `crates/cgx2-memory-conformance/**`. Preserve existing `Session` behavior and public APIs. Do not edit CLI/emitter/report schemas, foreign adapters, root Cargo files, or shared docs.

## Deliverable

1. Add a nonbreaking, read-only, allocation-free `Session::root_census() -> Result<RootCensus, MemoryError>` with `rooted_handles: usize` and `root_registrations: usize`; count duplicate registrations in the latter. Add `root_census_region(region: RegionId) -> Result<RootCensus, MemoryError>` if region-local slot tables permit it, returning the existing closed/unknown-region error for invalid IDs. Use checked accumulation and a defined overflow error rather than silent wrap. The census must inspect actual runtime root registrations, not source bindings or `MemoryReport.live_bytes`; calling it must not alter allocation/fault counters.
2. Test rooting the same handle twice, unrooting once/twice and failed unroot, same-region cycles, region close, stale handles, nested `no_gc`, injected reserve failure, and source-like cleanup. Check totals and per-region counts before and after failures. A zero census is a necessary cleanup condition, not a proof against all leaks or invalid memory access. This lane only supplies the API; CLI/native failure reports cannot claim runtime root parity until a later consumer uses it.
3. Extend the owned release-gate report with exact direct-runtime evidence, ASan/Miri/LSan command and result when available, and remaining source/native/foreign/concurrency gaps. Keep deterministic corpus seed and step bounds reproducible. Fix a demonstrated runtime bug with a regression trace if one appears.

## Verification

Run targeted offline tests twice, formatter, Clippy with warnings denied, and compatible dynamic tools only when installed. Report exact counts, commands, skipped tools, limitations and commit ID. Stop for review.
