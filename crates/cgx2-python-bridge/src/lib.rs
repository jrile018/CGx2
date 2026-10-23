//! Python-free metadata by default; opt in to the linked CPython host with `in-process`.
#[cfg(feature = "in-process")]
mod host;
mod metadata;
#[cfg(feature = "in-process")]
mod numeric_array;
#[cfg(feature = "in-process")]
mod tensor;
#[cfg(feature = "in-process")]
mod value;

#[cfg(feature = "in-process")]
pub use host::{PythonBridge, TorchProbe, TorchStatus};
pub use metadata::*;
#[cfg(feature = "in-process")]
pub use numeric_array::{
    MAX_F64_ARRAY_BYTES, MAX_F64_ARRAY_ELEMENTS, MAX_LIVE_ARRAY_BYTES, checked_f64_array_bytes,
};
#[cfg(feature = "in-process")]
pub use tensor::TensorHandle;
#[cfg(feature = "in-process")]
pub use value::{BridgeValue, PyHandle};
