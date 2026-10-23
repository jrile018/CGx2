use std::{collections::BTreeMap, fmt};

use pyo3::exceptions::{PyAttributeError, PyModuleNotFoundError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};

use crate::numeric_array::{MAX_LIVE_ARRAY_BYTES, checked_f64_array_bytes, to_python_array};
use crate::tensor::validate_torch_capability;
use crate::value::{BridgeValue, PyHandle};

/// Limits apply independently to each call's combined arguments and to each `to_value` result.
pub const MAX_VALUE_DEPTH: usize = 16;
pub const MAX_VALUE_ELEMENTS: usize = 4096;
pub const MAX_VALUE_BYTES: usize = 1_048_576;
pub const MAX_KEYWORDS: usize = 128;
/// Maximum retained bytes across a Python failure's class, message, and traceback.
pub const MAX_PYTHON_FAILURE_BYTES: usize = 16_384;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonFailure {
    pub exception_type: String,
    pub message: String,
    pub traceback: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeError {
    ModuleUnavailable {
        module: String,
        failure: PythonFailure,
    },
    AttributeMissing {
        attribute: String,
        failure: PythonFailure,
    },
    TypeConversion {
        expected: &'static str,
        actual: String,
        failure: Option<PythonFailure>,
    },
    PythonException(PythonFailure),
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ModuleUnavailable { module, failure } => write!(
                f,
                "Python module {module:?} unavailable: {}: {}",
                failure.exception_type, failure.message
            ),
            Self::AttributeMissing { attribute, failure } => write!(
                f,
                "Python attribute {attribute:?} missing: {}: {}",
                failure.exception_type, failure.message
            ),
            Self::TypeConversion {
                expected, actual, ..
            } => write!(f, "cannot convert Python {actual} to {expected}"),
            Self::PythonException(failure) => {
                write!(f, "Python {}: {}", failure.exception_type, failure.message)
            }
        }
    }
}

impl std::error::Error for BridgeError {}

/// The linked interpreter and the result of an actual CPU float32 PyTorch check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TorchProbe {
    pub python_version: String,
    pub python_prefix: String,
    pub status: TorchStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TorchStatus {
    Available {
        torch_version: String,
    },
    Unavailable {
        failure: PythonFailure,
    },
    Broken {
        torch_version: Option<String>,
        error: BridgeError,
    },
}

pub(crate) fn python_failure(py: Python<'_>, error: PyErr) -> PythonFailure {
    let exception_type = error.get_type(py).name().map_or_else(
        |_| "<unknown>".into(),
        |name| name.to_string_lossy().into_owned(),
    );
    let message = error.value(py).str().map_or_else(
        |_| error.to_string(),
        |text| text.to_string_lossy().into_owned(),
    );
    let traceback = error
        .traceback(py)
        .and_then(|tb| tb.format().ok())
        .unwrap_or_default();
    PythonFailure {
        exception_type: cap_utf8(exception_type, 256),
        message: cap_utf8(message, 4_096),
        traceback: cap_utf8(traceback, MAX_PYTHON_FAILURE_BYTES - 256 - 4_096),
    }
}

fn cap_utf8(mut value: String, max_bytes: usize) -> String {
    if value.len() > max_bytes {
        let mut end = max_bytes;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        value.truncate(end);
    }
    value
}

pub(crate) fn python_exception(py: Python<'_>, error: PyErr) -> BridgeError {
    BridgeError::PythonException(python_failure(py, error))
}

/// A host for the interpreter selected and linked when this crate was built.
#[derive(Debug, Clone)]
pub struct PythonBridge {
    python_version: String,
    python_prefix: String,
}

impl PythonBridge {
    pub fn new() -> Result<Self, BridgeError> {
        Python::attach(|py| {
            let sys = py
                .import("sys")
                .map_err(|error| python_exception(py, error))?;
            let python_version = sys
                .getattr("version")
                .and_then(|value| value.extract::<String>())
                .map_err(|error| python_exception(py, error))?;
            let python_prefix = sys
                .getattr("prefix")
                .and_then(|value| value.extract::<String>())
                .map_err(|error| python_exception(py, error))?;
            Ok(Self {
                python_version,
                python_prefix,
            })
        })
    }

    pub fn python_version(&self) -> &str {
        &self.python_version
    }
    pub fn python_prefix(&self) -> &str {
        &self.python_prefix
    }

