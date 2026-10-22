# Wave 12 managed reference state model

This is the direct `Session` runtime profile. A handle contains a session identity,
region, slot index, and generation. A live handle names one occupied slot with the
same generation in an open region. Handles are values, not roots: only successful
`root(handle)` calls contribute to the root count. `unroot` removes one count.

`alloc_object(region, payload_len, reference_slots)` creates a zero-filled byte
payload and exactly `reference_slots` initially empty checked slots. The profile
limits payloads to `MAX_OBJECT_PAYLOAD_BYTES` and references to
`MAX_OBJECT_REFERENCE_SLOTS`; exceeding either returns `AllocationError::ShapeLimit`.
The object retains its reference-slot allocation for its lifetime. Its logical
region charge is `SLOT_METADATA_BYTES + payload capacity + reference-slot capacity
* REFERENCE_SLOT_METADATA_BYTES`. Allocation checks arithmetic, host capacity,
the region limit, and every active `no_gc` budget before committing a slot.

`set_reference(owner, index, target)` accepts `None` to clear a slot. The owner
must be a live object allocation, the index must exist, and a `Some(target)` must
be live in the same region. Cross-region edges are rejected, including edges into
another open region. Failed updates leave the old slot, charges, roots, and graph
unchanged. Updates allocate nothing and are allowed inside `no_gc`; they cannot
trigger collection or consume budget. Existing `link` edges remain strong and
retain their v1–v5 behavior. `reference(owner, index)` observes the stored value
while the owner is live.

Collection starts from every positive root count in the selected region and
traverses both legacy links and occupied checked slots. Shared children remain
live if any root reaches them. Unreachable cycles are reclaimed together;
generation increases so old handles remain invalid after slot reuse. Clearing
or overwriting the last reachable edge makes the old child collectible at the
next collection. The collector never visits another region. A region closes only
without roots, releases its objects without counting as collection, and never
reuses its region identity.

Manual regions collect only on explicit `collect`. Auto regions may collect at
the next allocation safepoint after their trigger or when an otherwise feasible
allocation needs headroom. An impossible shape, budget, or per-object region
request fails before an auto collection decision. A `no_gc` guard suppresses all
session collection; nested guards each charge every successful allocation
cumulatively, and any exhausted budget rejects the allocation. Failed allocation
may increment the destination region's failure counter, but it does not install
an object, charge a budget, or change committed/live bytes. An auto collection
that precedes a later failed allocation remains a separately reported event.

State-machine tests compare live handles, references, charges, failures, and
per-region reports with this model after each deterministic transition. Finite
seed coverage and ordinary Rust tests do not establish the release memory-safety
proof; sanitizer, Miri, source-level escape checking, and foreign-handle audits
remain separate work.
