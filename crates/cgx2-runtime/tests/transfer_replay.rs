//! Deterministic direct-runtime cross-region replay. Source-level cases live in cgx2c.
#[cfg(feature = "fault-injection")]
use cgx2_runtime::AllocationError;
use cgx2_runtime::{CollectionMode, Handle, MemoryError, RegionId, Session};

#[derive(Debug, PartialEq, Eq)]
struct Step {
    operation: &'static str,
    region_index: usize,
    handle_generation: Option<u64>,
    owned_values: usize,
    root_registrations: usize,
    committed_bytes: usize,
    live_bytes: usize,
    collection_count: u64,
    first_error: Option<MemoryError>,
    valid_handles: Vec<bool>,
}

fn record(
    session: &Session,
    operation: &'static str,
    region: RegionId,
    handle: Option<Handle>,
    owned_values: usize,
    first_error: Option<MemoryError>,
    handles: &[Handle],
) -> Step {
    if let Some(handle) = handle {
        assert_eq!(handle.region(), region);
    }
    let report = session.memory_report();
    let row = &report.regions[region.index()];
    Step {
        operation,
        region_index: region.index(),
        handle_generation: handle.map(Handle::generation),
        owned_values,
        root_registrations: session.root_census().unwrap().root_registrations,
        committed_bytes: row.committed_bytes,
        live_bytes: row.live_bytes,
        collection_count: row.collection_count,
        first_error,
        valid_handles: handles
            .iter()
            .map(|handle| session.read_bytes(*handle).is_ok())
            .collect(),
    }
}

fn replay(close_source_first: bool) -> Vec<Step> {
    let mut session = Session::new();
    let source_region = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let destination_region = session
        .create_region(
            CollectionMode::Auto {
                trigger_bytes: 4096,
            },
            4096,
        )
        .unwrap();
    let mut trace = Vec::new();
    let mut first_error = None;
    let mut owned = session.alloc_owned_bytes(4).unwrap();
    owned.as_mut_slice().copy_from_slice(&[1, 2, 3, 4]);
    trace.push(record(
        &session,
        "allocate owned",
        source_region,
        None,
        1,
        first_error,
        &[],
    ));
    let source = session.promote_owned(source_region, owned).unwrap();
    session.root(source).unwrap();
    session.root(source).unwrap();
    trace.push(record(
        &session,
        "promote and duplicate root",
        source_region,
        Some(source),
        0,
        first_error,
        &[source],
    ));
    assert_eq!(trace.last().unwrap().root_registrations, 2);

    let mut copy = session.copy_managed_to_owned(source).unwrap();
    assert_ne!(
        session.read_bytes(source).unwrap().as_ptr(),
        copy.as_slice().as_ptr()
    );
    copy.as_mut_slice()[0] = 9;
    assert_eq!(session.read_bytes(source), Ok(&[1, 2, 3, 4][..]));
    trace.push(record(
        &session,
        "copy to distinct owned storage",
        source_region,
        Some(source),
        1,
        first_error,
        &[source],
    ));
    let destination = session.promote_owned(destination_region, copy).unwrap();
    session.root(destination).unwrap();
    assert_eq!(session.read_bytes(destination), Ok(&[9, 2, 3, 4][..]));
    trace.push(record(
        &session,
        "promote copy into other region",
        destination_region,
        Some(destination),
        0,
        first_error,
        &[source, destination],
    ));
    assert_eq!(trace.last().unwrap().root_registrations, 3);
    assert_eq!(trace.last().unwrap().valid_handles, [true, true]);

    let before = session.memory_report();
    let roots = session.root_census().unwrap();
    let error = session.link(source, destination).unwrap_err();
    first_error.get_or_insert(error);
    assert_eq!(error, MemoryError::CrossRegionEdge);
    assert_eq!(session.root_census().unwrap(), roots);
    assert_eq!(session.memory_report(), before);
    trace.push(record(
        &session,
        "reject cross-region edge",
        destination_region,
        Some(destination),
        0,
        first_error,
        &[source, destination],
    ));
    assert_eq!(trace.last().unwrap().valid_handles, [true, true]);

    let (first_region, first_handle, second_region, second_handle) = if close_source_first {
        (source_region, source, destination_region, destination)
    } else {
        (destination_region, destination, source_region, source)
    };
    assert_eq!(
        session.close_region(first_region),
        Err(MemoryError::RegionHasRoots)
    );
    assert_eq!(first_error, Some(MemoryError::CrossRegionEdge));
    trace.push(record(
        &session,
        "reject rooted close",
        first_region,
        Some(first_handle),
        0,
        first_error,
        &[source, destination],
    ));
    session.unroot(first_handle).unwrap();
    if first_handle == source {
        session.unroot(first_handle).unwrap();
    }
    session.close_region(first_region).unwrap();
    trace.push(record(
        &session,
        "close first region",
        first_region,
        Some(first_handle),
        0,
        first_error,
        &[source, destination],
    ));
    assert_eq!(
        trace.last().unwrap().valid_handles,
        if close_source_first {
            [false, true]
        } else {
            [true, false]
        }
    );
    assert!(session.read_bytes(second_handle).is_ok());
    session.unroot(second_handle).unwrap();
    if second_handle == source {
        session.unroot(second_handle).unwrap();
    }
    session.close_region(second_region).unwrap();
    trace.push(record(
        &session,
        "close second region",
        second_region,
        Some(second_handle),
        0,
        first_error,
        &[source, destination],
    ));
    assert_eq!(session.root_census().unwrap().root_registrations, 0);
    let finished = session.memory_report();
    assert_eq!(
        session.close_region(first_region),
        Err(MemoryError::InvalidRegion)
    );
    assert_eq!(session.memory_report(), finished);
    assert_eq!(trace.last().unwrap().valid_handles, [false, false]);
    trace
}

