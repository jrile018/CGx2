use std::fmt;

use pyo3::exceptions::{PyAttributeError, PyModuleNotFoundError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyFloat, PyInt, PyString, PyTuple};

use crate::tensor::validate_torch_capability;
use crate::value::{BridgeValue, PyHandle};

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
        exception_type,
        message,
        traceback,
    }
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
        Python::attach(|py| {
            let values = args
                .iter()
                .map(|arg| bridge_to_python(py, arg))
                .collect::<Result<Vec<_>, _>>()?;
            let tuple = PyTuple::new(py, values).map_err(|error| python_exception(py, error))?;
            callable
                .0
                .bind(py)
                .call1(tuple)
                .map(|value| PyHandle(value.unbind()))
                .map_err(|error| python_exception(py, error))
        })
    }

    pub fn to_value(&self, handle: &PyHandle) -> Result<BridgeValue, BridgeError> {
        Python::attach(|py| {
            let value = handle.0.bind(py);
            if value.is_none() {
                return Ok(BridgeValue::None);
            }
            if value.is_instance_of::<PyBool>() {
                return value
                    .extract::<bool>()
                    .map(BridgeValue::Bool)
                    .map_err(|error| python_exception(py, error));
            }
            if value.is_instance_of::<PyInt>() {
                return value
                    .extract::<i64>()
                    .map(BridgeValue::I64)
                    .map_err(|error| BridgeError::TypeConversion {
                        expected: "i64",
                        actual: "int outside i64 range".into(),
                        failure: Some(python_failure(py, error)),
                    });
            }
            if value.is_instance_of::<PyFloat>() {
                return value
                    .extract::<f64>()
                    .map(BridgeValue::F64)
                    .map_err(|error| python_exception(py, error));
            }
            if value.is_instance_of::<PyString>() {
                return value
                    .extract::<String>()
                    .map(BridgeValue::String)
                    .map_err(|error| python_exception(py, error));
            }
            Ok(BridgeValue::Handle(handle.clone()))
        })
    }
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
        BridgeValue::String(value) => Ok(value
            .as_str()
            .into_pyobject(py)
            .map_err(|error| python_exception(py, error.into()))?
            .into_any()
            .unbind()),
        BridgeValue::Handle(value) => Ok(value.0.clone_ref(py)),
    }
}

#[cfg(test)]
mod tests {
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
}
