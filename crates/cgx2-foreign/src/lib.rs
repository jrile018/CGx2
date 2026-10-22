//! Validated, allowlisted host calls through the CGx2 CPython bridge.

mod call;
mod declaration;
mod error;

pub use cgx2_python_bridge::{BridgeValue, PythonBridge};
pub use declaration::{
    CallDeclaration, Registry, SourceCall, ValueShape, parse_declarations, registry_from_source,
};
pub use error::ForeignError;