#[test]
fn replay_copy_alias_edge_and_both_close_orders() {
    for close_source_first in [true, false] {
        let first = replay(close_source_first);
        let second = replay(close_source_first);
        assert_eq!(first, second, "replay must be deterministic");
        assert!(first.iter().all(|step| step.collection_count == 0));
        assert!(first.iter().any(|step| step.handle_generation.is_some()));
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn nested_no_gc_copy_failures_preserve_regions_guards_and_first_error() {
    use cgx2_runtime::ReserveSite;

    let mut session = Session::new();
    let source_region = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let destination_region = session
        .create_region(CollectionMode::Auto { trigger_bytes: 0 }, 4096)
        .unwrap();
    let source = session.alloc_bytes(source_region, 4).unwrap();
    session.root(source).unwrap();
    let stale_region = session.create_region(CollectionMode::Manual, 256).unwrap();
    let stale = session.alloc_bytes(stale_region, 1).unwrap();
    session.close_region(stale_region).unwrap();
    let collections_before = session.memory_report().totals.collection_count;
    assert_eq!(collections_before, 0);
    let destination_before = session.stats(destination_region).unwrap();
    let roots_before = session.root_census().unwrap();

    let mut outer = session.no_gc(4096);
    let mut inner = outer.no_gc(4096);
    inner.inject_host_failure_at_reserve(1);
    assert!(matches!(
        inner.copy_managed_to_owned(stale),
        Err(MemoryError::InvalidHandle)
    ));
    assert_eq!(
        inner.host_reserve_attempts(),
        0,
        "validation precedes reserve"
    );
    let first_error = MemoryError::InvalidHandle;
    inner.inject_host_failure_at_reserve(1);
    assert!(matches!(
        inner.copy_managed_to_owned(source),
        Err(MemoryError::Allocation(AllocationError::HostAllocation))
    ));
    assert_eq!(
        inner.last_host_failure_site(),
        Some(ReserveSite::ManagedToOwned)
    );
    assert_eq!(
        inner.collect(destination_region),
        Err(MemoryError::CollectionForbidden)
    );

    let copy = inner.copy_managed_to_owned(source).unwrap();
    inner.inject_host_failure_at_reserve(1);
    let (error, copy) = inner.promote_owned(destination_region, copy).unwrap_err();
    assert_eq!(error, AllocationError::HostAllocation);
    assert_eq!(inner.last_host_failure_site(), Some(ReserveSite::SlotTable));
    let after_failed_promotion = inner.stats(destination_region).unwrap();
    assert_eq!(
        after_failed_promotion.committed_bytes,
        destination_before.committed_bytes
    );
    assert_eq!(
        after_failed_promotion.live_bytes,
        destination_before.live_bytes
    );
    assert_eq!(
        after_failed_promotion.collection_count,
        destination_before.collection_count
    );
    assert_eq!(
        after_failed_promotion.allocation_failures,
        destination_before.allocation_failures + 1
    );
    assert_eq!(inner.root_census().unwrap(), roots_before);
    assert_eq!(inner.read_bytes(source), Ok(&[0, 0, 0, 0][..]));
    assert_eq!(first_error, MemoryError::InvalidHandle);
    let destination = inner.promote_owned(destination_region, copy).unwrap();
    inner.root(destination).unwrap();
    assert_eq!(
        inner.close_region(destination_region),
        Err(MemoryError::RegionHasRoots)
    );
    assert_eq!(
        inner.memory_report().totals.collection_count,
        collections_before
    );
    drop(inner);
    assert_eq!(
        outer.collect(destination_region),
        Err(MemoryError::CollectionForbidden)
    );
    assert_eq!(outer.read_bytes(destination), Ok(&[0, 0, 0, 0][..]));
    drop(outer);
    assert_eq!(
        session.memory_report().totals.collection_count,
        collections_before
    );
    session.unroot(destination).unwrap();
    session.close_region(destination_region).unwrap();
    session.unroot(source).unwrap();
    session.close_region(source_region).unwrap();
    assert_eq!(session.root_census().unwrap().root_registrations, 0);
}
