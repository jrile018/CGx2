//! Wave 21 direct-runtime graph matrix, separate from the source/native fixtures.
use cgx2_runtime::{CollectionMode, MemoryError, Session};

fn mode(name: &str) -> CollectionMode {
    match name {
        "manual" => CollectionMode::Manual,
        "auto" => CollectionMode::Auto {
            trigger_bytes: 4096,
        },
        _ => panic!("unknown mode: {name}"),
    }
}

#[test]
fn checked_cross_region_reference_and_copy_survive_both_close_orders() {
    let corpus = include_str!("../corpus/wave21-region-transfer.csv");
    let mut cases = 0;
    for line in corpus.lines().skip(1) {
        let fields: Vec<_> = line.split(',').collect();
        assert_eq!(fields.len(), 4, "{line}");
        let mut session = Session::new();
        let source_region = session.create_region(mode(fields[1]), 4096).unwrap();
        let destination_region = session.create_region(mode(fields[2]), 4096).unwrap();
        let source = session.alloc_object(source_region, 4, 1).unwrap();
        session.root(source).unwrap();
        session.root(source).unwrap();
        let owned_copy = session.copy_managed_to_owned(source).unwrap();
        let destination = session
            .promote_owned(destination_region, owned_copy)
            .unwrap();
        session.root(destination).unwrap();
        assert_ne!(
            session.read_bytes(source).unwrap().as_ptr(),
            session.read_bytes(destination).unwrap().as_ptr(),
            "{line}"
        );
        let before = session.memory_report();
        let roots = session.root_census().unwrap();
        assert_eq!(
            session.set_reference(source, 0, Some(destination)),
            Err(MemoryError::CrossRegionEdge),
            "{line}"
        );
        assert_eq!(session.memory_report(), before, "{line}");
        assert_eq!(session.root_census().unwrap(), roots, "{line}");
        assert_eq!(session.reference(source, 0), Ok(None), "{line}");

        let (first_region, first_handle, second_region, second_handle) = if fields[3] == "source" {
            (source_region, source, destination_region, destination)
        } else {
            assert_eq!(fields[3], "destination");
            (destination_region, destination, source_region, source)
        };
        assert_eq!(
            session.close_region(first_region),
            Err(MemoryError::RegionHasRoots)
        );
        session.unroot(first_handle).unwrap();
        if first_handle == source {
            session.unroot(first_handle).unwrap();
        }
        session.close_region(first_region).unwrap();
        assert!(session.read_bytes(second_handle).is_ok(), "{line}");
        session.unroot(second_handle).unwrap();
        if second_handle == source {
            session.unroot(second_handle).unwrap();
        }
        session.close_region(second_region).unwrap();
        assert_eq!(
            session.root_census().unwrap().root_registrations,
            0,
            "{line}"
        );
        assert_eq!(session.memory_report().totals.committed_bytes, 0, "{line}");
        cases += 1;
    }
    assert_eq!(cases, 4);
}
