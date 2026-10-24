use cgx2_runtime::{
    AllocationError, CollectionMode, GraphError, GraphShape, MemoryError, PayloadError, Session,
    MAX_GRAPH_REFERENCES, MAX_OBJECT_PAYLOAD_BYTES,
};

#[test]
fn payload_writes_are_checked_atomic_and_do_not_allocate() {
    let mut session = Session::try_new().unwrap();
    let region = session.default_region();
    let object = session.alloc_object(region, 4, 0).unwrap();
    session.write_bytes(object, 1, b"cg").unwrap();
    let before = session.stats(region).unwrap();
    assert_eq!(session.read_bytes(object).unwrap(), b"\0cg\0");
    for offset in [4, usize::MAX] {
        assert_eq!(
            session.write_bytes(object, offset, b"bad"),
            Err(PayloadError::OutOfBounds)
        );
        assert_eq!(session.read_bytes(object).unwrap(), b"\0cg\0");
    }
    session.write_bytes(object, 4, b"").unwrap();
    assert_eq!(session.stats(region).unwrap(), before);
    session.collect(region).unwrap();
    assert_eq!(
        session.write_bytes(object, 0, b""),
        Err(PayloadError::Memory(MemoryError::InvalidHandle))
    );
}

#[test]
fn initialized_children_survive_pressure_before_parent_allocation() {
    let mut session = Session::try_new().unwrap();
    let region = session
        .create_region(CollectionMode::Auto { trigger_bytes: 1 }, 1 << 20)
        .unwrap();
    let child = session.alloc_object(region, 3, 0).unwrap();
    session.write_bytes(child, 0, b"one").unwrap();
    let parent = session
        .alloc_initialized_object(region, b"parent", &[Some(child), Some(child)])
        .unwrap();
    assert_eq!(session.root_census().unwrap().root_registrations, 0);
    session.root(parent).unwrap();
    session.collect(region).unwrap();
    assert_eq!(session.read_bytes(child).unwrap(), b"one");
    assert_eq!(session.reference(parent, 1).unwrap(), Some(child));
    session.unroot(parent).unwrap();
    session.collect(region).unwrap();
    assert_eq!(session.read_bytes(child), Err(MemoryError::InvalidHandle));
}

#[test]
fn maximum_collection_and_string_use_traced_chunks_without_reducing_bounds() {
    let mut session = Session::try_new().unwrap();
    let region = session.default_region();
    let child = session.alloc_object(region, 1, 0).unwrap();
    let mut payload = vec![b'x'; MAX_OBJECT_PAYLOAD_BYTES];
    payload[MAX_OBJECT_PAYLOAD_BYTES - 1] = b'z';
    let references = vec![Some(child); MAX_GRAPH_REFERENCES];
    let graph = session.alloc_graph(region, &payload, &references).unwrap();
    assert_eq!(
        session.graph_shape(graph).unwrap(),
        GraphShape {
            payload_bytes: payload.len(),
            references: references.len()
        }
    );
    assert_eq!(session.root_census().unwrap().root_registrations, 0);
    for index in [0, 254, 255, 256, MAX_GRAPH_REFERENCES - 1] {
        assert_eq!(session.graph_reference(graph, index).unwrap(), Some(child));
    }
    session.root(graph).unwrap();
    session
        .set_graph_reference(graph, 256, Some(graph))
        .unwrap();
    session
        .set_graph_reference(graph, MAX_GRAPH_REFERENCES - 1, Some(graph))
        .unwrap();
    session.collect(region).unwrap();
    let boundary = MAX_OBJECT_PAYLOAD_BYTES - 24;
    session
        .write_graph_bytes(graph, boundary - 1, b"abc")
        .unwrap();
    let mut actual = [0; 3];
    session
        .read_graph_bytes(graph, boundary - 1, &mut actual)
        .unwrap();
    assert_eq!(&actual, b"abc");
    session
        .read_graph_bytes(graph, MAX_OBJECT_PAYLOAD_BYTES - 1, &mut actual[..1])
        .unwrap();
    assert_eq!(actual[0], b'z');
    session.unroot(graph).unwrap();
    session.collect(region).unwrap();
    assert_eq!(session.stats(region).unwrap().committed_bytes, 0);
}

