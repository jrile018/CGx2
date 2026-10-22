mod error;
mod eval;

pub use error::RunError;
pub use eval::{RunResult, Value};

pub fn run_source(source: &str, args: &[String]) -> Result<RunResult, RunError> {
    eval::run_source(source, args)
}
