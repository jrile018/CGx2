//! Deterministic public signatures for one checked CGx2 module.

mod error;
mod index;
mod render;

pub use error::IndexError;
pub use index::{
    build_index, build_index_with_imports, build_syntax_index, ApiContract, ApiForeign,
    ApiFunction, ApiIndex, ApiInstantiation, ApiLoopContract, ApiParameter, ApiRecord, ApiType,
    ApiTypeParameter, ApiVariant, ApiVariantCase,
};

mod memory_v1;
pub use memory_v1::{build_memory_index, ApiMemoryIndexV1};
mod memory_v2;
pub use memory_v2::{
    build_memory_index_v2, build_public_memory_index_v1, ApiMemoryIndexV2, ApiPublicMemoryIndexV1,
};
