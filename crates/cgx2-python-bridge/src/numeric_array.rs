//! The declared float64 boundary. All buffers are acquired, inspected, copied, and
//! released while attached to the interpreter. The returned Vec owns its storage.
use crate::host::{BridgeError, PythonBridge, conversion_error, python_exception};
use crate::value::PyHandle;
use pyo3::buffer::{PyBuffer, PyUntypedBuffer};
use pyo3::prelude::*;

pub const MAX_F64_ARRAY_ELEMENTS: usize = 1_048_576;
pub const MAX_F64_ARRAY_BYTES: usize = 8_388_608;
pub const MAX_LIVE_ARRAY_BYTES: usize = 16_777_216;

pub fn checked_f64_array_bytes(values: &[f64]) -> Result<usize, BridgeError> {
    let bytes = values
        .len()
        .checked_mul(8)
        .ok_or_else(|| conversion_error("f64_array_1d_copy_v1 byte count", "overflow".into()))?;
    if values.len() > MAX_F64_ARRAY_ELEMENTS || bytes > MAX_F64_ARRAY_BYTES {
        return Err(conversion_error(
            "f64_array_1d_copy_v1 at most 1048576 elements and 8388608 bytes",
            values.len().to_string(),
        ));
    }
    if values.iter().any(|value| !value.is_finite()) {
        return Err(conversion_error(
            "finite f64_array_1d_copy_v1 elements",
            "nonfinite element".into(),
        ));
    }
    Ok(bytes)
}

pub(crate) fn to_python_array(py: Python<'_>, values: &[f64]) -> Result<Py<PyAny>, BridgeError> {
    checked_f64_array_bytes(values)?;
    let module = py
        .import("array")
        .map_err(|error| python_exception(py, error))?;
    let array = module
        .getattr("array")
        .and_then(|ctor| ctor.call1(("d", [0.0_f64])))
        .map_err(|error| python_exception(py, error))?;
    array
        .call_method1("__imul__", (values.len(),))
        .map_err(|error| python_exception(py, error))?;
    {
        let buffer = PyBuffer::<f64>::get(&array).map_err(|error| python_exception(py, error))?;
        buffer
            .copy_from_slice(py, values)
            .map_err(|error| python_exception(py, error))?;
    }
    Ok(array.unbind())
}

impl PythonBridge {
    /// Accepts only a native-endian, contiguous, rank-one float64 buffer.
    /// `input_bytes` counts all still-live CGx2-owned copied input payload for this call.
    pub fn to_f64_array_1d_copy_v1(
        &self,
        handle: &PyHandle,
        input_bytes: usize,
    ) -> Result<Vec<f64>, BridgeError> {
        Python::attach(|py| {
            let value = handle.0.bind(py);
            let actual = value.get_type().name().map_or_else(
                |_| "unknown".into(),
                |name| name.to_string_lossy().into_owned(),
            );
            let bad = |reason: &str| {
                conversion_error(
                    "f64_array_1d_copy_v1 native-endian contiguous 1D float64 buffer",
                    format!("{actual}: {reason}"),
                )
            };
            let raw = PyUntypedBuffer::get(value).map_err(|_| bad("no buffer"))?;
            if raw.dimensions() != 1 {
                return Err(bad("wrong rank"));
            }
            let format = raw.format().to_bytes();
            let native_format = matches!(format, b"d" | b"@d" | b"=d")
                || (cfg!(target_endian = "little") && format == b"<d")
                || (cfg!(target_endian = "big") && (format == b">d" || format == b"!d"));
            if raw.item_size() != 8 || !native_format {
                return Err(bad("wrong dtype or byte order"));
            }
            if !raw.is_c_contiguous() {
                return Err(bad("noncontiguous buffer"));
            }
            let len = raw.shape()[0];
            let bytes = len
                .checked_mul(8)
                .ok_or_else(|| bad("byte count overflow"))?;
            if len > MAX_F64_ARRAY_ELEMENTS
                || bytes > MAX_F64_ARRAY_BYTES
                || raw.len_bytes() != bytes
            {
                return Err(bad("length exceeds 1048576 elements or 8388608 bytes"));
            }
            if input_bytes
                .checked_add(bytes)
                .is_none_or(|total| total > MAX_LIVE_ARRAY_BYTES)
            {
                return Err(bad("aggregate copied payload exceeds 16777216 bytes"));
            }
            let typed = raw
                .into_typed::<f64>()
                .map_err(|_| bad("invalid float64 buffer"))?;
            let mut copied = vec![0.0; len];
            typed
                .copy_to_slice(py, &mut copied)
                .map_err(|error| python_exception(py, error))?;
            if copied.iter().any(|number| !number.is_finite()) {
                return Err(bad("nonfinite element"));
            }
            Ok(copied)
        })
    }
}