#[test]
fn zeroed_graphs_have_exact_shapes_and_allow_aliases_and_cycles() {
    let mut session = Session::try_new().unwrap();
    let region = session
        .create_region(CollectionMode::Manual, 8 << 20)
        .unwrap();
    for count in [0, 1, 255, 256, 257, 65_536] {
        let graph = session.alloc_zeroed_graph(region, 3, count).unwrap();
        session.root(graph).unwrap();
        if count != 0 {
            session
                .set_graph_reference(graph, count - 1, Some(graph))
                .unwrap();
        }
        let mut bytes = [1; 3];
        session.read_graph_bytes(graph, 0, &mut bytes).unwrap();
        assert_eq!(bytes, [0; 3]);
        assert_eq!(
            session.graph_reference(graph, count),
            Err(GraphError::OutOfBounds)
        );
        session.unroot(graph).unwrap();
        session.collect(region).unwrap();
        assert_eq!(session.stats(region).unwrap().live_bytes, 0);
    }
}

#[test]
fn invalid_regions_children_and_shapes_fail_before_allocation() {
    let mut session = Session::try_new().unwrap();
    let region = session.default_region();
    let other = session.create_region(CollectionMode::Manual, 1024).unwrap();
    let child = session.alloc_bytes(other, 1).unwrap();
    let before = session.stats(region).unwrap();
    assert_eq!(
        session.alloc_initialized_object(region, b"", &[Some(child)]),
        Err(GraphError::Memory(MemoryError::CrossRegionEdge))
    );
    assert_eq!(
        session.alloc_graph(region, b"", &[Some(child)]),
        Err(GraphError::Memory(MemoryError::CrossRegionEdge))
    );
    assert_eq!(
        session.alloc_zeroed_graph(region, 0, 65_537),
        Err(GraphError::ShapeLimit)
    );
    assert_eq!(
        session.alloc_zeroed_graph(region, usize::MAX, 0),
        Err(GraphError::ShapeLimit)
    );
    assert_eq!(session.stats(region).unwrap(), before);
    session.collect(other).unwrap();
    assert_eq!(
        session.alloc_graph(other, b"", &[Some(child)]),
        Err(GraphError::Memory(MemoryError::InvalidHandle))
    );
    let foreign = Session::try_new().unwrap();
    assert_eq!(
        session.alloc_zeroed_graph(foreign.default_region(), 0, 0),
        Err(GraphError::Memory(MemoryError::InvalidRegion))
    );
}

#[test]
fn malformed_headers_links_and_bounds_reject_without_output_or_payload_mutation() {
    let mut session = Session::try_new().unwrap();
    let region = session.default_region();
    let graph = session.alloc_zeroed_graph(region, 2, 257).unwrap();
    assert_eq!(
        session.write_graph_bytes(graph, usize::MAX, b"a"),
        Err(GraphError::OutOfBounds)
    );
    let mut output = [9; 3];
    assert_eq!(
        session.read_graph_bytes(graph, 0, &mut output),
        Err(GraphError::OutOfBounds)
    );
    assert_eq!(output, [9; 3]);
    session.set_reference(graph, 0, Some(graph)).unwrap();
    assert_eq!(session.graph_shape(graph), Err(GraphError::InvalidLayout));
    assert_eq!(
        session.write_graph_bytes(graph, 0, b"xx"),
        Err(GraphError::InvalidLayout)
    );
    session.write_bytes(graph, 0, b"oops").unwrap();
    assert_eq!(session.graph_shape(graph), Err(GraphError::InvalidLayout));
    assert_eq!(&session.read_bytes(graph).unwrap()[24..], &[0; 2]);
}

#[test]
fn nested_no_gc_budgets_charge_roots_and_chunks_across_regions_without_collection() {
    let mut session = Session::try_new().unwrap();
    let region = session
        .create_region(CollectionMode::Manual, 1 << 20)
        .unwrap();
    let default = session.default_region();
    let child = session.alloc_bytes(region, 1).unwrap();
    let before = session.memory_report().totals.collection_count;
    {
        let mut outer = session.try_no_gc(1 << 16).unwrap();
        {
            let mut inner = outer.try_no_gc(0).unwrap();
            assert_eq!(
                inner.alloc_graph(region, b"", &[Some(child)]),
                Err(GraphError::Memory(MemoryError::Allocation(
                    AllocationError::BudgetExceeded
                )))
            );
            assert_eq!(
                inner.alloc_zeroed_graph(default, 0, 257),
                Err(GraphError::Memory(MemoryError::Allocation(
                    AllocationError::BudgetExceeded
                )))
            );
            assert_eq!(inner.collect(region), Err(MemoryError::CollectionForbidden));
            assert_eq!(
                inner.collect(default),
                Err(MemoryError::CollectionForbidden)
            );
        }
        let graph = outer.alloc_graph(region, b"ok", &[Some(child)]).unwrap();
        outer.root(graph).unwrap();
    }
    assert_eq!(session.memory_report().totals.collection_count, before);
    assert_eq!(session.root_census().unwrap().root_registrations, 1);
}

