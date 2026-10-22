use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::host::{BridgeError, PythonBridge, python_exception, python_failure};
use crate::value::PyHandle;

/// A checked CPU float32 PyTorch tensor. Tensor storage remains owned by PyTorch.
#[derive(Debug, Clone)]
pub struct TensorHandle(PyHandle);

fn conversion(expected: &'static str, actual: impl Into<String>) -> BridgeError {
    BridgeError::TypeConversion {
        expected,
        actual: actual.into(),
        failure: None,
    }
}

impl PythonBridge {
    /// Copies `values` into a new CPU float32 tensor with the requested shape.
    pub fn tensor_from_f32(
        &self,
        values: &[f32],
        shape: &[usize],
    ) -> Result<TensorHandle, BridgeError> {
        let count = shape
            .iter()
            .try_fold(1usize, |size, dimension| size.checked_mul(*dimension))
            .ok_or_else(|| {
                conversion(
                    "tensor shape with representable element count",
                    "shape product overflow",
                )
            })?;
        if count != values.len() {
            return Err(conversion(
                "tensor shape matching values",
                format!("shape has {count} elements; values has {}", values.len()),
            ));
        }
        let torch = self.import_module("torch")?;
        Python::attach(|py| {
            let torch = torch.0.bind(py);
            let kwargs = PyDict::new(py);
            kwargs
                .set_item(
                    "dtype",
                    torch
                        .getattr("float32")
                        .map_err(|error| python_exception(py, error))?,
                )
                .map_err(|error| python_exception(py, error))?;
            kwargs
                .set_item("device", "cpu")
                .map_err(|error| python_exception(py, error))?;
            let tensor = torch
                .getattr("tensor")
                .and_then(|function| function.call((values.to_vec(),), Some(&kwargs)))
                .map_err(|error| python_exception(py, error))?;
            let reshaped = tensor
                .call_method1("reshape", (shape.to_vec(),))
                .map_err(|error| python_exception(py, error))?;
            Ok(TensorHandle(PyHandle(reshaped.unbind())))
        })
    }

    /// Checks a dynamically obtained object before exposing typed tensor operations.
    pub fn tensor_from_handle(&self, handle: PyHandle) -> Result<TensorHandle, BridgeError> {
        let torch = self.import_module("torch")?;
        Python::attach(|py| {
            let tensor_type = torch
                .0
                .bind(py)
                .getattr("Tensor")
                .map_err(|error| python_exception(py, error))?;
            let valid = handle
                .0
                .bind(py)
                .is_instance(&tensor_type)
                .map_err(|error| python_exception(py, error))?;
            if !valid {
                return Err(conversion("torch.Tensor", "other Python object"));
            }
            let tensor = TensorHandle(handle);
            tensor.dtype()?;
            tensor.device()?;
            tensor.shape()?;
            Ok(tensor)
        })
    }
}

impl TensorHandle {
    pub fn shape(&self) -> Result<Vec<usize>, BridgeError> {
        Python::attach(|py| {
            self.0
                .0
                .bind(py)
                .getattr("shape")
                .and_then(|shape| shape.extract::<Vec<usize>>())
                .map_err(|error| BridgeError::TypeConversion {
                    expected: "nonnegative tensor shape",
                    actual: "unreadable shape".into(),
                    failure: Some(python_failure(py, error)),
                })
        })
    }

    pub fn dtype(&self) -> Result<String, BridgeError> {
        Python::attach(|py| {
            let dtype = self
                .0
                .0
                .bind(py)
                .getattr("dtype")
                .and_then(|value| value.str())
                .map_err(|error| python_exception(py, error))?
                .to_string_lossy()
                .into_owned();
            if dtype != "torch.float32" {
                return Err(conversion("torch.float32", dtype));
            }
            Ok(dtype)
        })
    }

    pub fn device(&self) -> Result<String, BridgeError> {
        Python::attach(|py| {
            let device = self
                .0
                .0
                .bind(py)
                .getattr("device")
                .and_then(|value| value.str())
                .map_err(|error| python_exception(py, error))?
                .to_string_lossy()
                .into_owned();
            if device != "cpu" {
                return Err(conversion("cpu tensor", device));
            }
            Ok(device)
        })
    }

    pub fn add(&self, other: &Self) -> Result<Self, BridgeError> {
        let left_shape = self.shape()?;
        let right_shape = other.shape()?;
        if left_shape != right_shape {
            return Err(conversion(
                "matching tensor shapes",
                format!("{left_shape:?} and {right_shape:?}"),
            ));
        }
        self.dtype()?;
        other.dtype()?;
        self.device()?;
        other.device()?;
        Python::attach(|py| {
            self.0
                .0
                .bind(py)
                .call_method1("add", (other.0.0.bind(py),))
                .map(|value| TensorHandle(PyHandle(value.unbind())))
                .map_err(|error| python_exception(py, error))
        })
    }
}
