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
        let count = checked_element_count(shape)?;
        if count != values.len() {
            return Err(conversion(
                "tensor shape matching values",
                format!("shape has {count} elements; values has {}", values.len()),
            ));
        }
        let torch = self.import_module("torch")?;
        tensor_from_f32_with_torch(&torch, values, shape)
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

fn tensor_from_f32_with_torch(
    torch: &PyHandle,
    values: &[f32],
    shape: &[usize],
) -> Result<TensorHandle, BridgeError> {
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

/// Exercise a tiny real tensor, both typed operations, and their numeric results.
pub(crate) fn validate_torch_capability(torch: &PyHandle) -> Result<(), BridgeError> {
    let left = tensor_from_f32_with_torch(torch, &[2.0], &[1, 1])?;
    let right = tensor_from_f32_with_torch(torch, &[3.0], &[1, 1])?;
    Python::attach(|py| {
        let tensor_type = torch
            .0
            .bind(py)
            .getattr("Tensor")
            .map_err(|error| python_exception(py, error))?;
        if !left
            .0
            .0
            .bind(py)
            .is_instance(&tensor_type)
            .map_err(|error| python_exception(py, error))?
        {
            return Err(conversion("torch.Tensor", "other Python object"));
        }
        Ok(())
    })?;
    for result in [left.mul(&right)?, left.matmul(&right)?] {
        let value = Python::attach(|py| {
            result
                .0
                .0
                .bind(py)
                .call_method0("item")
                .and_then(|value| value.extract::<f32>())
                .map_err(|error| python_exception(py, error))
        })?;
        if value != 6.0 {
            return Err(conversion(
                "correct CPU float32 torch arithmetic",
                format!("tiny tensor result {value}"),
            ));
        }
    }
    Ok(())
}

impl TensorHandle {
    fn checked_shape(&self) -> Result<Vec<usize>, BridgeError> {
        let shape = self.shape()?;
        checked_element_count(&shape)?;
        Ok(shape)
    }

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

    /// Elementwise CPU float32 multiplication with identical shapes; never broadcasts.
    pub fn mul(&self, other: &Self) -> Result<Self, BridgeError> {
        let left_shape = self.checked_shape()?;
        let right_shape = other.checked_shape()?;
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
        self.call_checked_binary(other, "mul", &left_shape)
    }

    /// Matrix multiplication of rank-2 CPU float32 tensors `(m, k)` and `(k, n)`.
    pub fn matmul(&self, other: &Self) -> Result<Self, BridgeError> {
        let left_shape = self.checked_shape()?;
        let right_shape = other.checked_shape()?;
        if left_shape.len() != 2 || right_shape.len() != 2 {
            return Err(conversion(
                "two rank-2 tensor shapes",
                format!("{left_shape:?} and {right_shape:?}"),
            ));
        }
        if left_shape[1] != right_shape[0] {
            return Err(conversion(
                "matching matmul inner dimensions",
                format!("{left_shape:?} and {right_shape:?}"),
            ));
        }
        let output_shape = [left_shape[0], right_shape[1]];
        checked_element_count(&output_shape)?;
        self.dtype()?;
        other.dtype()?;
        self.device()?;
        other.device()?;
        self.call_checked_binary(other, "matmul", &output_shape)
    }

    fn call_checked_binary(
        &self,
        other: &Self,
        method: &str,
        expected_shape: &[usize],
    ) -> Result<Self, BridgeError> {
        let result = Python::attach(|py| {
            self.0
                .0
                .bind(py)
                .call_method1(method, (other.0.0.bind(py),))
                .map(|value| TensorHandle(PyHandle(value.unbind())))
                .map_err(|error| python_exception(py, error))
        })?;
        let actual_shape = result.checked_shape()?;
        if actual_shape != expected_shape {
            return Err(conversion(
                "typed operation result shape",
                format!("expected {expected_shape:?}, got {actual_shape:?}"),
            ));
        }
        result.dtype()?;
        result.device()?;
        Ok(result)
    }
}

fn checked_element_count(shape: &[usize]) -> Result<usize, BridgeError> {
    if shape
        .iter()
        .any(|&dimension| dimension > isize::MAX as usize)
    {
        return Err(conversion(
            "tensor dimensions representable by Python",
            format!("dimension exceeds {}", isize::MAX),
        ));
    }
    shape
        .iter()
        .try_fold(1usize, |count, dimension| count.checked_mul(*dimension))
        .filter(|&count| count <= isize::MAX as usize)
        .ok_or_else(|| {
            conversion(
                "tensor shape with representable element count",
                "shape product exceeds signed Python size",
            )
        })
}

#[cfg(test)]
mod tests {
    use std::ffi::CString;

    use pyo3::types::PyModule;

    use super::*;

    fn fake_tensor(shape: &[usize], dtype: &str, device: &str) -> TensorHandle {
        Python::attach(|py| {
            let source = CString::new(
                "class FakeTensor:\n    def __init__(self, shape, dtype, device):\n        self.shape = shape\n        self.dtype = dtype\n        self.device = device\n    def mul(self, other):\n        raise AssertionError('mul was called')\n    def matmul(self, other):\n        raise AssertionError('matmul was called')\n",
            )
            .unwrap();
            let module =
                PyModule::from_code(py, &source, c"fake_tensor.py", c"fake_tensor").unwrap();
            let object = module
                .getattr("FakeTensor")
                .unwrap()
                .call1((shape.to_vec(), dtype, device))
                .unwrap();
            TensorHandle(PyHandle(object.unbind()))
        })
    }

    #[test]
    fn mul_rejects_broadcast_shapes_before_calling_python() {
        let left = fake_tensor(&[2, 1], "torch.float32", "cpu");
        let right = fake_tensor(&[2, 3], "torch.float32", "cpu");
        assert!(matches!(
            left.mul(&right),
            Err(BridgeError::TypeConversion {
                expected: "matching tensor shapes",
                ..
            })
        ));
    }

    #[test]
    fn matmul_rejects_rank_and_inner_dimension_before_calling_python() {
        let matrix = fake_tensor(&[2, 3], "torch.float32", "cpu");
        let vector = fake_tensor(&[3], "torch.float32", "cpu");
        assert!(matches!(
            matrix.matmul(&vector),
            Err(BridgeError::TypeConversion { .. })
        ));
        let mismatch = fake_tensor(&[4, 2], "torch.float32", "cpu");
        assert!(matches!(
            matrix.matmul(&mismatch),
            Err(BridgeError::TypeConversion { .. })
        ));
    }

    #[test]
    fn matmul_rejects_output_overflow_before_calling_python() {
        let huge = isize::MAX as usize;
        let left = fake_tensor(&[huge, 0], "torch.float32", "cpu");
        let right = fake_tensor(&[0, huge], "torch.float32", "cpu");
        assert!(matches!(
            left.matmul(&right),
            Err(BridgeError::TypeConversion { .. })
        ));
    }

    #[test]
    fn mul_rejects_input_element_count_overflow_before_calling_python() {
        let invalid = fake_tensor(&[isize::MAX as usize, 3], "torch.float32", "cpu");
        assert!(matches!(
            invalid.mul(&invalid),
            Err(BridgeError::TypeConversion { .. })
        ));
    }

    #[test]
    fn mul_rejects_element_count_above_signed_python_limit() {
        let invalid = fake_tensor(&[isize::MAX as usize, 2], "torch.float32", "cpu");
        assert!(matches!(
            invalid.mul(&invalid),
            Err(BridgeError::TypeConversion {
                expected: "tensor shape with representable element count",
                ..
            })
        ));
    }

    #[test]
    fn typed_ops_reject_wrong_dtype_and_device_before_calling_python() {
        let good = fake_tensor(&[1, 1], "torch.float32", "cpu");
        let wrong_dtype = fake_tensor(&[1, 1], "torch.float64", "cpu");
        let wrong_device = fake_tensor(&[1, 1], "torch.float32", "cuda:0");
        assert!(matches!(
            good.mul(&wrong_dtype),
            Err(BridgeError::TypeConversion { .. })
        ));
        assert!(matches!(
            good.matmul(&wrong_device),
            Err(BridgeError::TypeConversion { .. })
        ));
    }

    #[test]
    fn tensor_creation_rejects_unrepresentable_dimension_before_importing_torch() {
        let bridge = PythonBridge::new().unwrap();
        assert!(matches!(
            bridge.tensor_from_f32(&[], &[usize::MAX, 0]),
            Err(BridgeError::TypeConversion { .. })
        ));
    }

    #[test]
    fn importing_a_named_torch_module_is_not_a_capability_proof() {
        Python::attach(|py| {
            let module = PyModule::from_code(
                py,
                c"__version__ = 'fake'\nclass Tensor: pass\n",
                c"fake_torch.py",
                c"fake_torch",
            )
            .unwrap();
            let handle = PyHandle(module.into_any().unbind());
            assert!(validate_torch_capability(&handle).is_err());
        });
    }

    #[test]
    fn typed_python_exception_keeps_class_message_and_traceback() {
        let left = fake_tensor(&[1], "torch.float32", "cpu");
        let right = fake_tensor(&[1], "torch.float32", "cpu");
        let Err(BridgeError::PythonException(failure)) = left.mul(&right) else {
            panic!("expected Python exception from fake tensor method");
        };
        assert_eq!(failure.exception_type, "AssertionError");
        assert_eq!(failure.message, "mul was called");
        assert!(failure.traceback.contains("fake_tensor.py"));
    }

    #[test]
    fn real_torch_rectangular_math_and_zero_sizes_or_explicit_skip() {
        let bridge = PythonBridge::new().unwrap();
        match bridge.import_module("torch") {
            Err(BridgeError::ModuleUnavailable { .. }) => {
                eprintln!(
                    "SKIPPED real PyTorch typed math: torch is absent from the linked Python interpreter"
                );
                return;
            }
            Err(error) => panic!("torch import failed unexpectedly: {error}"),
            Ok(_) => {}
        }
        let left = bridge
            .tensor_from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3])
            .unwrap();
        let other = bridge
            .tensor_from_f32(&[10.0, 20.0, 30.0, 40.0, 50.0, 60.0], &[2, 3])
            .unwrap();
        let multiplied = left.mul(&other).unwrap();
        assert_eq!(multiplied.shape().unwrap(), vec![2, 3]);
        assert_eq!(multiplied.dtype().unwrap(), "torch.float32");
        assert_eq!(multiplied.device().unwrap(), "cpu");
        assert_eq!(
            tensor_rows(&multiplied),
            vec![vec![10.0, 40.0, 90.0], vec![160.0, 250.0, 360.0]]
        );

        let right = bridge
            .tensor_from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[3, 2])
            .unwrap();
        let matrix = left.matmul(&right).unwrap();
        assert_eq!(matrix.shape().unwrap(), vec![2, 2]);
        assert_eq!(matrix.dtype().unwrap(), "torch.float32");
        assert_eq!(matrix.device().unwrap(), "cpu");
        assert_eq!(
            tensor_rows(&matrix),
            vec![vec![22.0, 28.0], vec![49.0, 64.0]]
        );

        let empty_left = bridge.tensor_from_f32(&[], &[0, 2]).unwrap();
        let empty_right = bridge.tensor_from_f32(&[], &[2, 0]).unwrap();
        let empty_product = empty_left.matmul(&empty_right).unwrap();
        assert_eq!(empty_product.shape().unwrap(), vec![0, 0]);
        assert_eq!(tensor_rows(&empty_product), Vec::<Vec<f32>>::new());
    }

    fn tensor_rows(tensor: &TensorHandle) -> Vec<Vec<f32>> {
        Python::attach(|py| {
            tensor
                .0
                .0
                .bind(py)
                .call_method0("tolist")
                .unwrap()
                .extract()
                .unwrap()
        })
    }
}
