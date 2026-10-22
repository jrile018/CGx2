use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use base64::Engine;
use sha2::{Digest, Sha256};

use crate::{
    check_document_size, parse_notebook, write_notebook, Notebook, NotebookError, Output,
    MAX_ASSET_BYTES,
};

pub const MAX_DISTINCT_ASSETS: usize = 32;
pub const MAX_TOTAL_ASSET_BYTES: usize = 12 * 1024 * 1024;

/// A validated notebook and its rich assets, independent of a sidecar directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotebookDocument {
    pub notebook: Notebook,
    pub assets: BTreeMap<String, Vec<u8>>,
}

pub(crate) fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn check_asset_size(bytes: usize) -> Result<(), NotebookError> {
    if bytes > MAX_ASSET_BYTES {
        Err(NotebookError::SizeLimit {
            what: "asset",
            limit: MAX_ASSET_BYTES,
        })
    } else {
        Ok(())
    }
}

fn check_asset_root(root: &Path) -> Result<(), NotebookError> {
    for ancestor in root.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(NotebookError::InvalidOutput(
                    "asset directory crosses a symlink".into(),
                ));
            }
            Ok(meta) if ancestor == root && !meta.is_dir() => {
                return Err(NotebookError::InvalidOutput(
                    "asset directory is not a regular directory".into(),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub fn save_asset(root: &Path, bytes: &[u8]) -> Result<String, NotebookError> {
    check_asset_size(bytes.len())?;
    check_asset_root(root)?;
    let digest = digest_bytes(bytes);
    fs::create_dir_all(root)?;
    check_asset_root(root)?;
    let path = root.join(&digest);
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
                drop(file);
                let _ = fs::remove_file(&path);
                return Err(NotebookError::Io(error));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            load_asset(root, &digest)?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(digest)
}

pub fn load_asset(root: &Path, digest: &str) -> Result<Vec<u8>, NotebookError> {
    if !valid_digest(digest) {
        return Err(NotebookError::InvalidDigest(digest.to_owned()));
    }
    check_asset_root(root)?;
    let path = root.join(digest);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(NotebookError::MissingAsset(digest.to_owned()));
        }
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(NotebookError::InvalidOutput(format!(
            "asset is not a regular file: {digest}"
        )));
    }
    check_asset_size(metadata.len().try_into().unwrap_or(usize::MAX))?;
    let mut bytes = Vec::new();
    fs::File::open(&path)?
        .take((MAX_ASSET_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    check_asset_size(bytes.len())?;
    let actual = digest_bytes(&bytes);
    if actual != digest {
        return Err(NotebookError::HashMismatch {
            expected: digest.to_owned(),
            actual,
        });
    }
    Ok(bytes)
}

fn asset_references(notebook: &Notebook) -> Result<BTreeMap<String, String>, NotebookError> {
    let mut references = BTreeMap::new();
    for output in notebook.cells.iter().flat_map(|cell| &cell.outputs) {
        if let Output::Asset { mime, sha256, .. } = output {
            if !matches!(
                mime.as_str(),
                "image/png" | "image/jpeg" | "image/gif" | "image/webp"
            ) {
                return Err(NotebookError::InvalidOutput(format!(
                    "unsupported image MIME type: {mime}"
                )));
            }
            if !valid_digest(sha256) {
                return Err(NotebookError::InvalidDigest(sha256.clone()));
            }
            match references.insert(sha256.clone(), mime.clone()) {
                Some(previous) if previous != *mime => {
                    return Err(NotebookError::InvalidOutput(format!(
                        "conflicting MIME types for asset {sha256}"
                    )));
                }
                _ => {}
            }
        }
    }
    if references.len() > MAX_DISTINCT_ASSETS {
        return Err(NotebookError::SizeLimit {
            what: "asset count",
            limit: MAX_DISTINCT_ASSETS,
        });
    }
    Ok(references)
}

fn checked_total(total: usize, additional: usize) -> Result<usize, NotebookError> {
    let total = total.saturating_add(additional);
    if total > MAX_TOTAL_ASSET_BYTES {
        Err(NotebookError::SizeLimit {
            what: "all assets",
            limit: MAX_TOTAL_ASSET_BYTES,
        })
    } else {
        Ok(total)
    }
}

/// Read an ordinary notebook with sidecars, or an exported one without them.
pub fn open_notebook_bytes(
    bytes: &[u8],
    asset_root: &Path,
) -> Result<NotebookDocument, NotebookError> {
    check_document_size(bytes.len())?;
    let text = std::str::from_utf8(bytes).map_err(|_| NotebookError::InvalidUtf8)?;
    if text.lines().any(|line| line.starts_with("%% embedded")) {
        return parse_self_contained(text);
    }
    let notebook = parse_notebook(text)?;
    let mut assets = BTreeMap::new();
    let mut total = 0;
    for digest in asset_references(&notebook)?.into_keys() {
        let data = load_asset(asset_root, &digest)?;
        total = checked_total(total, data.len())?;
        assets.insert(digest, data);
    }
    Ok(NotebookDocument { notebook, assets })
}

/// Serialize a portable extension of native version 1 text. Old parsers reject
/// the embedded headers rather than silently discarding assets.
pub fn export_self_contained(
    notebook: &Notebook,
    asset_root: &Path,
) -> Result<String, NotebookError> {
    let references = asset_references(notebook)?;
    let mut assets = BTreeMap::new();
    let mut total = 0;
    for digest in references.keys() {
        let bytes = load_asset(asset_root, digest)?;
        total = checked_total(total, bytes.len())?;
        assets.insert(digest.clone(), bytes);
    }
    export_document(notebook, &assets)
}

pub fn export_document(
    notebook: &Notebook,
    assets: &BTreeMap<String, Vec<u8>>,
) -> Result<String, NotebookError> {
    let references = asset_references(notebook)?;
    if references.len() != assets.len() {
        return Err(NotebookError::InvalidOutput(
            "portable asset set differs from notebook references".into(),
        ));
    }
    let mut text = write_notebook(notebook)?;
    let mut total = 0;
    for (digest, mime) in references {
        let bytes = assets
            .get(&digest)
            .ok_or_else(|| NotebookError::MissingAsset(digest.clone()))?;
        check_asset_size(bytes.len())?;
        total = checked_total(total, bytes.len())?;
        let actual = digest_bytes(bytes);
        if actual != digest {
            return Err(NotebookError::HashMismatch {
                expected: digest,
                actual,
            });
        }
        text.push_str(&format!(
            "%% embedded sha256={actual} mime={mime} size={}\n",
            bytes.len()
        ));
        text.push_str(&base64::engine::general_purpose::STANDARD.encode(bytes));
        text.push('\n');
        check_document_size(text.len())?;
    }
    Ok(text)
}

/// Decode and verify every asset before returning any of them to callers.
pub fn parse_self_contained(text: &str) -> Result<NotebookDocument, NotebookError> {
    check_document_size(text.len())?;
    let mut offset = 0;
    let mut first_embedded = None;
    for line in text.split_inclusive('\n') {
        if line.starts_with("%% embedded") {
            first_embedded = Some(offset);
            break;
        }
        offset += line.len();
    }
    let (main_text, blocks) = first_embedded.map_or((text, ""), |index| text.split_at(index));
    let notebook = parse_notebook(main_text)?;
    let references = asset_references(&notebook)?;
    let mut assets = BTreeMap::new();
    let mut total = 0;
    let mut lines = blocks.split_inclusive('\n');
    while let Some(raw_header) = lines.next() {
        if !raw_header.ends_with('\n') {
            return Err(NotebookError::InvalidOutput(
                "unterminated embedded header".into(),
            ));
        }
        let header = raw_header.trim_end_matches('\n');
        let words: Vec<_> = header.split_ascii_whitespace().collect();
        let ["%%", "embedded", hash_field, mime_field, size_field] = words.as_slice() else {
            return Err(NotebookError::InvalidOutput(format!(
                "malformed embedded header: {header}"
            )));
        };
        let digest = hash_field
            .strip_prefix("sha256=")
            .ok_or_else(|| NotebookError::InvalidDigest((*hash_field).into()))?;
        if !valid_digest(digest) {
            return Err(NotebookError::InvalidDigest(digest.into()));
        }
        let mime = mime_field
            .strip_prefix("mime=")
            .ok_or_else(|| NotebookError::InvalidOutput("missing embedded MIME".into()))?;
        if references
            .get(digest)
            .is_none_or(|expected| expected != mime)
        {
            return Err(NotebookError::InvalidOutput(format!(
                "unreferenced asset or MIME mismatch: {digest}"
            )));
        }
        if assets.contains_key(digest) {
            return Err(NotebookError::InvalidOutput(format!(
                "duplicate embedded asset: {digest}"
            )));
        }
        let size: usize = size_field
            .strip_prefix("size=")
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| NotebookError::InvalidOutput("invalid embedded size".into()))?;
        check_asset_size(size)?;
        total = checked_total(total, size)?;
        let raw_body = lines
            .next()
            .ok_or_else(|| NotebookError::InvalidBase64("missing embedded body".into()))?;
        if !raw_body.ends_with('\n') {
            return Err(NotebookError::InvalidBase64(
                "unterminated embedded body".into(),
            ));
        }
        let encoded = raw_body.trim_end_matches('\n');
        let expected_len = size.div_ceil(3) * 4;
        if encoded.len() != expected_len {
            return Err(NotebookError::InvalidBase64(
                "encoded length differs from declared size".into(),
            ));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|error| NotebookError::InvalidBase64(error.to_string()))?;
        if bytes.len() != size {
            return Err(NotebookError::InvalidOutput(format!(
                "embedded asset size mismatch: {digest}"
            )));
        }
        let actual = digest_bytes(&bytes);
        if actual != digest {
            return Err(NotebookError::HashMismatch {
                expected: digest.into(),
                actual,
            });
        }
        assets.insert(digest.into(), bytes);
    }
    let expected: BTreeSet<_> = references.into_keys().collect();
    let actual: BTreeSet<_> = assets.keys().cloned().collect();
    if let Some(missing) = expected.difference(&actual).next() {
        return Err(NotebookError::MissingAsset(missing.clone()));
    }
    Ok(NotebookDocument { notebook, assets })
}

pub fn import_self_contained(text: &str, asset_root: &Path) -> Result<Notebook, NotebookError> {
    let document = parse_self_contained(text)?;
    check_asset_root(asset_root)?;
    for digest in document.assets.keys() {
        let path = asset_root.join(digest);
        if fs::symlink_metadata(&path).is_ok() {
            load_asset(asset_root, digest)?;
        }
    }
    for bytes in document.assets.values() {
        save_asset(asset_root, bytes)?;
    }
    Ok(document.notebook)
}
