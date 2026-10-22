//! Validated, allowlisted host calls through the CGx2 CPython bridge.

mod call;
mod declaration;
mod error;

pub use declaration::{CallDeclaration, Registry, ValueShape, parse_declarations};
pub use error::ForeignError;