    /// Probes tensor creation and typed arithmetic in the interpreter selected at build time.
    pub fn probe_torch(&self) -> TorchProbe {
        let status = Python::attach(|py| match py.import("torch") {
            Ok(module) => {
                let torch = PyHandle(module.into_any().unbind());
                self.probe_imported_torch(&torch)
            }
            Err(error) => {
                let is_absent = error.is_instance_of::<PyModuleNotFoundError>(py)
                    && error
                        .value(py)
                        .getattr("name")
                        .and_then(|name| name.extract::<String>())
                        .is_ok_and(|name| name == "torch");
                let failure = python_failure(py, error);
                if is_absent {
                    TorchStatus::Unavailable { failure }
                } else {
                    TorchStatus::Broken {
                        torch_version: None,
                        error: BridgeError::PythonException(failure),
                    }
                }
            }
        });
        TorchProbe {
            python_version: self.python_version.clone(),
            python_prefix: self.python_prefix.clone(),
            status,
        }
    }

    fn probe_imported_torch(&self, torch: &PyHandle) -> TorchStatus {
        let version =
            self.getattr(torch, "__version__")
                .and_then(|value| match self.to_value(&value)? {
                    BridgeValue::String(version) => Ok(version),
                    _ => Err(BridgeError::TypeConversion {
                        expected: "torch.__version__ string",
                        actual: "non-string version".into(),
                        failure: None,
                    }),
                });
        match version {
            Ok(torch_version) => match validate_torch_capability(torch) {
                Ok(()) => TorchStatus::Available { torch_version },
                Err(error) => TorchStatus::Broken {
                    torch_version: Some(torch_version),
                    error,
                },
            },
            Err(error) => TorchStatus::Broken {
                torch_version: None,
                error,
            },
        }
    }

    pub fn import_module(&self, name: &str) -> Result<PyHandle, BridgeError> {
        Python::attach(|py| {
            py.import(name)
                .map(|module| PyHandle(module.into_any().unbind()))
                .map_err(|error| {
                    if error.is_instance_of::<PyModuleNotFoundError>(py) {
                        BridgeError::ModuleUnavailable {
                            module: name.into(),
                            failure: python_failure(py, error),
                        }
                    } else {
                        python_exception(py, error)
                    }
                })
        })
    }

    pub fn getattr(&self, handle: &PyHandle, name: &str) -> Result<PyHandle, BridgeError> {
        Python::attach(|py| {
            handle
                .0
                .bind(py)
                .getattr(name)
                .map(|value| PyHandle(value.unbind()))
                .map_err(|error| {
                    if error.is_instance_of::<PyAttributeError>(py) {
                        BridgeError::AttributeMissing {
                            attribute: name.into(),
                            failure: python_failure(py, error),
                        }
                    } else {
                        python_exception(py, error)
                    }
                })
        })
    }

    pub fn call(&self, callable: &PyHandle, args: &[BridgeValue]) -> Result<PyHandle, BridgeError> {
        self.call_kwargs(callable, args, &[])
    }

    /// Calls with positional arguments and unique Python identifier keywords.
    /// Keywords are inserted in lexical order, regardless of the input order.
    pub fn call_kwargs(
        &self,
        callable: &PyHandle,
        positional: &[BridgeValue],
        keyword: &[(&str, BridgeValue)],
    ) -> Result<PyHandle, BridgeError> {
        if keyword.len() > MAX_KEYWORDS {
            return Err(conversion_error(
                "at most 128 keyword arguments",
                keyword.len().to_string(),
            ));
        }
        let mut sorted = BTreeMap::new();
        let mut budget = ValueBudget::default();
        for (name, value) in keyword {
            budget.add_bytes(name.len())?;
            if !valid_keyword_name(name)? {
                return Err(conversion_error(
                    "Python identifier keyword name",
                    (*name).into(),
                ));
            }
            if sorted.insert(*name, value).is_some() {
                return Err(conversion_error("unique keyword names", (*name).into()));
            }
        }
        for value in positional {
            budget.check_rust(value, 0)?;
        }
        for value in sorted.values() {
            budget.check_rust(value, 0)?;
        }
        Python::attach(|py| {
            let values = positional
                .iter()
                .map(|arg| bridge_to_python(py, arg))
                .collect::<Result<Vec<_>, _>>()?;
            let tuple = PyTuple::new(py, values).map_err(|error| python_exception(py, error))?;
            let kwargs = PyDict::new(py);
            for (name, value) in sorted {
                let value = bridge_to_python(py, value)?;
                kwargs
                    .set_item(name, value)
                    .map_err(|error| python_exception(py, error))?;
            }
            callable
                .0
                .bind(py)
                .call(tuple, Some(&kwargs))
                .map(|value| PyHandle(value.unbind()))
                .map_err(|error| python_exception(py, error))
        })
    }

