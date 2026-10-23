//! Fixed direct-runtime host unwind traces. `CGX2_REPLAY_SEED=0x19a002`
//! selects one row from the committed corpus when reducing a failure.
#![cfg(feature = "fault-injection")]
use std::panic::{catch_unwind, AssertUnwindSafe};

use cgx2_runtime::{
    AllocationError, CollectionMode, Handle, MemoryError, RegionId, ReserveSite, RootCensus,
    Session,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Exit {
    Early,
    Panic,
}

#[derive(Clone, Copy, Debug)]
struct Trace {
    seed: u64,
    exit: Exit,
    reserve_index: usize,
    reserve_site: ReserveSite,
}

fn traces() -> impl Iterator<Item = Trace> {
    include_str!("../corpus/wave19-unwind-traces.csv")
        .lines()
        .skip(1)
        .map(|line| {
            let mut fields = line.split(',');
            let seed =
                u64::from_str_radix(fields.next().unwrap().trim_start_matches("0x"), 16).unwrap();
            let exit = match fields.next().unwrap() {
                "early" => Exit::Early,
                "panic" => Exit::Panic,
                value => panic!("unknown exit {value}"),
            };
            let reserve_index = fields.next().unwrap().parse().unwrap();
            let reserve_site = match fields.next().unwrap() {
                "ObjectPayload" => ReserveSite::ObjectPayload,
                "ObjectReferences" => ReserveSite::ObjectReferences,
                value => panic!("unknown reserve site {value}"),
            };
            assert!(fields.next().is_none());
            Trace {
                seed,
                exit,
                reserve_index,
                reserve_site,
            }
        })
}

fn census(handles: usize, registrations: usize) -> RootCensus {
    RootCensus {
        rooted_handles: handles,
        root_registrations: registrations,
    }
}

// Every expected field stays explicit at each replay checkpoint.
#[allow(clippy::too_many_arguments)]
fn check(
    session: &Session,
    region: RegionId,
    expected: RootCensus,
    live: &[Handle],
    dead: &[Handle],
    collections: u64,
    trace: Trace,
    step: &str,
) {
    let context = format!("seed=0x{:06x} step={step}", trace.seed);
    assert_eq!(session.root_census(), Ok(expected), "{context}");
    assert_eq!(
        session.root_census_region(region),
        Ok(expected),
        "{context}"
    );
    let stats = session.stats(region).unwrap();
    assert_eq!(stats.collection_count, collections, "{context}");
    assert_eq!(stats.committed_bytes, stats.live_bytes, "{context}");
    let report = session.memory_report();
    let row = report.regions.iter().find(|row| row.id == region).unwrap();
    assert!(row.is_open, "{context}");
    assert_eq!(row.committed_bytes, stats.committed_bytes, "{context}");
    assert_eq!(row.live_bytes, stats.live_bytes, "{context}");
    assert_eq!(row.peak_bytes, stats.peak_bytes, "{context}");
    assert_eq!(
        row.allocation_failures, stats.allocation_failures,
        "{context}"
    );
    assert_eq!(row.collection_count, collections, "{context}");
    assert_eq!(
        row.total_collected_bytes, stats.total_collected_bytes,
        "{context}"
    );
    assert_eq!(report.totals, stats, "{context}");
    for &handle in live {
        assert!(session.read_bytes(handle).is_ok(), "{context}: live handle");
    }
    for &handle in dead {
        assert_eq!(
            session.read_bytes(handle),
            Err(MemoryError::InvalidHandle),
            "{context}: dead handle"
        );
    }
}

fn replay(trace: Trace) {
    let mut session = Session::new();
    let default_report = session.memory_report();
    session.inject_host_failure_at_reserve(1);
    assert_eq!(
        session.create_region(CollectionMode::Manual, 4096),
        Err(AllocationError::HostAllocation),
        "seed=0x{:06x} create reserve",
        trace.seed
    );
    assert_eq!(
        session.last_host_failure_site(),
        Some(ReserveSite::RegionTable)
    );
    assert_eq!(session.memory_report(), default_report);
    assert_eq!(session.root_census(), Ok(census(0, 0)));

    // Both forms of host exit occur after region creation. Region lifetime is
    // explicit: a dropped stack frame does not close an otherwise valid ID.
    let mut region = None;
    let creation = catch_unwind(AssertUnwindSafe(|| -> Result<(), &'static str> {
        region = Some(session.create_region(CollectionMode::Manual, 4096).unwrap());
        match trace.exit {
            Exit::Early => Err("host early return after create"),
            Exit::Panic => panic!("host panic after create"),
        }
    }));
    match trace.exit {
        Exit::Early => assert!(matches!(
            creation,
            Ok(Err("host early return after create"))
        )),
        Exit::Panic => assert!(creation.is_err()),
    }
    let region = region.unwrap();
    check(
        &session,
        region,
        census(0, 0),
        &[],
        &[],
        0,
        trace,
        "create exit",
    );

    let owner = session.alloc_object(region, 1, 1).unwrap();
    let child = session.alloc_bytes(region, 1).unwrap();
    session.root(owner).unwrap();
    session.root(owner).unwrap();
    session.root(child).unwrap();
    session.set_reference(owner, 0, Some(child)).unwrap();
    check(
        &session,
        region,
        census(2, 3),
        &[owner, child],
        &[],
        0,
        trace,
        "graph rooted",
    );
    session.unroot(child).unwrap();
    check(
        &session,
        region,
        census(1, 2),
        &[owner, child],
        &[],
        0,
        trace,
        "edge retains child",
    );

    let mut transient = None;
    {
        let mut outer = session.no_gc(200);
        let aborted = catch_unwind(AssertUnwindSafe(|| -> Result<(), &'static str> {
            let mut inner = outer.no_gc(200);
            let before = inner.stats(region).unwrap();
            let before_census = inner.root_census().unwrap();
            inner.inject_host_failure_at_reserve(trace.reserve_index);
            let first_failure = inner.alloc_object(region, 1, 1);
            let cleanup_failure = inner.unroot(child);
            assert_eq!(
                first_failure,
                Err(AllocationError::HostAllocation),
                "seed=0x{:06x} injected reserve",
                trace.seed
            );
            assert_eq!(cleanup_failure, Err(MemoryError::NotRooted));
            assert_eq!(inner.last_host_failure_site(), Some(trace.reserve_site));
            assert_eq!(inner.host_reserve_attempts(), trace.reserve_index);
            let after = inner.stats(region).unwrap();
            assert_eq!(after.allocation_failures, before.allocation_failures + 1);
            assert_eq!(after.committed_bytes, before.committed_bytes);
            assert_eq!(after.live_bytes, before.live_bytes);
            assert_eq!(after.peak_bytes, before.peak_bytes);
            assert_eq!(after.collection_count, before.collection_count);
            assert_eq!(inner.root_census(), Ok(before_census));
            check(
                &inner,
                region,
                census(1, 2),
                &[owner, child],
                &[],
                0,
                trace,
                "failed guarded reserve",
            );

            let created = inner.alloc_bytes(region, 1).unwrap();
            inner.root(created).unwrap();
            transient = Some(created);
            check(
                &inner,
                region,
                census(2, 3),
                &[owner, child, created],
                &[],
                0,
                trace,
                "successful guarded mutation",
            );
            match trace.exit {
                Exit::Early => Err("host early return after mutation"),
                Exit::Panic => panic!("host panic after mutation"),
            }
        }));
        match trace.exit {
            Exit::Early => assert!(matches!(
                aborted,
                Ok(Err("host early return after mutation"))
            )),
            Exit::Panic => assert!(aborted.is_err()),
        }
        let transient = transient.unwrap();
        check(
            &outer,
            region,
            census(2, 3),
            &[owner, child, transient],
            &[],
            0,
            trace,
            "inner dropped; outer retained",
        );
        assert_eq!(outer.collect(region), Err(MemoryError::CollectionForbidden));
        let before = outer.stats(region).unwrap();
        assert_eq!(
            outer.alloc_bytes(region, 200),
            Err(AllocationError::BudgetExceeded)
        );
        let after = outer.stats(region).unwrap();
        assert_eq!(after.allocation_failures, before.allocation_failures + 1);
        assert_eq!(after.committed_bytes, before.committed_bytes);
        assert_eq!(after.collection_count, before.collection_count);
        check(
            &outer,
            region,
            census(2, 3),
            &[owner, child, transient],
            &[],
            0,
            trace,
            "outer budget after abort",
        );
    }

    let transient = transient.unwrap();
    assert_eq!(
        session.close_region(region),
        Err(MemoryError::RegionHasRoots)
    );
    let collected = catch_unwind(AssertUnwindSafe(|| -> Result<(), &'static str> {
        session.collect(region).unwrap();
        match trace.exit {
            Exit::Early => Err("host early return after collect"),
            Exit::Panic => panic!("host panic after collect"),
        }
    }));
    match trace.exit {
        Exit::Early => assert!(matches!(
            collected,
            Ok(Err("host early return after collect"))
        )),
        Exit::Panic => assert!(collected.is_err()),
    }
    check(
        &session,
        region,
        census(2, 3),
        &[owner, child, transient],
        &[],
        1,
        trace,
        "post guard collection",
    );
    session.unroot(transient).unwrap();
    session.collect(region).unwrap();
    check(
        &session,
        region,
        census(1, 2),
        &[owner, child],
        &[transient],
        2,
        trace,
        "transient reclaimed",
    );
    session.unroot(owner).unwrap();
    check(
        &session,
        region,
        census(1, 1),
        &[owner, child],
        &[transient],
        2,
        trace,
        "duplicate root released once",
    );
    session.unroot(owner).unwrap();
    session.collect(region).unwrap();
    check(
        &session,
        region,
        census(0, 0),
        &[],
        &[owner, child, transient],
        3,
        trace,
        "all roots released",
    );
    let closed = catch_unwind(AssertUnwindSafe(|| -> Result<(), &'static str> {
        session.close_region(region).unwrap();
        match trace.exit {
            Exit::Early => Err("host early return after close"),
            Exit::Panic => panic!("host panic after close"),
        }
    }));
    match trace.exit {
        Exit::Early => assert!(matches!(closed, Ok(Err("host early return after close")))),
        Exit::Panic => assert!(closed.is_err()),
    }
    assert_eq!(session.root_census(), Ok(census(0, 0)));
    assert_eq!(
        session.root_census_region(region),
        Err(MemoryError::InvalidRegion)
    );
    assert_eq!(session.stats(region), Err(MemoryError::InvalidRegion));
    let row = session
        .memory_report()
        .regions
        .into_iter()
        .find(|r| r.id == region)
        .unwrap();
    assert!(!row.is_open);
    assert_eq!(row.committed_bytes, 0);
    assert_eq!(row.live_bytes, 0);
    assert_eq!(row.collection_count, 3);
}

#[test]
fn fixed_host_unwind_corpus() {
    let filter = std::env::var("CGX2_REPLAY_SEED").ok();
    let mut count = 0;
    for trace in traces() {
        if filter.as_deref().is_some_and(|seed| {
            u64::from_str_radix(seed.trim_start_matches("0x"), 16).unwrap() != trace.seed
        }) {
            continue;
        }
        replay(trace);
        count += 1;
    }
    assert!(count > 0, "CGX2_REPLAY_SEED did not match a corpus row");
}
