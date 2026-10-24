use cgx2_runtime::{CollectionMode, MemoryError, Session};
fn main() {
    let mut session = Session::try_new().unwrap();
    let auto = session.create_region(CollectionMode::Auto { trigger_bytes: 1 }, 4096).unwrap();
    let manual = session.create_region(CollectionMode::Manual, 4096).unwrap();
    let (old, kept);
    {
        let mut outer = session.try_no_gc(4096).unwrap();
        let mut inner = outer.try_no_gc(2048).unwrap();
        old = inner.alloc_bytes(auto, 8).unwrap();
        inner.alloc_bytes(auto, 8).unwrap();
        kept = inner.alloc_bytes(manual, 8).unwrap();
        assert_eq!(inner.collect(auto), Err(MemoryError::CollectionForbidden));
        assert_eq!(inner.collect(manual), Err(MemoryError::CollectionForbidden));
        assert_eq!(inner.memory_observation_v2().collection_count, 0);
        assert!(inner.read_bytes(old).is_ok());
    }
    assert_eq!(session.memory_observation_v2().collection_count, 0);
    assert_eq!(session.memory_observation_v2().active_guards, 0);
    session.alloc_bytes(auto, 8).unwrap();
    assert_eq!(session.stats(auto).unwrap().collection_count, 1);
    assert!(session.read_bytes(old).is_err());
    assert!(session.read_bytes(kept).is_ok());
    assert_eq!(session.stats(manual).unwrap().collection_count, 0);
    session.close_region(manual).unwrap();
    assert_eq!(session.memory_observation_v2().collection_count, 1);
    assert_eq!(session.root_census().unwrap().root_registrations, 0);
    println!("protected=0 exit=0 next_auto=1 manual=0 roots=0");
}