#[test]
fn exact_graph_budget_includes_temporary_root_bookkeeping() {
    use cgx2_runtime::{Handle, REFERENCE_SLOT_METADATA_BYTES, SLOT_METADATA_BYTES};
    let charge = 24
        + SLOT_METADATA_BYTES
        + 2 * REFERENCE_SLOT_METADATA_BYTES
        + std::mem::size_of::<Handle>();
    for budget in [0, charge - 1, charge] {
        let mut session = Session::try_new().unwrap();
        let region = session.default_region();
        {
            let mut guard = session.try_no_gc(budget).unwrap();
            let result = guard.alloc_zeroed_graph(region, 0, 1);
            if budget == charge {
                assert!(result.is_ok());
            } else {
                assert_eq!(
                    result,
                    Err(GraphError::Memory(MemoryError::Allocation(
                        AllocationError::BudgetExceeded
                    )))
                );
            }
            assert_eq!(guard.stats(region).unwrap().collection_count, 0);
            assert_eq!(guard.root_census().unwrap().root_registrations, 0);
        }
        assert_eq!(session.stats(region).unwrap().collection_count, 0);
        session.collect(region).unwrap();
        assert_eq!(session.stats(region).unwrap().committed_bytes, 0);
    }
}

#[test]
fn bulk_roots_count_duplicates_and_validate_all_children_before_registration() {
    let mut session = Session::try_new().unwrap();
    let region = session.default_region();
    let stale = session.alloc_bytes(region, 0).unwrap();
    session.collect(region).unwrap();
    let live = session.alloc_bytes(region, 0).unwrap();
    let mut scope = session.scoped_roots();
    assert_eq!(
        scope.root_many(&[Some(live), Some(stale)]),
        Err(MemoryError::InvalidHandle)
    );
    assert_eq!(scope.root_census().unwrap().root_registrations, 0);
    scope
        .root_many(&[Some(live), None, Some(live), Some(live)])
        .unwrap();
    assert_eq!(scope.root_census().unwrap().root_registrations, 3);
    assert_eq!(scope.cleanup().released_roots, 3);
    assert_eq!(scope.cleanup().released_roots, 0);
    assert_eq!(scope.root_census().unwrap().root_registrations, 0);
}

#[cfg(feature = "fault-injection")]
#[test]
fn every_constructor_reserve_failure_cleans_temporary_roots_and_preserves_children() {
    let mut observed_failures = 0;
    for reserve in 1..40 {
        let mut session = Session::try_new().unwrap();
        let region = session
            .create_region(CollectionMode::Manual, 1 << 20)
            .unwrap();
        let child = session
            .alloc_initialized_object(region, b"original", &[])
            .unwrap();
        session.root(child).unwrap();
        session.inject_host_failure_at_reserve(reserve);
        let references = vec![Some(child); 257];
        let result = session.alloc_graph(region, b"payload", &references);
        assert_eq!(session.root_census().unwrap().root_registrations, 1);
        assert_eq!(session.read_bytes(child).unwrap(), b"original");
        assert_eq!(session.stats(region).unwrap().collection_count, 0);
        match result {
            Err(GraphError::Memory(MemoryError::Allocation(AllocationError::HostAllocation))) => {
                observed_failures += 1
            }
            Ok(handle) => assert_eq!(session.graph_shape(handle).unwrap().references, 257),
            other => panic!("unexpected failure at reserve {reserve}: {other:?}"),
        }
        session.unroot(child).unwrap();
        session.collect(region).unwrap();
        assert_eq!(session.stats(region).unwrap().committed_bytes, 0);
    }
    assert!(observed_failures >= 7);
}
