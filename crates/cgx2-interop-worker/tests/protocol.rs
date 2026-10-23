use cgx2_interop_worker::v1::*;
use serde_json::json;
#[test]
fn golden_big_endian_json_vector_and_all_truncations() {
    let limits = Limits::default();
    let e = envelope(
        "s",
        1,
        Kind::Result,
        &Returned {
            value: WireValue::U64(42),
            state_bytes: 8,
        },
    )
    .unwrap();
    let bytes = encode(&e, &limits).unwrap();
    let expected = br#"{"schema_version":1,"session_id":"s","request_id":1,"kind":"result","payload":{"state_bytes":8,"value":{"type":"u64","value":42}}}"#;
    assert_eq!(&bytes[..4], &(expected.len() as u32).to_be_bytes());
    assert_eq!(&bytes[4..], expected);
    assert_eq!(decode(expected, &limits).unwrap(), e);
    for cut in 0..expected.len() {
        assert!(decode(&expected[..cut], &limits).is_err());
    }
    let mut extra = expected.to_vec();
    extra.push(b'X');
    assert!(decode(&extra, &limits).is_err());
}
#[test]
fn unknown_fields_kinds_versions_duplicate_keys_utf8_and_nesting_reject() {
    let limits = Limits::default();
    for bytes in [
        br#"{"schema_version":1,"session_id":"s","request_id":1,"kind":"new","payload":{}}"#.as_slice(),
        br#"{"schema_version":2,"session_id":"s","request_id":1,"kind":"result","payload":{}}"#,
        br#"{"schema_version":1,"session_id":"s","request_id":1,"kind":"result","payload":{},"extra":0}"#,
        br#"{"schema_version":1,"session_id":"s","request_id":1,"request_id":2,"kind":"result","payload":{}}"#,
        br#"{"schema_version":1,"session_id":"s","request_id":1,"kind":"result","payload":{"x":1,"x":2}}"#,
        br#"{"schema_version":1,"session_id":"s","request_id":1,"kind":"result","payload":1.5}"#,
        &[255],
    ] { assert!(decode(bytes, &limits).is_err(), "{bytes:?}"); }
    let e = envelope(
        "s",
        1,
        Kind::Result,
        &json!({"value":{"type":"unit"},"state_bytes":0,"extra":0}),
    )
    .unwrap();
    assert!(payload::<Returned>(&e).is_err());
    let mut deep = "0".to_string();
    for _ in 0..33 {
        deep = format!("[{deep}]");
    }
    let e = envelope("s", 1, Kind::Invoke, &json!({})).unwrap();
    let text = String::from_utf8(serde_json::to_vec(&e).unwrap())
        .unwrap()
        .replace("\"payload\":{}", &format!("\"payload\":{deep}"));
    assert!(decode(text.as_bytes(), &limits).is_err());
    assert!(decode(&vec![b' '; DEFAULT_BYTES + 1], &limits).is_err());
}
#[test]
fn copied_types_round_trip_without_nan_json_or_raw_addresses() {
    for value in [
        WireValue::Unit,
        WireValue::Bool(true),
        WireValue::I64(i64::MIN),
        WireValue::U64(u64::MAX),
        WireValue::F32Bits(f32::NAN.to_bits()),
        WireValue::F64Bits(f64::INFINITY.to_bits()),
        WireValue::String("héllo 🦀".into()),
        WireValue::Bytes("AAH/".into()),
        WireValue::List(vec![WireValue::Bool(true), WireValue::Bool(false)]),
        WireValue::Handle(Handle {
            session_id: "s".into(),
            id: 1,
            declared_type: "fixture".into(),
            owner: "child".into(),
            release: "session".into(),
        }),
    ] {
        let e = envelope(
            "s",
            1,
            Kind::Result,
            &Returned {
                value: value.clone(),
                state_bytes: 0,
            },
        )
        .unwrap();
        let bytes = encode(&e, &Limits::default()).unwrap();
        let result: Returned = payload(&decode(&bytes[4..], &Limits::default()).unwrap()).unwrap();
        assert_eq!(value, result.value);
    }
}
