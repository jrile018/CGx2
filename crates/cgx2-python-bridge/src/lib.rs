//! Native CGx2 to CPython bridge.

mod host;
mod numeric_array;
mod tensor;
mod value;

pub use host::{
    BridgeError, MAX_KEYWORDS, MAX_PYTHON_FAILURE_BYTES, MAX_VALUE_BYTES, MAX_VALUE_DEPTH,
    MAX_VALUE_ELEMENTS, PythonBridge, PythonFailure, TorchProbe, TorchStatus,
};
pub use numeric_array::{
    MAX_F64_ARRAY_BYTES, MAX_F64_ARRAY_ELEMENTS, MAX_LIVE_ARRAY_BYTES, checked_f64_array_bytes,
};
pub use tensor::TensorHandle;
pub use value::{BridgeValue, PyHandle};
