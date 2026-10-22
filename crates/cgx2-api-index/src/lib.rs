//! Deterministic public signatures for one checked CGx2 module.

mod error;
mod index;
mod render;

pub use error::IndexError;
pub use index::{build_index, ApiFunction, ApiIndex, ApiParameter};
