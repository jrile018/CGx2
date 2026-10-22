//! Focused seeded state transitions for allocation ordering. The v4 generator
//! and model remain frozen; these traces cover newly specified failure paths.
use cgx2_runtime::{AllocationError, CollectionMode, MemoryError, Session, SLOT_METADATA_BYTES};

const REGRESSION_SEEDS: [u64; 4] = [0, 1, 42, 255];

#[test]
fn preflight_failures_preserve_auto_state_until_a_valid_safepoint() {
    for seed in REGRESSION_SEEDS {
        let len = (seed % 7) as usize;
        let charge = SLOT_METADATA_BYTES + len;
        let mut session = Session::new();
        let region = session
            .create_region(
                CollectionMode::Auto {
                    trigger_bytes: charge,
                },
                charge,
            )
            .unwrap();
        let original = session.alloc_bytes(region, len).unwrap();
        let before = session.stats(region).unwrap();

        assert_eq!(
            session.alloc_bytes(region, usize::MAX),
            Err(AllocationError::HostAllocation),
            "seed={seed}"
        );
        assert_eq!(
            session.alloc_bytes(region, len + 1),
            Err(AllocationError::RegionLimit),
            "seed={seed}"
        );
        let failed = session.stats(region).unwrap();
        assert_eq!(
            failed.collection_count, before.collection_count,
            "seed={seed}"
        );
        assert_eq!(
            failed.committed_bytes, before.committed_bytes,
            "seed={seed}"
        );
        assert_eq!(failed.live_bytes, before.live_bytes, "seed={seed}");
        assert_eq!(
            failed.allocation_failures,
            before.allocation_failures + 2,
            "seed={seed}"
        );
        assert_eq!(
            session.read_bytes(original).unwrap().len(),
            len,
            "seed={seed}"
        );

        let replacement = session.alloc_bytes(region, len).unwrap();
        let after = session.stats(region).unwrap();
        assert_eq!(
            after.collection_count,
            before.collection_count + 1,
            "seed={seed}"
        );
        assert_eq!(after.total_collected_bytes, charge, "seed={seed}");
        assert_eq!(after.committed_bytes, charge, "seed={seed}");
        assert_eq!(
            session.read_bytes(original),
            Err(MemoryError::InvalidHandle)
        );
        assert_eq!(session.read_bytes(replacement).unwrap().len(), len);
    }
}

#[test]
fn capacity_pressure_reclaims_below_trigger_and_failure_keeps_rooted_handle() {
    for seed in REGRESSION_SEEDS {
        let len = (seed % 7) as usize;
        let charge = SLOT_METADATA_BYTES + len;
        let mut session = Session::new();
        let region = session
            .create_region(
                CollectionMode::Auto {
                    trigger_bytes: 3 * charge,
                },
                charge,
            )
            .unwrap();
        let stale = session.alloc_bytes(region, len).unwrap();
        let before = session.stats(region).unwrap();
        let retained = session.alloc_bytes(region, len).unwrap();
        let reclaimed = session.stats(region).unwrap();
        assert_eq!(
            reclaimed.collection_count,
            before.collection_count + 1,
            "seed={seed}"
        );
        assert_eq!(reclaimed.total_collected_bytes, charge, "seed={seed}");
        assert_eq!(reclaimed.committed_bytes, charge, "seed={seed}");
        assert_eq!(session.read_bytes(stale), Err(MemoryError::InvalidHandle));

        session.root(retained).unwrap();
        assert_eq!(
            session.alloc_bytes(region, len),
            Err(AllocationError::RegionLimit),
            "seed={seed}"
        );
        let failed = session.stats(region).unwrap();
        assert_eq!(
            failed.collection_count,
            reclaimed.collection_count + 1,
            "seed={seed}"
        );
        assert_eq!(
            failed.total_collected_bytes,
            reclaimed.total_collected_bytes
        );
        assert_eq!(failed.committed_bytes, charge, "seed={seed}");
        assert_eq!(failed.allocation_failures, 1, "seed={seed}");
        assert_eq!(session.read_bytes(retained).unwrap().len(), len);
    }
}
