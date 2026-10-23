use std::panic::{catch_unwind, AssertUnwindSafe};

use cgx2_runtime::{
    AllocationError, CleanupReport, CollectionMode, MemoryError, RootCensus, Session,
};

#[test]
fn scoped_roots_own_duplicates_without_consuming_manual_registrations() {
    let mut session = Session::try_new().unwrap();
    let region = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let handle = session.alloc_bytes(region, 1).unwrap();
    session.root(handle).unwrap();
    {
        let mut scope = session.scoped_roots();
        scope.root(handle).unwrap();
        scope.root(handle).unwrap();
        assert_eq!(scope.root_census().unwrap().root_registrations, 3);
        scope.unroot(handle).unwrap();
        assert_eq!(scope.unroot(handle), Err(MemoryError::NotRooted));
        assert_eq!(scope.close_region(region), Err(MemoryError::RegionHasRoots));
        scope.collect(region).unwrap();
        assert!(scope.read_bytes(handle).is_ok());
        let outcome = scope.finish::<(), _>(Err("cancelled by caller"));
        assert_eq!(outcome.result, Err("cancelled by caller"));
        assert_eq!(
            outcome.cleanup,
            CleanupReport {
                released_roots: 2,
                first_error: None
            }
        );
        assert_eq!(scope.cleanup(), CleanupReport::default());
        // A drained scope can safely register again without another allocation.
        scope.root(handle).unwrap();
    }
    assert_eq!(session.root_census(), Ok(RootCensus::default()));
    session.collect(region).unwrap();
    assert_eq!(session.read_bytes(handle), Err(MemoryError::InvalidHandle));
}

#[test]
fn nested_cleanup_on_early_return_and_unwind_is_region_local() {
    for unwind in [false, true] {
        let mut session = Session::try_new().unwrap();
        let a = session.create_region(CollectionMode::Manual, 4096).unwrap();
        let b = session.create_region(CollectionMode::Manual, 4096).unwrap();
        let first = session.alloc_bytes(a, 1).unwrap();
        let second = session.alloc_bytes(b, 1).unwrap();
        session.root(second).unwrap();
        let result = catch_unwind(AssertUnwindSafe(|| -> Result<(), &'static str> {
            let mut outer = session.scoped_roots();
            outer.root(first).unwrap();
            {
                let mut inner = outer.scoped_roots();
                inner.root(first).unwrap();
                inner.root(second).unwrap();
                if unwind {
                    panic!("host unwind");
                }
                assert_eq!(inner.cleanup().released_roots, 2);
            }
            assert_eq!(outer.root_census_region(a).unwrap().root_registrations, 1);
            Err("original failure")
        }));
        if unwind {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap(), Err("original failure"));
        }
        assert_eq!(session.root_census_region(a), Ok(RootCensus::default()));
        assert_eq!(session.root_census_region(b).unwrap().root_registrations, 1);
        assert_eq!(session.stats(a).unwrap().collection_count, 0);
        assert_eq!(session.stats(b).unwrap().collection_count, 0);
        session.close_region(a).unwrap();
        assert!(session.read_bytes(second).is_ok());
        session.unroot(second).unwrap();
        session.close_region(b).unwrap();
    }
}

#[test]
fn failed_guard_entry_preserves_outer_budget_and_collection_prohibition() {
    let mut session = Session::try_new().unwrap();
    let region = session.default_region();
    {
        let mut outer = session.try_no_gc(0).unwrap();
        assert!(matches!(
            outer.try_no_gc(4096),
            Err(AllocationError::BudgetExceeded)
        ));
        assert_eq!(outer.collect(region), Err(MemoryError::CollectionForbidden));
        assert_eq!(outer.root_census(), Ok(RootCensus::default()));
    }
    session.collect(region).unwrap();
}

