#![cfg(feature = "in-process")]

use cgx2_python_bridge::{BridgeError, BridgeValue, PythonBridge};
use std::collections::BTreeMap;

fn bridge() -> PythonBridge {
    PythonBridge::new().unwrap()
}

#[test]
fn stdlib_keywords_are_forwarded_in_sorted_order() {
    let bridge = bridge();
    let json = bridge.import_module("json").unwrap();
    let dumps = bridge.getattr(&json, "dumps").unwrap();
    let mut object = BTreeMap::new();
    object.insert("é".into(), BridgeValue::String("雪".into()));
    object.insert("items".into(), BridgeValue::List(vec![BridgeValue::I64(1)]));
    let result = bridge
        .call_kwargs(
            &dumps,
            &[BridgeValue::Dict(object)],
            &[
                ("ensure_ascii", BridgeValue::Bool(false)),
                ("sort_keys", BridgeValue::Bool(true)),
            ],
        )
        .unwrap();
    assert!(
        matches!(bridge.to_value(&result).unwrap(), BridgeValue::String(value) if value == "{\"items\": [1], \"é\": \"雪\"}")
    );

    let collections = bridge.import_module("collections").unwrap();
    let counter = bridge.getattr(&collections, "Counter").unwrap();
    let result = bridge
        .call_kwargs(&counter, &[], &[("apples", BridgeValue::I64(2))])
        .unwrap();
    let apples = bridge.getattr(&result, "most_common").unwrap();
    assert!(matches!(
        bridge
            .to_value(&bridge.call(&apples, &[]).unwrap())
            .unwrap(),
        BridgeValue::List(_)
    ));
}

#[test]
fn keyword_errors_are_conversion_errors_before_call() {
    let bridge = bridge();
    let builtins = bridge.import_module("builtins").unwrap();
    let dict = bridge.getattr(&builtins, "dict").unwrap();
    for keywords in [
        vec![("a", BridgeValue::I64(1)), ("a", BridgeValue::I64(2))],
        vec![("", BridgeValue::I64(1))],
        vec![("two words", BridgeValue::I64(1))],
    ] {
        assert!(matches!(
            bridge.call_kwargs(&dict, &[], &keywords).unwrap_err(),
            BridgeError::TypeConversion { .. }
        ));
    }
    let many = (0..129)
        .map(|i| (format!("k{i}"), BridgeValue::I64(i)))
        .collect::<Vec<_>>();
    let keywords = many
        .iter()
        .map(|(key, value)| (key.as_str(), value.clone()))
        .collect::<Vec<_>>();
    assert!(matches!(
        bridge.call_kwargs(&dict, &[], &keywords).unwrap_err(),
        BridgeError::TypeConversion { .. }
    ));
    assert!(
        bridge
            .call_kwargs(&dict, &[], &[("π", BridgeValue::I64(1))])
            .is_ok()
    );

    let ordered = bridge
        .call_kwargs(
            &dict,
            &[],
            &[("z", BridgeValue::I64(1)), ("a", BridgeValue::I64(2))],
        )
        .unwrap();
    let keys = bridge.getattr(&ordered, "keys").unwrap();
    let keys = bridge.call(&keys, &[]).unwrap();
    let list = bridge.getattr(&builtins, "list").unwrap();
    let keys = bridge.call(&list, &[BridgeValue::Handle(keys)]).unwrap();
    assert!(
        matches!(bridge.to_value(&keys).unwrap(), BridgeValue::List(values) if matches!(values.as_slice(), [BridgeValue::String(a), BridgeValue::String(z)] if a == "a" && z == "z"))
    );
}

#[test]
fn nested_values_and_bytes_copy_both_directions() {
    let bridge = bridge();
    let builtins = bridge.import_module("builtins").unwrap();
    let list = bridge.getattr(&builtins, "list").unwrap();
    let mut data = BTreeMap::new();
    data.insert("blob".into(), BridgeValue::Bytes(vec![0, 255, 42]));
    data.insert(
        "nested".into(),
        BridgeValue::Tuple(vec![
            BridgeValue::String("雪".into()),
            BridgeValue::List(vec![BridgeValue::None]),
        ]),
    );
    let source = BridgeValue::Dict(data);
    let result = bridge
        .call(&list, &[BridgeValue::List(vec![source])])
        .unwrap();
    let BridgeValue::List(mut copied) = bridge.to_value(&result).unwrap() else {
        panic!("list copy expected")
    };
    let Some(BridgeValue::Dict(values)) = copied.pop() else {
        panic!("dictionary copy expected")
    };
    assert!(
        matches!(values.get("blob"), Some(BridgeValue::Bytes(bytes)) if bytes == &[0, 255, 42])
    );
    assert!(
        matches!(values.get("nested"), Some(BridgeValue::Tuple(items)) if matches!(items.as_slice(), [BridgeValue::String(text), BridgeValue::List(inner)] if text == "雪" && matches!(inner.as_slice(), [BridgeValue::None])))
    );
    let append = bridge.getattr(&result, "append").unwrap();
    bridge.call(&append, &[BridgeValue::I64(7)]).unwrap();
    assert_eq!(copied.len(), 0);
    assert!(
        matches!(bridge.to_value(&result).unwrap(), BridgeValue::List(values) if values.len() == 2)
    );

    let mut original = vec![BridgeValue::I64(1)];
    let python_copy = bridge
        .call(&list, &[BridgeValue::List(original.clone())])
        .unwrap();
    original.push(BridgeValue::I64(2));
    assert!(
        matches!(bridge.to_value(&python_copy).unwrap(), BridgeValue::List(values) if values.len() == 1)
    );
}

