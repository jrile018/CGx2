//! Python-free boundary limits and error metadata.
use std::fmt;

/// Limits apply independently to each call's combined arguments and to each `to_value` result.
pub const MAX_VALUE_DEPTH: usize = 16;
pub const MAX_VALUE_ELEMENTS: usize = 4096;
pub const MAX_VALUE_BYTES: usize = 1_048_576;
pub const MAX_KEYWORDS: usize = 128;
/// Maximum retained bytes across a Python failure's class, message, and traceback.
pub const MAX_PYTHON_FAILURE_BYTES: usize = 16_384;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonFailure {
    pub exception_type: String,
    pub message: String,
    pub traceback: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeError {
    ModuleUnavailable {
        module: String,
        failure: PythonFailure,
    },
    AttributeMissing {
        attribute: String,
        failure: PythonFailure,
    },
    TypeConversion {
        expected: &'static str,
        actual: String,
        failure: Option<PythonFailure>,
    },
    PythonException(PythonFailure),
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ModuleUnavailable { module, failure } => write!(
                f,
                "Python module {module:?} unavailable: {}: {}",
                failure.exception_type, failure.message
            ),
            Self::AttributeMissing { attribute, failure } => write!(
                f,
                "Python attribute {attribute:?} missing: {}: {}",
                failure.exception_type, failure.message
            ),
            Self::TypeConversion {
                expected, actual, ..
            } => write!(f, "cannot convert Python {actual} to {expected}"),
            Self::PythonException(failure) => {
                write!(f, "Python {}: {}", failure.exception_type, failure.message)
            }
        }
    }
}

impl std::error::Error for BridgeError {}