#[cfg(feature = "fault-injection")]
#[test]
fn entry_and_root_reserve_faults_are_atomic_and_retryable() {
    use cgx2_runtime::{ReserveSiteV2, SessionEntryError};
    assert!(matches!(
        Session::try_new_with_host_failure_at_reserve(1),
        Err(SessionEntryError::HostAllocation {
            site: ReserveSiteV2::SessionRegions
        })
    ));
    let mut session = Session::try_new_with_host_failure_at_reserve(2).unwrap();
    assert_eq!(session.host_reserve_attempts(), 1);
    assert!(matches!(
        session.try_no_gc(1),
        Err(AllocationError::HostAllocation)
    ));
    assert_eq!(
        session.last_host_failure_site_v2(),
        Some(ReserveSiteV2::GuardBudgets)
    );
    assert_eq!(session.last_host_failure_site(), None);
    let region = session.default_region();
    session.collect(region).unwrap();
    let handle = session.alloc_bytes(region, 1).unwrap();
    let before = session.memory_report();
    {
        let mut scope = session.scoped_roots();
        scope.inject_host_failure_at_reserve(1);
        assert_eq!(
            scope.root(handle),
            Err(MemoryError::Allocation(AllocationError::HostAllocation))
        );
        assert_eq!(
            scope.last_host_failure_site_v2(),
            Some(ReserveSiteV2::ScopedRoots)
        );
        assert_eq!(scope.root_census(), Ok(RootCensus::default()));
        assert_eq!(scope.memory_report(), before);
        scope.root(handle).unwrap();
        scope.inject_host_failure_at_reserve(1);
        assert_eq!(
            scope.root(handle),
            Err(MemoryError::Allocation(AllocationError::HostAllocation))
        );
        assert_eq!(scope.root_census().unwrap().root_registrations, 1);
        let attempts = scope.host_reserve_attempts();
        assert_eq!(scope.cleanup().released_roots, 1);
        assert_eq!(scope.cleanup(), CleanupReport::default());
        assert_eq!(scope.host_reserve_attempts(), attempts);
    }
    assert_eq!(session.root_census(), Ok(RootCensus::default()));
}

#[test]
fn root_bookkeeping_budget_failure_preserves_existing_roots() {
    let mut session = Session::try_new().unwrap();
    let region = session.default_region();
    let handle = session.alloc_bytes(region, 1).unwrap();
    session.root(handle).unwrap();
    {
        let mut guard = session.try_no_gc(0).unwrap();
        let mut scope = guard.scoped_roots();
        assert_eq!(
            scope.root(handle),
            Err(MemoryError::Allocation(AllocationError::BudgetExceeded))
        );
        assert_eq!(scope.cleanup(), CleanupReport::default());
        assert_eq!(scope.root_census().unwrap().root_registrations, 1);
        assert_eq!(scope.collect(region), Err(MemoryError::CollectionForbidden));
    }
    session.unroot(handle).unwrap();
    session.collect(region).unwrap();
}

#[test]
fn root_bookkeeping_charges_each_guard_and_cleanup_does_not_refund() {
    let root_bytes = std::mem::size_of::<cgx2_runtime::Handle>();
    let nested_table_bytes = 4 * std::mem::size_of::<usize>(); // two Budget entries
    for limit_outer in [true, false] {
        let mut session = Session::try_new().unwrap();
        let region = session.default_region();
        let handle = session.alloc_bytes(region, 1).unwrap();
        {
            let outer_limit = if limit_outer {
                nested_table_bytes + root_bytes
            } else {
                usize::MAX
            };
            let inner_limit = if limit_outer { usize::MAX } else { root_bytes };
            let mut outer = session.try_no_gc(outer_limit).unwrap();
            let mut inner = outer.try_no_gc(inner_limit).unwrap();
            {
                let mut scope = inner.scoped_roots();
                scope.root(handle).unwrap();
                assert_eq!(
                    scope.root(handle),
                    Err(MemoryError::Allocation(AllocationError::BudgetExceeded))
                );
                assert_eq!(scope.cleanup().released_roots, 1);
                scope.root(handle).unwrap(); // retained capacity needs no new reserve
            }
            let mut new_scope = inner.scoped_roots();
            assert_eq!(
                new_scope.root(handle),
                Err(MemoryError::Allocation(AllocationError::BudgetExceeded))
            );
            assert_eq!(new_scope.root_census(), Ok(RootCensus::default()));
            assert_eq!(
                new_scope.collect(region),
                Err(MemoryError::CollectionForbidden)
            );
        }
        assert_eq!(session.stats(region).unwrap().collection_count, 0);
        session.collect(region).unwrap();
    }
}