#[test]
fn cycles_fall_back_to_handles_and_limits_are_errors() {
    let bridge = bridge();
    let builtins = bridge.import_module("builtins").unwrap();
    let list = bridge.getattr(&builtins, "list").unwrap();
    let cyclic = bridge.call(&list, &[]).unwrap();
    let append = bridge.getattr(&cyclic, "append").unwrap();
    bridge
        .call(&append, &[BridgeValue::Handle(cyclic.clone())])
        .unwrap();
    assert!(
        matches!(bridge.to_value(&cyclic).unwrap(), BridgeValue::List(values) if matches!(values.as_slice(), [BridgeValue::Handle(_)]))
    );

    let mut deep = BridgeValue::None;
    for _ in 0..20 {
        deep = BridgeValue::List(vec![deep]);
    }
    assert!(matches!(
        bridge.call(&list, &[deep]).unwrap_err(),
        BridgeError::TypeConversion { .. }
    ));
    assert!(matches!(
        bridge
            .call(&list, &[BridgeValue::Bytes(vec![0; 1_048_577])])
            .unwrap_err(),
        BridgeError::TypeConversion { .. }
    ));
    assert!(matches!(
        bridge
            .call(&list, &[BridgeValue::List(vec![BridgeValue::None; 4_097])])
            .unwrap_err(),
        BridgeError::TypeConversion { .. }
    ));

    let bytes = bridge.getattr(&builtins, "bytes").unwrap();
    let oversized = bridge.call(&bytes, &[BridgeValue::I64(1_048_577)]).unwrap();
    assert!(matches!(
        bridge.to_value(&oversized).unwrap_err(),
        BridgeError::TypeConversion { .. }
    ));

    let range = bridge.getattr(&builtins, "range").unwrap();
    let numbers = bridge.call(&range, &[BridgeValue::I64(4_097)]).unwrap();
    let too_many = bridge.call(&list, &[BridgeValue::Handle(numbers)]).unwrap();
    assert!(matches!(
        bridge.to_value(&too_many).unwrap_err(),
        BridgeError::TypeConversion { .. }
    ));

    let mut python_deep = bridge.call(&list, &[]).unwrap();
    for _ in 0..17 {
        python_deep = bridge
            .call(
                &list,
                &[BridgeValue::Tuple(vec![BridgeValue::Handle(python_deep)])],
            )
            .unwrap();
    }
    assert!(matches!(
        bridge.to_value(&python_deep).unwrap_err(),
        BridgeError::TypeConversion { .. }
    ));

    let dict = bridge.getattr(&builtins, "dict").unwrap();
    let non_string_key = bridge
        .call(
            &dict,
            &[BridgeValue::List(vec![BridgeValue::Tuple(vec![
                BridgeValue::I64(1),
                BridgeValue::I64(2),
            ])])],
        )
        .unwrap();
    assert!(matches!(
        bridge.to_value(&non_string_key).unwrap(),
        BridgeValue::Handle(_)
    ));
}

#[test]
fn cloned_handles_alias_and_outlive_original_and_exceptions_are_distinct() {
    let bridge = bridge();
    let builtins = bridge.import_module("builtins").unwrap();
    let list = bridge.getattr(&builtins, "list").unwrap();
    let original = bridge.call(&list, &[]).unwrap();
    let clone = original.clone();
    drop(original);
    let append = bridge.getattr(&clone, "append").unwrap();
    bridge.call(&append, &[BridgeValue::I64(9)]).unwrap();
    assert!(
        matches!(bridge.to_value(&clone).unwrap(), BridgeValue::List(values) if matches!(values.as_slice(), [BridgeValue::I64(9)]))
    );

    let json = bridge.import_module("json").unwrap();
    let loads = bridge.getattr(&json, "loads").unwrap();
    assert!(matches!(
        bridge
            .call_kwargs(&loads, &[BridgeValue::String("{".into())], &[])
            .unwrap_err(),
        BridgeError::PythonException(_)
    ));

    let target = bridge
        .call(&bridge.getattr(&builtins, "object").unwrap(), &[])
        .unwrap();
    let sys = bridge.import_module("sys").unwrap();
    let getrefcount = bridge.getattr(&sys, "getrefcount").unwrap();
    let count = || {
        let result = bridge
            .call(&getrefcount, &[BridgeValue::Handle(target.clone())])
            .unwrap();
        let BridgeValue::I64(count) = bridge.to_value(&result).unwrap() else {
            panic!("refcount expected")
        };
        count
    };
    let initial = count();
    let math = bridge.import_module("math").unwrap();
    let sqrt = bridge.getattr(&math, "sqrt").unwrap();
    for _ in 0..100 {
        assert!(matches!(
            bridge
                .call(&sqrt, &[BridgeValue::Handle(target.clone())])
                .unwrap_err(),
            BridgeError::PythonException(_)
        ));
    }
    assert_eq!(count(), initial);
}
