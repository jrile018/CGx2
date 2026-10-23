use pyo3::prelude::*;
use std::collections::BTreeMap;

/// An owned reference to an object in the linked CPython interpreter.
#[derive(Debug)]
pub struct PyHandle(pub(crate) Py<PyAny>);

impl Clone for PyHandle {
    fn clone(&self) -> Self {
        Python::attach(|py| Self(self.0.clone_ref(py)))
    }
}

/// Values copied across the boundary, except for `Handle` which owns a Python reference.
#[derive(Debug, Clone)]
pub enum BridgeValue {
    None,
    Bool(bool),
    I64(i64),
    F64(f64),
    /// CGx2-owned contiguous finite float64 payload for the declared copied-array route.
    F64Array1DCopyV1(Vec<f64>),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<BridgeValue>),
    Tuple(Vec<BridgeValue>),
    Dict(BTreeMap<String, BridgeValue>),
    Handle(PyHandle),
}
