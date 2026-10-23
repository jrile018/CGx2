use crate::{ForeignError, Registry, ValueShape};
use cgx2_python_bridge::{
    BridgeValue, MAX_KEYWORDS, MAX_LIVE_ARRAY_BYTES, MAX_VALUE_BYTES, MAX_VALUE_DEPTH,
    MAX_VALUE_ELEMENTS, PythonBridge, checked_f64_array_bytes,
};
use std::collections::BTreeMap;

impl Registry {
    /// Validate the exact key and every argument before importing a module or reading
    /// a Python attribute. Python calls themselves are not transactional.
    pub fn call(
        &self,
        bridge: &PythonBridge,
        key: &str,
        positional: &[BridgeValue],
        keyword: &[(&str, BridgeValue)],
    ) -> Result<BridgeValue, ForeignError> {
        let declared = self
            .calls
            .get(key)
            .ok_or_else(|| ForeignError::BlockedCall { key: key.into() })?;
        if positional.len() != declared.positional.len() {
            return Err(ForeignError::Argument(format!(
                "{key} expects {} positional arguments; got {}",
                declared.positional.len(),
                positional.len()
            )));
        }
        if keyword.len() > MAX_KEYWORDS {
            return Err(ForeignError::Argument(format!(
                "at most {MAX_KEYWORDS} keywords are supported"
            )));
        }
        let mut budget = InputBudget::default();
        for (index, (value, shape)) in positional.iter().zip(&declared.positional).enumerate() {
            check_shape(value, *shape, &format!("positional[{index}]"))?;
            budget.check(value, 0)?;
        }
        let mut supplied = BTreeMap::new();
        for (name, value) in keyword {
            let shape = declared
                .keywords
                .get(*name)
                .ok_or_else(|| ForeignError::Argument(format!("undeclared keyword {name:?}")))?;
            if supplied.insert(*name, value).is_some() {
                return Err(ForeignError::Argument(format!(
                    "duplicate keyword {name:?}"
                )));
            }
            check_shape(value, *shape, &format!("keyword {name:?}"))?;
            budget.add_bytes(name.len())?;
            budget.check(value, 0)?;
        }

        let mut target = bridge
            .import_module(&declared.module)
            .map_err(|error| ForeignError::from_bridge(error, false))?;
        for attribute in &declared.attribute {
            target = bridge
                .getattr(&target, attribute)
                .map_err(|error| ForeignError::from_bridge(error, false))?;
        }
        let result = bridge
            .call_kwargs(&target, positional, keyword)
            .map_err(|error| ForeignError::from_bridge(error, false))?;
        if declared.returns == ValueShape::PythonHandle {
            return Ok(BridgeValue::Handle(result));
        }
        if declared.returns == ValueShape::F64Array1DCopyV1 {
            return bridge
                .to_f64_array_1d_copy_v1(&result, budget.bytes + budget.array_bytes)
                .map(BridgeValue::F64Array1DCopyV1)
                .map_err(|error| ForeignError::from_bridge(error, true));
        }
        let copied = bridge
            .to_value(&result)
            .map_err(|error| ForeignError::from_bridge(error, true))?;
        if contains_handle(&copied) {
            return Err(ForeignError::ReturnConversion {
                expected: declared.returns.as_str().into(),
                actual: "nested Python handle".into(),
                failure: None,
            });
        }
        if shape_of(&copied) != declared.returns {
            return Err(ForeignError::ReturnConversion {
                expected: declared.returns.as_str().into(),
                actual: shape_of(&copied).as_str().into(),
                failure: None,
            });
        }
        Ok(copied)
    }
}

fn check_shape(
    value: &BridgeValue,
    expected: ValueShape,
    location: &str,
) -> Result<(), ForeignError> {
    let actual = shape_of(value);
    if actual != expected {
        return Err(ForeignError::Argument(format!(
            "{location} expects {}; got {}",
            expected.as_str(),
            actual.as_str()
        )));
    }
    Ok(())
}

