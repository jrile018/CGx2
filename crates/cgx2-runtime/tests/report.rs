use cgx2_runtime::{AllocationError, CollectionMode, MemoryError, Session, SLOT_METADATA_BYTES};

fn assert_valid_json(input: &str) {
    fn whitespace(bytes: &[u8], pos: &mut usize) {
        while bytes.get(*pos).is_some_and(u8::is_ascii_whitespace) {
            *pos += 1;
        }
    }
    fn value(bytes: &[u8], pos: &mut usize) {
        whitespace(bytes, pos);
        match bytes.get(*pos) {
            Some(b'{') => {
                *pos += 1;
                whitespace(bytes, pos);
                if bytes.get(*pos) != Some(&b'}') {
                    loop {
                        string(bytes, pos);
                        whitespace(bytes, pos);
                        assert_eq!(bytes.get(*pos), Some(&b':'));
                        *pos += 1;
                        value(bytes, pos);
                        whitespace(bytes, pos);
                        if bytes.get(*pos) != Some(&b',') {
                            break;
                        }
                        *pos += 1;
                    }
                }
                assert_eq!(bytes.get(*pos), Some(&b'}'));
                *pos += 1;
            }
            Some(b'[') => {
                *pos += 1;
                whitespace(bytes, pos);
                if bytes.get(*pos) != Some(&b']') {
                    loop {
                        value(bytes, pos);
                        whitespace(bytes, pos);
                        if bytes.get(*pos) != Some(&b',') {
                            break;
                        }
                        *pos += 1;
                    }
                }
                assert_eq!(bytes.get(*pos), Some(&b']'));
                *pos += 1;
            }
            Some(b'"') => string(bytes, pos),
            Some(b't') => literal(bytes, pos, b"true"),
            Some(b'f') => literal(bytes, pos, b"false"),
            Some(b'n') => literal(bytes, pos, b"null"),
            Some(b'0'..=b'9') => {
                let start = *pos;
                while bytes.get(*pos).is_some_and(u8::is_ascii_digit) {
                    *pos += 1;
                }
                assert!(std::str::from_utf8(&bytes[start..*pos])
                    .unwrap()
                    .parse::<u128>()
                    .is_ok());
            }
            other => panic!("unexpected JSON token: {other:?}"),
        }
    }
    fn string(bytes: &[u8], pos: &mut usize) {
        assert_eq!(bytes.get(*pos), Some(&b'"'));
        *pos += 1;
        while let Some(byte) = bytes.get(*pos) {
            *pos += 1;
            match byte {
                b'"' => return,
                b'\\' => {
                    assert!(bytes.get(*pos).is_some());
                    *pos += 1;
                }
                0..=31 => panic!("unescaped control character"),
                _ => {}
            }
        }
        panic!("unterminated string");
    }
    fn literal(bytes: &[u8], pos: &mut usize, expected: &[u8]) {
        assert_eq!(bytes.get(*pos..*pos + expected.len()), Some(expected));
        *pos += expected.len();
    }
    let bytes = input.as_bytes();
    let mut pos = 0;
    value(bytes, &mut pos);
    whitespace(bytes, &mut pos);
    assert_eq!(pos, bytes.len());
}

#[test]
fn report_has_stable_order_totals_and_terminal_rows() {
    let mut session = Session::new();
    let default = session.default_region();
    let manual = session.create_region(CollectionMode::Manual, 1024).unwrap();
    let auto = session
        .create_region(CollectionMode::Auto { trigger_bytes: 128 }, 1024)
        .unwrap();
    session.alloc_bytes(manual, 10).unwrap();
    session.alloc_bytes(auto, 20).unwrap();
    session.close_region(manual).unwrap();

    let report = session.memory_report();
    assert_eq!(report, session.memory_report());
    assert_eq!(report.regions.len(), 3);
    assert_eq!(report.regions[0].id, default);
    assert_eq!(report.regions[1].id, manual);
    assert_eq!(report.regions[2].id, auto);
    assert!(!report.regions[1].is_open);
    assert_eq!(report.regions[1].committed_bytes, 0);
    assert_eq!(report.regions[1].peak_bytes, SLOT_METADATA_BYTES + 10);
    assert_eq!(report.regions[2].committed_bytes, SLOT_METADATA_BYTES + 20);
    assert_eq!(report.totals.committed_bytes, SLOT_METADATA_BYTES + 20);
    assert_eq!(report.totals.peak_bytes, 2 * SLOT_METADATA_BYTES + 30);
    assert_eq!(session.stats(manual), Err(MemoryError::InvalidRegion));
    let handle = session.alloc_bytes(auto, 0).unwrap();
    assert_eq!(session.read_bytes(handle).unwrap(), []);
}

#[test]
fn collection_pauses_record_only_completed_cycles() {
    let mut session = Session::new();
    let manual = session.create_region(CollectionMode::Manual, 1024).unwrap();
    session.collect(manual).unwrap(); // zero live objects is still a cycle
    let before = session.memory_report();
    assert_eq!(before.regions[1].collection_count, 1);
    assert_eq!(before.regions[1].pause.sample_count, 1);
    assert_eq!(before.regions[1].pause.buckets.iter().sum::<u64>(), 1);
    assert!(before.regions[1].pause.min_ns <= before.regions[1].pause.max_ns);

    {
        let mut guard = session.no_gc(0);
        assert_eq!(guard.collect(manual), Err(MemoryError::CollectionForbidden));
        assert_eq!(
            guard.alloc_bytes(manual, 1),
            Err(AllocationError::BudgetExceeded)
        );
        assert_eq!(guard.memory_report().regions[1].pause.sample_count, 1);
    }
    assert_eq!(session.memory_report().regions[1].pause.sample_count, 1);
    session.collect(manual).unwrap();
    let after = session.memory_report();
    assert_eq!(after.regions[1].pause.sample_count, 2);
    assert_eq!(after.regions[1].collection_count, 2);
}

