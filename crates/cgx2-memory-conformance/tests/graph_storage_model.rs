//! Independent logical-slot oracle for the chunked runtime representation.
//! Library conformance; source acceptance is recorded separately.
use cgx2_runtime::{CollectionMode, Session, REFERENCE_SLOT_METADATA_BYTES, SLOT_METADATA_BYTES};

#[test]
fn bounded_graph_slot_model_checks_boundaries_aliases_cycles_and_reclamation() {
    for count in [0_usize, 1, 255, 256, 257, 511, 65_536] {
        for seed in 1..=4_u64 {
            let mut session = Session::try_new().unwrap();
            let region = session
                .create_region(CollectionMode::Manual, 8 << 20)
                .unwrap();
            let child = session
                .alloc_initialized_object(region, b"child", &[])
                .unwrap();
            session.root(child).unwrap();
            let graph = session.alloc_zeroed_graph(region, 257, count).unwrap();
            session.root(graph).unwrap();
            let chunks = count.div_ceil(255).max(1);
            let expected_minimum = 5
                + SLOT_METADATA_BYTES
                + 257
                + chunks * (24 + SLOT_METADATA_BYTES)
                + (count + chunks) * REFERENCE_SLOT_METADATA_BYTES;
            assert!(session.stats(region).unwrap().committed_bytes >= expected_minimum);
            let mut model = vec![0_u8; count]; // 0=None, 1=child, 2=self.
            let mut state = seed;
            for step in 0..64 {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                if count > 0 {
                    let index = (state as usize) % count;
                    let value = ((state >> 32) % 3) as u8;
                    model[index] = value;
                    session
                        .set_graph_reference(
                            graph,
                            index,
                            match value {
                                0 => None,
                                1 => Some(child),
                                _ => Some(graph),
                            },
                        )
                        .unwrap();
                }
                if step % 16 == 0 {
                    let mut observed = Vec::new();
                    session
                        .visit_graph_references(graph, |index, handle| {
                            assert_eq!(index, observed.len());
                            observed.push(match handle {
                                None => 0,
                                Some(h) if h == child => 1,
                                Some(h) if h == graph => 2,
                                _ => panic!("foreign graph edge"),
                            });
                        })
                        .unwrap();
                    assert_eq!(observed, model);
                    session.collect(region).unwrap();
                    assert_eq!(session.read_bytes(child).unwrap(), b"child");
                    assert_eq!(session.root_census().unwrap().root_registrations, 2);
                }
            }
            session.unroot(child).unwrap();
            session.unroot(graph).unwrap();
            session.collect(region).unwrap();
            assert_eq!(session.stats(region).unwrap().committed_bytes, 0);
            assert_eq!(session.root_census().unwrap().root_registrations, 0);
            session.close_region(region).unwrap();
        }
    }
}