fn shape_of(value: &BridgeValue) -> ValueShape {
    match value {
        BridgeValue::None => ValueShape::None,
        BridgeValue::Bool(_) => ValueShape::Bool,
        BridgeValue::I64(_) => ValueShape::I64,
        BridgeValue::F64(_) => ValueShape::F64,
        BridgeValue::F64Array1DCopyV1(_) => ValueShape::F64Array1DCopyV1,
        BridgeValue::String(_) => ValueShape::String,
        BridgeValue::Bytes(_) => ValueShape::Bytes,
        BridgeValue::List(_) => ValueShape::List,
        BridgeValue::Tuple(_) => ValueShape::Tuple,
        BridgeValue::Dict(_) => ValueShape::Dict,
        BridgeValue::Handle(_) => ValueShape::PythonHandle,
    }
}

fn contains_handle(value: &BridgeValue) -> bool {
    match value {
        BridgeValue::Handle(_) => true,
        BridgeValue::List(items) | BridgeValue::Tuple(items) => items.iter().any(contains_handle),
        BridgeValue::Dict(items) => items.values().any(contains_handle),
        _ => false,
    }
}

#[derive(Default)]
struct InputBudget {
    elements: usize,
    bytes: usize,
    array_bytes: usize,
}

impl InputBudget {
    fn add_bytes(&mut self, n: usize) -> Result<(), ForeignError> {
        self.bytes = self
            .bytes
            .checked_add(n)
            .ok_or_else(|| ForeignError::Argument("copied payload size overflow".into()))?;
        if self.bytes > MAX_VALUE_BYTES {
            return Err(ForeignError::Argument(format!(
                "copied payload exceeds {MAX_VALUE_BYTES} bytes"
            )));
        }
        self.check_aggregate()?;
        Ok(())
    }
    fn check_aggregate(&self) -> Result<(), ForeignError> {
        if self
            .bytes
            .checked_add(self.array_bytes)
            .is_none_or(|total| total > MAX_LIVE_ARRAY_BYTES)
        {
            return Err(ForeignError::Argument(
                "aggregate copied payload exceeds 16777216 bytes".into(),
            ));
        }
        Ok(())
    }

    fn check(&mut self, value: &BridgeValue, depth: usize) -> Result<(), ForeignError> {
        if depth > MAX_VALUE_DEPTH {
            return Err(ForeignError::Argument(format!(
                "value depth exceeds {MAX_VALUE_DEPTH}"
            )));
        }
        self.elements = self
            .elements
            .checked_add(1)
            .ok_or_else(|| ForeignError::Argument("value node count overflow".into()))?;
        if self.elements > MAX_VALUE_ELEMENTS {
            return Err(ForeignError::Argument(format!(
                "value nodes exceed {MAX_VALUE_ELEMENTS}"
            )));
        }
        match value {
            BridgeValue::F64Array1DCopyV1(values) => {
                if depth != 0 {
                    return Err(ForeignError::Argument(
                        "declared array must be top-level".into(),
                    ));
                }
                let bytes = checked_f64_array_bytes(values)
                    .map_err(|error| ForeignError::Argument(error.to_string()))?;
                self.array_bytes = self.array_bytes.checked_add(bytes).ok_or_else(|| {
                    ForeignError::Argument("aggregate array byte count overflow".into())
                })?;
                self.check_aggregate()?;
                Ok(())
            }
            BridgeValue::String(value) => self.add_bytes(value.len()),
            BridgeValue::Bytes(value) => self.add_bytes(value.len()),
            BridgeValue::List(values) | BridgeValue::Tuple(values) => {
                for value in values {
                    self.check(value, depth + 1)?;
                }
                Ok(())
            }
            BridgeValue::Dict(values) => {
                for (key, value) in values {
                    self.add_bytes(key.len())?;
                    self.check(value, depth + 1)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
