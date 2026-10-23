use cgx2_runtime::{AllocationError, CollectionMode, MemoryError, RootCensus, Session};

#[test]
fn failed_nested_scope_and_source_like_cleanup_leave_no_registrations() {
    let mut session = Session::new();
    let region = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let retained = session.alloc_bytes(region, 1).unwrap();
    session.root(retained).unwrap();
    session.root(retained).unwrap();
    let rooted = RootCensus {
        rooted_handles: 1,
        root_registrations: 2,
    };
    assert_eq!(session.root_census(), Ok(rooted));

    let failed_scope: Result<(), AllocationError> = {
        let mut outer = session.no_gc(4096);
        let mut inner = outer.no_gc(2048);
        assert_eq!(inner.root_census_region(region), Ok(rooted));
        inner.inject_host_failure_at_reserve(1);
        assert_eq!(
            inner.alloc_object(region, 1, 1),
            Err(AllocationError::HostAllocation)
        );
        let report_after_failure = inner.memory_report();
        let attempts_after_failure = inner.host_reserve_attempts();
        let failure_site = inner.last_host_failure_site();
        assert_eq!(inner.root_census(), Ok(rooted));
        assert_eq!(inner.root_census_region(region), Ok(rooted));
        assert_eq!(inner.memory_report(), report_after_failure);
        assert_eq!(inner.host_reserve_attempts(), attempts_after_failure);
        assert_eq!(inner.last_host_failure_site(), failure_site);
        Err(AllocationError::HostAllocation)
    };
    assert_eq!(failed_scope, Err(AllocationError::HostAllocation));
    assert_eq!(session.root_census(), Ok(rooted));

    // Mirrors finally/unwind cleanup of two source bindings for one runtime handle.
    session.unroot(retained).unwrap();
    assert_eq!(
        session.root_census_region(region),
        Ok(RootCensus {
            rooted_handles: 1,
            root_registrations: 1,
        })
    );
    session.unroot(retained).unwrap();
    assert_eq!(session.unroot(retained), Err(MemoryError::NotRooted));
    assert_eq!(session.root_census(), Ok(RootCensus::default()));
    session.collect(region).unwrap();
    session.close_region(region).unwrap();
    assert_eq!(session.root_census(), Ok(RootCensus::default()));
}