    pub fn to_value(&self, handle: &PyHandle) -> Result<BridgeValue, BridgeError> {
        Python::attach(|py| {
            let value = handle.0.bind(py);
            let mut budget = ValueBudget::default();
            budget.check_python(py, value, 0, &mut Vec::new())?;
            python_to_bridge(py, value, &mut Vec::new())
        })
    }
}

fn valid_keyword_name(name: &str) -> Result<bool, BridgeError> {
    Python::attach(|py| {
        PyString::new(py, name)
            .call_method0("isidentifier")
            .and_then(|value| value.extract::<bool>())
            .map_err(|error| python_exception(py, error))
    })
}

pub(crate) fn conversion_error(expected: &'static str, actual: String) -> BridgeError {
    BridgeError::TypeConversion {
        expected,
        actual,
        failure: None,
    }
}

#[derive(Default)]
struct ValueBudget {
    elements: usize,
    bytes: usize,
    array_bytes: usize,
}

impl ValueBudget {
    fn add_element(&mut self, depth: usize) -> Result<(), BridgeError> {
        if depth > MAX_VALUE_DEPTH {
            return Err(conversion_error(
                "container depth at most 16",
                depth.to_string(),
            ));
        }
        self.elements += 1;
        if self.elements > MAX_VALUE_ELEMENTS {
            return Err(conversion_error(
                "at most 4096 value elements",
                self.elements.to_string(),
            ));
        }
        Ok(())
    }

    fn add_bytes(&mut self, amount: usize) -> Result<(), BridgeError> {
        self.bytes = self.bytes.saturating_add(amount);
        if self.bytes > MAX_VALUE_BYTES {
            return Err(conversion_error(
                "at most 1048576 payload bytes",
                self.bytes.to_string(),
            ));
        }
        self.check_aggregate()?;
        Ok(())
    }
    fn check_aggregate(&self) -> Result<(), BridgeError> {
        if self
            .bytes
            .checked_add(self.array_bytes)
            .is_none_or(|total| total > MAX_LIVE_ARRAY_BYTES)
        {
            return Err(conversion_error(
                "at most 16777216 aggregate copied payload bytes",
                "exceeded".into(),
            ));
        }
        Ok(())
    }

