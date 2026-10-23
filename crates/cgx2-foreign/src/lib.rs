//! Validated Python metadata, isolated scalar calls, and opt-in CPython host calls.

#[cfg(feature = "in-process")]
mod call;
mod declaration;
mod error;
#[cfg(feature = "in-process")]
pub mod package_v1;
mod worker;

#[cfg(feature = "in-process")]
pub use cgx2_python_bridge::{BridgeValue, PythonBridge};
pub use declaration::{
    CallDeclaration, Registry, SourceCall, ValueShape, parse_declarations, registry_from_source,
};
pub use error::ForeignError;
pub use worker::{PythonProvenance, ScalarValue, WorkerConfig, WorkerOutput};
