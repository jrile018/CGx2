//! Runtime-only lifetime evidence. Source/target evidence is a separate runner.
use cgx2_runtime::{MemoryError, Session};

#[test]
fn temporary_root_transfer_retains_one_registration_and_releases_obsolete_values() {
    let mut session = Session::try_new().unwrap();
    let region = session.default_region();
    let a = session.alloc_graph(region, &[1], &[]).unwrap();
    let b = session.alloc_graph(region, &[2], &[]).unwrap();
    let c = session.alloc_graph(region, &[3], &[]).unwrap();
    let mut scope = session.scoped_roots();
    scope.root(a).unwrap();
    let marker = scope.checkpoint();
    scope.root(b).unwrap();
    scope.root(b).unwrap();
    scope.root(c).unwrap();
    let cleanup = scope.finish_since(marker, Some(b));
    assert_eq!(cleanup.released_roots, 2);
    assert_eq!(cleanup.first_error, None);
    assert_eq!(scope.root_census().unwrap().root_registrations, 2);
    scope.collect(region).unwrap();
    assert!(scope.read_bytes(a).is_ok());
    assert!(scope.read_bytes(b).is_ok());
    assert_eq!(scope.read_bytes(c), Err(MemoryError::InvalidHandle));
    scope.release(b).unwrap();
    assert_eq!(scope.root_census().unwrap().root_registrations, 1);
    let out = scope.finish::<(), &str>(Err("primary"));
    assert_eq!(out.result, Err("primary"));
    assert_eq!(out.cleanup.released_roots, 1);
    assert_eq!(scope.root_census().unwrap().root_registrations, 0);
}

#[test]
fn guarded_scope_restores_its_registration_owner_on_unwind() {
    let mut session = Session::try_new().unwrap();
    let region = session.default_region();
    let a = session.alloc_graph(region, &[1], &[]).unwrap();
    let b = session.alloc_graph(region, &[2], &[]).unwrap();
    let mut scope = session.scoped_roots();
    scope.root(a).unwrap();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = scope.with_no_gc(1024, |guard| {
            guard.root(b).unwrap();
            assert_eq!(guard.memory_observation_v2().active_guards, 1);
            panic!("cooperative unwind fixture")
        });
    }));
    assert!(panic.is_err());
    assert_eq!(scope.memory_observation_v2().active_guards, 0);
    assert_eq!(scope.root_census().unwrap().root_registrations, 2);
    assert_eq!(scope.memory_observation_v2().collection_count, 0);
    drop(scope);
    assert_eq!(session.root_census().unwrap().root_registrations, 0);
}

#[test]
fn cancellation_by_dropping_guarded_execution_scope_releases_every_root() {
    let mut session = Session::try_new().unwrap();
    let region = session.default_region();
    let a = session.alloc_graph(region, &[1], &[]).unwrap();
    {
        let mut scope = session.scoped_roots();
        scope
            .with_no_gc(1024, |guard| {
                guard.root(a).unwrap();
                assert_eq!(guard.root_census().unwrap().root_registrations, 1);
            })
            .unwrap();
    }
    assert_eq!(session.root_census().unwrap().root_registrations, 0);
    assert_eq!(session.memory_observation_v2().collection_count, 0);
    assert_eq!(session.memory_observation_v2().active_guards, 0);
}
