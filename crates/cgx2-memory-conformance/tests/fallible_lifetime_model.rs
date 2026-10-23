//! Fixed transition corpus with a root-count oracle independent of runtime tables.
//! These are library lifetime tests, not source ownership/lowering acceptance.
use std::panic::{catch_unwind, AssertUnwindSafe};

use cgx2_runtime::{
    AllocationError as A, CleanupReport, CollectionMode, Handle, MemoryError as M, RegionId,
    ReserveSiteV2, RootCensus, ScopedRoots, Session, SessionEntryError,
};

fn census(session: &Session, regions: [RegionId; 2], registrations: [usize; 2]) {
    for (region, count) in regions.into_iter().zip(registrations) {
        assert_eq!(
            session.root_census_region(region),
            Ok(RootCensus {
                rooted_handles: usize::from(count != 0),
                root_registrations: count,
            })
        );
        assert_eq!(session.stats(region).unwrap().collection_count, 0);
    }
    assert_eq!(
        session.root_census(),
        Ok(RootCensus {
            rooted_handles: registrations.iter().filter(|&&n| n != 0).count(),
            root_registrations: registrations.iter().sum(),
        })
    );
}

fn root_fault(
    scope: &mut ScopedRoots<'_>,
    handle: Handle,
    regions: [RegionId; 2],
    counts: [usize; 2],
) {
    let before = scope.memory_report();
    scope.inject_host_failure_at_reserve(1);
    assert_eq!(scope.root(handle), Err(M::Allocation(A::HostAllocation)));
    assert_eq!(
        scope.last_host_failure_site_v2(),
        Some(ReserveSiteV2::ScopedRoots)
    );
    assert_eq!(scope.last_host_failure_site(), None);
    census(scope, regions, counts);
    assert_eq!(scope.memory_report(), before);
}

#[test]
fn replay_entry_root_cleanup_and_exit_transitions() {
    for line in include_str!("../corpus/release-lifetimes.csv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = line.split(',').collect();
        let seed = u64::from_str_radix(fields[0].trim_start_matches("0x"), 16).unwrap();
        let mode = match fields[1] {
            "manual" => CollectionMode::Manual,
            "auto" => CollectionMode::Auto { trigger_bytes: 1 },
            other => panic!("unknown mode {other}"),
        };
        assert!(matches!(
            Session::try_new_with_host_failure_at_reserve(1),
            Err(SessionEntryError::HostAllocation {
                site: ReserveSiteV2::SessionRegions
            })
        ));
        let mut session = Session::try_new().unwrap();
        let regions = [
            session.create_region(mode, 8192).unwrap(),
            session.create_region(CollectionMode::Manual, 8192).unwrap(),
        ];
        let handles = [
            session.alloc_bytes(regions[0], 1).unwrap(),
            session.alloc_bytes(regions[1], 1).unwrap(),
        ];
        session.root(handles[0]).unwrap();
        session.root(handles[1]).unwrap();
        census(&session, regions, [1, 1]);

        // A failed first guard entry does not suppress later explicit collection.
        session.inject_host_failure_at_reserve(1);
        assert!(matches!(session.try_no_gc(8192), Err(A::HostAllocation)));
        assert_eq!(
            session.last_host_failure_site_v2(),
            Some(ReserveSiteV2::GuardBudgets)
        );
        census(&session, regions, [1, 1]);

        // Budget rejection precedes reserve injection and leaves both regions alone.
        {
            let mut guard = session.try_no_gc(0).unwrap();
            guard.inject_host_failure_at_reserve(1);
            assert!(matches!(guard.try_no_gc(4096), Err(A::BudgetExceeded)));
            assert_eq!(guard.host_reserve_attempts(), 0);
            let mut scope = guard.scoped_roots();
            assert_eq!(
                scope.root(handles[0]),
                Err(M::Allocation(A::BudgetExceeded))
            );
            assert_eq!(scope.host_reserve_attempts(), 0);
            assert_eq!(scope.cleanup(), CleanupReport::default());
            census(&scope, regions, [1, 1]);
            for region in regions {
                assert_eq!(scope.collect(region), Err(M::CollectionForbidden));
            }
            scope.disable_host_failure();
        }
        {
            let mut unrelated = Session::try_new().unwrap();
            let foreign = unrelated
                .alloc_bytes(unrelated.default_region(), 1)
                .unwrap();
            let mut scope = session.scoped_roots();
            scope.inject_host_failure_at_reserve(1);
            assert_eq!(scope.root(foreign), Err(M::InvalidHandle));
            assert_eq!(scope.host_reserve_attempts(), 0);
            assert_eq!(scope.cleanup(), CleanupReport::default());
            census(&scope, regions, [1, 1]);
            scope.disable_host_failure();
        }

        let exit = catch_unwind(AssertUnwindSafe(|| -> Result<(), &'static str> {
            let mut guard = session.try_no_gc(8192).unwrap();
            guard.inject_host_failure_at_reserve(1);
            assert!(matches!(guard.try_no_gc(4096), Err(A::HostAllocation)));
            census(&guard, regions, [1, 1]);
            let mut nested = guard.try_no_gc(4096).unwrap();
            for region in regions {
                assert_eq!(nested.collect(region), Err(M::CollectionForbidden));
            }
            // Allocation failure in nested guards cannot collect even an auto region.
            nested.inject_host_failure_at_reserve(1);
            assert_eq!(nested.alloc_bytes(regions[0], 2), Err(A::HostAllocation));
            census(&nested, regions, [1, 1]);
            let mut outer = nested.scoped_roots();
            root_fault(&mut outer, handles[0], regions, [1, 1]);
            outer.root(handles[0]).unwrap();
            census(&outer, regions, [2, 1]);
            root_fault(&mut outer, handles[1], regions, [2, 1]);
            {
                let mut inner = outer.scoped_roots();
                root_fault(&mut inner, handles[1], regions, [2, 1]);
                inner.root(handles[1]).unwrap();
                inner.root(handles[1]).unwrap();
                census(&inner, regions, [2, 3]);
                inner.unroot(handles[1]).unwrap();
                assert_eq!(inner.unroot(handles[1]), Err(M::NotRooted));
                census(&inner, regions, [2, 2]);
                assert_eq!(inner.close_region(regions[1]), Err(M::RegionHasRoots));
                match fields[2] {
                    "unwind" => panic!("seed={seed:#x} intentional unwind"),
                    "cancel" => {
                        let outcome = inner.finish::<(), _>(Err("cancelled"));
                        assert_eq!(outcome.result, Err("cancelled"));
                        assert_eq!(outcome.cleanup.released_roots, 2);
                    }
                    "return" => {
                        assert_eq!(inner.cleanup().released_roots, 2);
                    }
                    other => panic!("unknown exit {other}"),
                }
                assert_eq!(inner.cleanup(), CleanupReport::default());
                census(&inner, regions, [2, 0]);
            }
            census(&outer, regions, [2, 0]);
            Err("original error")
        }));
        if fields[2] == "unwind" {
            assert!(exit.is_err());
        } else {
            assert_eq!(exit.unwrap(), Err("original error"));
        }
        census(&session, regions, [1, 0]);
        session.unroot(handles[0]).unwrap();
        census(&session, regions, [0, 0]);
        // Leaving no_gc and root scopes does not implicitly collect manual/auto regions.
        for (region, handle) in regions.into_iter().zip(handles) {
            assert!(session.read_bytes(handle).is_ok());
            session.collect(region).unwrap();
            assert_eq!(session.read_bytes(handle), Err(M::InvalidHandle));
            session.close_region(region).unwrap();
        }
    }
}
