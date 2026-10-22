//! Standalone, versioned CGx2 notebook documents and content-addressed assets.

mod assets;
mod parse;
mod provenance;
mod write;

use std::fmt;
use std::io;

pub use assets::{
    export_document, export_self_contained, import_self_contained, load_asset, open_notebook_bytes,
    parse_self_contained, save_asset, NotebookDocument, MAX_DISTINCT_ASSETS, MAX_TOTAL_ASSET_BYTES,
};
pub use parse::{parse_notebook, parse_notebook_bytes};
pub use provenance::{
    make_provenance, prefix_hash, source_hash, EVALUATOR_PROFILE, PROVENANCE_SCHEMA_VERSION,
};
pub use write::write_notebook;

pub const MAX_NOTEBOOK_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_ASSET_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notebook {
    pub version: u32,
    pub cells: Vec<Cell>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub id: String,
    pub kind: CellKind,
    pub source: String,
    pub outputs: Vec<Output>,
    pub provenance: Option<Provenance>,
}

/// A saved observation. It never establishes a live evaluator state on import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    pub schema_version: u32,
    pub source_sha256: String,
    pub prefix_sha256: String,
    pub evaluator: String,
    pub ordinal: u64,
    pub output_ordinal: Option<u64>,
    pub result: ProvenanceResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenanceResult {
    Success,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    Code,
    Markdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    Text {
        mime: String,
        ordinal: u64,
        body: String,
    },
    Asset {
        mime: String,
        ordinal: u64,
        sha256: String,
    },
}

#[derive(Debug)]
pub enum NotebookError {
    UnsupportedVersion(String),
    MalformedHeader { line: usize, header: String },
    DuplicateCellId(String),
    UnknownCellId(String),
    InvalidCellId(String),
    InvalidOutput(String),
    InvalidDigest(String),
    InvalidProvenance(String),
    InvalidUtf8,
    SizeLimit { what: &'static str, limit: usize },
    MissingAsset(String),
    HashMismatch { expected: String, actual: String },
    InvalidBase64(String),
    Io(io::Error),
}

impl fmt::Display for NotebookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(v) => write!(f, "unsupported notebook version: {v}"),
            Self::MalformedHeader { line, header } => {
                write!(f, "malformed header at line {line}: {header}")
            }
            Self::DuplicateCellId(id) => write!(f, "duplicate cell ID: {id}"),
            Self::UnknownCellId(id) => write!(f, "output references unknown cell ID: {id}"),
            Self::InvalidCellId(id) => write!(f, "invalid cell ID: {id}"),
            Self::InvalidOutput(reason) => write!(f, "invalid output: {reason}"),
            Self::InvalidDigest(digest) => write!(f, "invalid SHA-256 digest: {digest}"),
            Self::InvalidProvenance(reason) => write!(f, "invalid provenance: {reason}"),
            Self::InvalidUtf8 => write!(f, "notebook is not UTF-8"),
            Self::SizeLimit { what, limit } => write!(f, "{what} exceeds {limit} bytes"),
            Self::MissingAsset(digest) => write!(f, "missing asset: {digest}"),
            Self::HashMismatch { expected, actual } => {
                write!(f, "asset hash mismatch: expected {expected}, got {actual}")
            }
            Self::InvalidBase64(reason) => write!(f, "invalid base64: {reason}"),
            Self::Io(error) => write!(f, "I/O error: {error}"),
        }
    }
}

impl std::error::Error for NotebookError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for NotebookError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

pub(crate) fn valid_mime(mime: &str) -> bool {
    let Some((major, minor)) = mime.split_once('/') else {
        return false;
    };
    !major.is_empty()
        && !minor.is_empty()
        && !minor.contains('/')
        && mime
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$&^_.+-/".contains(&byte))
}

pub(crate) fn check_document_size(bytes: usize) -> Result<(), NotebookError> {
    if bytes > MAX_NOTEBOOK_BYTES {
        Err(NotebookError::SizeLimit {
            what: "notebook",
            limit: MAX_NOTEBOOK_BYTES,
        })
    } else {
        Ok(())
    }
}
