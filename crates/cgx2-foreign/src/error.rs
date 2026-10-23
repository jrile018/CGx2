#[cfg(feature = "in-process")]
use cgx2_python_bridge::BridgeError;
use cgx2_python_bridge::PythonFailure;
use std::fmt;

/// Stable source-facing categories for the later `.cg` adapter.
#[derive(Debug)]
pub enum ForeignError {
    Declaration(String),
    BlockedCall {
        key: String,
    },
    Argument(String),
    ModuleUnavailable {
        module: String,
        failure: PythonFailure,
    },
    AttributeMissing {
        attribute: String,
        failure: PythonFailure,
    },
    PythonException(PythonFailure),
    ReturnConversion {
        expected: String,
        actual: String,
        failure: Option<PythonFailure>,
    },
}

impl ForeignError {
    pub fn category(&self) -> &'static str {
        match self {
            Self::Declaration(_) => "declaration",
            Self::BlockedCall { .. } => "blocked_call",
            Self::Argument(_) => "argument",
            Self::ModuleUnavailable { .. } => "module_unavailable",
            Self::AttributeMissing { .. } => "attribute_missing",
            Self::PythonException(_) => "python_exception",
            Self::ReturnConversion { .. } => "return_conversion",
        }
    }

    #[cfg(feature = "in-process")]
    pub(crate) fn from_bridge(error: BridgeError, during_return: bool) -> Self {
        match error {
            BridgeError::ModuleUnavailable { module, failure } => {
                Self::ModuleUnavailable { module, failure }
            }
            BridgeError::AttributeMissing { attribute, failure } => {
                Self::AttributeMissing { attribute, failure }
            }
            BridgeError::PythonException(failure) => Self::PythonException(failure),
            BridgeError::TypeConversion {
                expected,
                actual,
                failure,
            } if during_return => Self::ReturnConversion {
                expected: expected.into(),
                actual,
                failure,
            },
            BridgeError::TypeConversion {
                expected, actual, ..
            } => Self::Argument(format!("expected {expected}; got {actual}")),
        }
    }
}

impl fmt::Display for ForeignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Declaration(message) | Self::Argument(message) => f.write_str(message),
            Self::BlockedCall { key } => write!(f, "foreign call key {key:?} is not declared"),
            Self::ModuleUnavailable { module, failure } => {
                write!(
                    f,
                    "Python module {module:?} is unavailable: {}",
                    failure.message
                )
            }
            Self::AttributeMissing { attribute, failure } => {
                write!(
                    f,
                    "Python attribute {attribute:?} is missing: {}",
                    failure.message
                )
            }
            Self::PythonException(failure) => {
                write!(f, "Python {}: {}", failure.exception_type, failure.message)
            }
            Self::ReturnConversion {
                expected, actual, ..
            } => {
                write!(f, "expected Python return {expected}; got {actual}")
            }
        }
    }
}

impl std::error::Error for ForeignError {}
