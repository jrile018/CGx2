//! Persistent execution state for a CGx2 notebook.

mod deps;
mod session;

pub use deps::CellFacts;
pub use session::{CellExecutor, CellResult, CellStatus, NotebookSession, SessionError};
