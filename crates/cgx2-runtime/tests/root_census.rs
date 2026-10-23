use cgx2_runtime::{CollectionMode, MemoryError, RootCensus, Session};

fn counts(rooted_handles: usize, root_registrations: usize) -> RootCensus {
    RootCensus {
        rooted_handles,
        root_registrations,
    }
}

#[test]
fn duplicate_registrations_and_failed_unroot_preserve_counts() {
    let mut session = Session::new();
    let region = session.default_region();
    let handle = session.alloc_bytes(region, 1).unwrap();
    assert_eq!(session.root_census(), Ok(counts(0, 0)));

    session.root(handle).unwrap();
    session.root(handle).unwrap();
    assert_eq!(session.root_census(), Ok(counts(1, 2)));
    assert_eq!(session.root_census_region(region), Ok(counts(1, 2)));
    session.unroot(handle).unwrap();
    assert_eq!(session.root_census(), Ok(counts(1, 1)));
    session.unroot(handle).unwrap();
    assert_eq!(session.root_census(), Ok(counts(0, 0)));
    assert_eq!(session.unroot(handle), Err(MemoryError::NotRooted));
    assert_eq!(session.root_census(), Ok(counts(0, 0)));
}

#[test]
fn cycle_collection_close_and_stale_handles_follow_live_slots() {
    let mut session = Session::new();
    let a = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let b = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let first = session.alloc_object(a, 1, 1).unwrap();
    let second = session.alloc_object(a, 1, 1).unwrap();
    let other = session.alloc_bytes(b, 1).unwrap();
    session.set_reference(first, 0, Some(second)).unwrap();
    session.set_reference(second, 0, Some(first)).unwrap();
    session.root(first).unwrap();
    session.root(other).unwrap();
    assert_eq!(session.root_census_region(a), Ok(counts(1, 1)));
    assert_eq!(session.root_census_region(b), Ok(counts(1, 1)));
    assert_eq!(session.root_census(), Ok(counts(2, 2)));

    session.collect(a).unwrap();
    assert_eq!(session.root_census_region(a), Ok(counts(1, 1)));
    assert_eq!(session.close_region(a), Err(MemoryError::RegionHasRoots));
    assert_eq!(session.root_census_region(a), Ok(counts(1, 1)));
    session.unroot(first).unwrap();
    session.collect(a).unwrap();
    assert_eq!(session.root(first), Err(MemoryError::InvalidHandle));
    assert_eq!(session.root_census_region(a), Ok(counts(0, 0)));
    assert_eq!(session.root_census(), Ok(counts(1, 1)));
    session.close_region(a).unwrap();
    assert_eq!(
        session.root_census_region(a),
        Err(MemoryError::InvalidRegion)
    );
    assert_eq!(session.root_census(), Ok(counts(1, 1)));

    let foreign = Session::new().default_region();
    assert_eq!(
        session.root_census_region(foreign),
        Err(MemoryError::InvalidRegion)
    );
    session.unroot(other).unwrap();
    session.collect(b).unwrap();
    assert_eq!(session.root_census(), Ok(counts(0, 0)));
}
