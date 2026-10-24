use cgx2_runtime::{AllocationError, Session};

#[test]
fn runtime_observer_does_not_refund_or_hide_failed_guard_lifetime() {
    let mut session = Session::try_new().unwrap();
    let start = session.memory_observation_v2();
    assert_eq!(start.charged_bytes, 0);
    let bytes = session.alloc_owned_bytes(8).unwrap();
    assert_eq!(session.memory_observation_v2().charged_bytes, 40);
    drop(bytes);
    assert_eq!(session.memory_observation_v2().charged_bytes, 40);
    let mut outer = session.try_no_gc(0).unwrap();
    let before = outer.memory_observation_v2();
    assert_eq!(before.active_guards, 1);
    assert!(matches!(
        outer.try_no_gc(0),
        Err(AllocationError::BudgetExceeded)
    ));
    assert_eq!(outer.memory_observation_v2(), before);
    assert!(matches!(
        outer.alloc_owned_bytes(8),
        Err(AllocationError::BudgetExceeded)
    ));
    assert_eq!(outer.memory_observation_v2(), before);
    drop(outer);
    let after = session.memory_observation_v2();
    assert_eq!(after.active_guards, 0);
    assert_eq!(after.collection_count, 0);
    assert_eq!(after.charged_bytes, before.charged_bytes);
}