    fn check_rust(&mut self, value: &BridgeValue, depth: usize) -> Result<(), BridgeError> {
        self.add_element(depth)?;
        match value {
            BridgeValue::F64Array1DCopyV1(values) => {
                if depth != 0 {
                    return Err(conversion_error(
                        "top-level f64_array_1d_copy_v1",
                        "nested array".into(),
                    ));
                }
                let bytes = checked_f64_array_bytes(values)?;
                self.array_bytes = self.array_bytes.checked_add(bytes).ok_or_else(|| {
                    conversion_error("aggregate copied array bytes", "overflow".into())
                })?;
                self.check_aggregate()?;
                Ok(())
            }
            BridgeValue::String(value) => self.add_bytes(value.len()),
            BridgeValue::Bytes(value) => self.add_bytes(value.len()),
            BridgeValue::List(values) | BridgeValue::Tuple(values) => {
                for value in values {
                    self.check_rust(value, depth + 1)?;
                }
                Ok(())
            }
            BridgeValue::Dict(values) => {
                for (key, value) in values {
                    self.add_bytes(key.len())?;
                    self.check_rust(value, depth + 1)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn check_python(
        &mut self,
        py: Python<'_>,
        value: &Bound<'_, PyAny>,
        depth: usize,
        stack: &mut Vec<usize>,
    ) -> Result<(), BridgeError> {
        self.add_element(depth)?;
        if value.is_exact_instance_of::<PyString>() {
            return self.add_bytes(
                value
                    .cast::<PyString>()
                    .map_err(|e| python_exception(py, e.into()))?
                    .to_str()
                    .map_err(|e| python_exception(py, e))?
                    .len(),
            );
        }
        if value.is_exact_instance_of::<PyBytes>() {
            return self.add_bytes(
                value
                    .cast::<PyBytes>()
                    .map_err(|e| python_exception(py, e.into()))?
                    .as_bytes()
                    .len(),
            );
        }
        if value.is_exact_instance_of::<PyInt>() {
            value
                .extract::<i64>()
                .map_err(|e| BridgeError::TypeConversion {
                    expected: "i64",
                    actual: "int outside i64 range".into(),
                    failure: Some(python_failure(py, e)),
                })?;
        }
        let identity = value.as_ptr() as usize;
        if stack.contains(&identity) {
            return Ok(()); // the repeated object is kept as a handle
        }
        if value.is_exact_instance_of::<PyList>() {
            stack.push(identity);
            for item in value
                .cast::<PyList>()
                .map_err(|e| python_exception(py, e.into()))?
                .iter()
            {
                self.check_python(py, &item, depth + 1, stack)?;
            }
            stack.pop();
        } else if value.is_exact_instance_of::<PyTuple>() {
            stack.push(identity);
            for item in value
                .cast::<PyTuple>()
                .map_err(|e| python_exception(py, e.into()))?
                .iter()
            {
                self.check_python(py, &item, depth + 1, stack)?;
            }
            stack.pop();
        } else if value.is_exact_instance_of::<PyDict>() {
            let dict = value
                .cast::<PyDict>()
                .map_err(|e| python_exception(py, e.into()))?;
            if dict
                .iter()
                .any(|(key, _)| !key.is_exact_instance_of::<PyString>())
            {
                return Ok(()); // preserve dictionaries with non-string keys as handles
            }
            stack.push(identity);
            for (key, item) in dict.iter() {
                self.add_bytes(
                    key.cast::<PyString>()
                        .map_err(|e| python_exception(py, e.into()))?
                        .to_str()
                        .map_err(|e| python_exception(py, e))?
                        .len(),
                )?;
                self.check_python(py, &item, depth + 1, stack)?;
            }
            stack.pop();
        }
        Ok(())
    }
}

fn python_to_bridge(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
    stack: &mut Vec<usize>,
) -> Result<BridgeValue, BridgeError> {
    if value.is_none() {
        return Ok(BridgeValue::None);
    }
    if value.is_exact_instance_of::<PyBool>() {
        return value
            .extract::<bool>()
            .map(BridgeValue::Bool)
            .map_err(|e| python_exception(py, e));
    }
    if value.is_exact_instance_of::<PyInt>() {
        return value
            .extract::<i64>()
            .map(BridgeValue::I64)
            .map_err(|e| python_exception(py, e));
    }
    if value.is_exact_instance_of::<PyFloat>() {
        return value
            .extract::<f64>()
            .map(BridgeValue::F64)
            .map_err(|e| python_exception(py, e));
    }
    if value.is_exact_instance_of::<PyString>() {
        return value
            .extract::<String>()
            .map(BridgeValue::String)
            .map_err(|e| python_exception(py, e));
    }
    if value.is_exact_instance_of::<PyBytes>() {
        return Ok(BridgeValue::Bytes(
            value
                .cast::<PyBytes>()
                .map_err(|e| python_exception(py, e.into()))?
                .as_bytes()
                .to_vec(),
        ));
    }
    let identity = value.as_ptr() as usize;
    if stack.contains(&identity) {
        return Ok(BridgeValue::Handle(PyHandle(value.clone().unbind())));
    }
    if value.is_exact_instance_of::<PyList>() {
        stack.push(identity);
        let result = value
            .cast::<PyList>()
            .map_err(|e| python_exception(py, e.into()))?
            .iter()
            .map(|item| python_to_bridge(py, &item, stack))
            .collect::<Result<Vec<_>, _>>()?;
        stack.pop();
        return Ok(BridgeValue::List(result));
    }
    if value.is_exact_instance_of::<PyTuple>() {
        stack.push(identity);
        let result = value
            .cast::<PyTuple>()
            .map_err(|e| python_exception(py, e.into()))?
            .iter()
            .map(|item| python_to_bridge(py, &item, stack))
            .collect::<Result<Vec<_>, _>>()?;
        stack.pop();
        return Ok(BridgeValue::Tuple(result));
    }
    if value.is_exact_instance_of::<PyDict>() {
        let dict = value
            .cast::<PyDict>()
            .map_err(|e| python_exception(py, e.into()))?;
        if dict
            .iter()
            .any(|(key, _)| !key.is_exact_instance_of::<PyString>())
        {
            return Ok(BridgeValue::Handle(PyHandle(value.clone().unbind())));
        }
        stack.push(identity);
        let mut result = BTreeMap::new();
        for (key, item) in dict.iter() {
            result.insert(
                key.extract::<String>()
                    .map_err(|e| python_exception(py, e))?,
                python_to_bridge(py, &item, stack)?,
            );
        }
        stack.pop();
        return Ok(BridgeValue::Dict(result));
    }
    Ok(BridgeValue::Handle(PyHandle(value.clone().unbind())))
}

fn bridge_to_python(py: Python<'_>, arg: &BridgeValue) -> Result<Py<PyAny>, BridgeError> {
    match arg {
        BridgeValue::None => Ok(py.None()),
        BridgeValue::Bool(value) => Ok(value
            .into_pyobject(py)
            .map_err(|error| python_exception(py, error.into()))?
            .to_owned()
            .into_any()
            .unbind()),
        BridgeValue::I64(value) => Ok(value
            .into_pyobject(py)
            .map_err(|error| python_exception(py, error.into()))?
            .into_any()
            .unbind()),
        BridgeValue::F64(value) => Ok(value
            .into_pyobject(py)
            .map_err(|error| python_exception(py, error.into()))?
            .into_any()
            .unbind()),
        BridgeValue::F64Array1DCopyV1(values) => to_python_array(py, values),
        BridgeValue::String(value) => Ok(value
            .as_str()
            .into_pyobject(py)
            .map_err(|error| python_exception(py, error.into()))?
            .into_any()
            .unbind()),
        BridgeValue::Bytes(value) => Ok(PyBytes::new(py, value).into_any().unbind()),
        BridgeValue::List(values) => {
            let converted = values
                .iter()
                .map(|value| bridge_to_python(py, value))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(PyList::new(py, converted)
                .map_err(|error| python_exception(py, error))?
                .into_any()
                .unbind())
        }
        BridgeValue::Tuple(values) => {
            let converted = values
                .iter()
                .map(|value| bridge_to_python(py, value))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(PyTuple::new(py, converted)
                .map_err(|error| python_exception(py, error))?
                .into_any()
                .unbind())
        }
        BridgeValue::Dict(values) => {
            let dict = PyDict::new(py);
            for (key, value) in values {
                dict.set_item(key, bridge_to_python(py, value)?)
                    .map_err(|error| python_exception(py, error))?;
            }
            Ok(dict.into_any().unbind())
        }
        BridgeValue::Handle(value) => Ok(value.0.clone_ref(py)),
    }
}

#[cfg(test)]
mod tests {
    use pyo3::exceptions::PyValueError;
    use pyo3::types::PyModule;

    use super::*;

    #[test]
    fn incomplete_imported_torch_is_broken_with_reported_version() {
        let bridge = PythonBridge::new().unwrap();
        Python::attach(|py| {
            let module = PyModule::from_code(
                py,
                c"__version__ = 'fake-1'\nclass Tensor: pass\n",
                c"incomplete_torch.py",
                c"incomplete_torch",
            )
            .unwrap();
            let torch = PyHandle(module.into_any().unbind());
            assert!(matches!(
                bridge.probe_imported_torch(&torch),
                TorchStatus::Broken {
                    torch_version: Some(version),
                    ..
                } if version == "fake-1"
            ));
        });
    }

    #[test]
    fn python_failure_caps_unicode_message_without_losing_class() {
        Python::attach(|py| {
            let failure = python_failure(py, PyValueError::new_err("雪".repeat(10_000)));
            assert_eq!(failure.exception_type, "ValueError");
            assert!(failure.message.len() <= 4_096);
            assert!(failure.message.is_char_boundary(failure.message.len()));
            assert!(
                failure.exception_type.len() + failure.message.len() + failure.traceback.len()
                    <= MAX_PYTHON_FAILURE_BYTES
            );
        });
    }
}