#[test]
fn report_rendering_has_a_versioned_stable_schema() {
    let session = Session::new();
    let report = session.memory_report();
    let json = report.to_json();
    assert_eq!(json, report.to_json());
    let session_id = json
        .split_once("\"session\":")
        .unwrap()
        .1
        .split(',')
        .next()
        .unwrap();
    let zeros = vec!["0"; 65].join(",");
    let expected = format!(
        "{{\"schema_version\":1,\"regions\":[{{\"region_id\":{{\"session\":{session_id},\"index\":0}},\"is_open\":true,\"mode\":\"auto\",\"trigger_bytes\":1048576,\"limit_bytes\":67108864,\"committed_bytes\":0,\"live_bytes\":0,\"peak_bytes\":0,\"allocation_failures\":0,\"collection_count\":0,\"total_collected_bytes\":0,\"pause\":{{\"sample_count\":0,\"total_ns\":0,\"min_ns\":0,\"max_ns\":0,\"buckets\":[{zeros}]}}}}],\"totals\":{{\"committed_bytes\":0,\"live_bytes\":0,\"peak_bytes\":0,\"allocation_failures\":0,\"collection_count\":0,\"total_collected_bytes\":0}}}}"
    );
    assert_eq!(json, expected);
    assert_valid_json(&json);
    assert_eq!(report.to_text(), report.to_text());
    assert!(report.to_text().starts_with("CGx2 memory report v1\n"));
}

#[test]
fn nested_guards_keep_two_regions_and_pause_counts_stable_on_failure() {
    let mut session = Session::new();
    let a = session
        .create_region(CollectionMode::Auto { trigger_bytes: 0 }, 1024)
        .unwrap();
    let b = session
        .create_region(CollectionMode::Auto { trigger_bytes: 0 }, 1024)
        .unwrap();
    let first = session.alloc_bytes(a, 1).unwrap();
    let second = session.alloc_bytes(b, 1).unwrap();
    let before = session.memory_report();
    let mut outer = session.no_gc(SLOT_METADATA_BYTES + 1);
    {
        let mut inner = outer.no_gc(SLOT_METADATA_BYTES + 1);
        assert_eq!(inner.collect(b), Err(MemoryError::CollectionForbidden));
        let third = inner.alloc_bytes(a, 1).unwrap();
        assert_eq!(inner.read_bytes(third).unwrap(), [0]);
        assert_eq!(
            inner.alloc_bytes(b, 1),
            Err(AllocationError::BudgetExceeded)
        );
        let during = inner.memory_report();
        assert_eq!(
            during.regions[1].collection_count,
            before.regions[1].collection_count
        );
        assert_eq!(
            during.regions[2].collection_count,
            before.regions[2].collection_count
        );
        assert_eq!(during.regions[1].pause, before.regions[1].pause);
        assert_eq!(during.regions[2].pause, before.regions[2].pause);
        assert_eq!(
            during.regions[2].committed_bytes,
            before.regions[2].committed_bytes
        );
        assert_eq!(
            during.regions[2].allocation_failures,
            before.regions[2].allocation_failures + 1
        );
    }
    assert_eq!(outer.collect(a), Err(MemoryError::CollectionForbidden));
    drop(outer);
    assert_eq!(session.read_bytes(first).unwrap(), [0]);
    assert_eq!(session.read_bytes(second).unwrap(), [0]);
    let b_before = session.memory_report().regions[2].clone();
    session.collect(a).unwrap();
    let after = session.memory_report();
    assert_eq!(after.regions[2], b_before);
    assert_eq!(
        after.totals.collection_count,
        after.regions.iter().map(|r| r.collection_count).sum()
    );
    assert_eq!(
        after.totals.total_collected_bytes,
        after.regions.iter().map(|r| r.total_collected_bytes).sum()
    );
}

#[test]
fn limit_failure_and_closed_region_leave_terminal_report_without_aliasing() {
    let mut session = Session::new();
    let region = session
        .create_region(CollectionMode::Manual, SLOT_METADATA_BYTES + 2)
        .unwrap();
    let handle = session.alloc_bytes(region, 2).unwrap();
    let before = session.memory_report();
    assert_eq!(
        session.alloc_bytes(region, 1),
        Err(AllocationError::RegionLimit)
    );
    let failed = session.memory_report();
    assert_eq!(
        failed.regions[1].committed_bytes,
        before.regions[1].committed_bytes
    );
    assert_eq!(failed.regions[1].live_bytes, before.regions[1].live_bytes);
    assert_eq!(failed.regions[1].peak_bytes, before.regions[1].peak_bytes);
    assert_eq!(failed.regions[1].collection_count, 0);
    assert_eq!(failed.regions[1].allocation_failures, 1);
    assert_eq!(session.read_bytes(handle).unwrap(), [0, 0]);
    session.close_region(region).unwrap();
    let terminal = session.memory_report().regions[1].clone();
    assert!(!terminal.is_open);
    assert_eq!(terminal.committed_bytes, 0);
    assert_eq!(terminal.live_bytes, 0);
    assert_eq!(terminal.peak_bytes, SLOT_METADATA_BYTES + 2);
    assert_eq!(terminal.allocation_failures, 1);
    let replacement = session.create_region(CollectionMode::Manual, 1024).unwrap();
    assert_ne!(region, replacement);
    assert_eq!(session.read_bytes(handle), Err(MemoryError::InvalidHandle));
    assert_eq!(
        session.alloc_bytes(region, 0),
        Err(AllocationError::InvalidRegion)
    );
    assert_eq!(session.memory_report().regions[1], terminal);
}
