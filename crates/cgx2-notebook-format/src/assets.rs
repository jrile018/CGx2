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

pub(crate) fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn digest_bytes(bytes: &[u8]) -> String {
    let hash = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in hash {
        out.push_str(&format!("{byte:02x}"));
    }
    out
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

pub fn save_asset(root: &Path, bytes: &[u8]) -> Result<String, NotebookError> {
    check_asset_size(bytes.len())?;
    let digest = digest_bytes(bytes);
    fs::create_dir_all(root)?;
    let path = root.join(&digest);
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            if let Err(error) = file.write_all(bytes) {
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
    let path = root.join(digest);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(NotebookError::MissingAsset(digest.to_owned()))
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

pub fn export_self_contained(
    notebook: &Notebook,
    asset_root: &Path,
) -> Result<String, NotebookError> {
    let mut text = write_notebook(notebook)?;
    for digest in asset_references(notebook) {
        let bytes = load_asset(asset_root, &digest)?;
        text.push_str(&format!(
            "%% embedded sha256={} size={}\n",
            digest,
            bytes.len()
        ));
        text.push_str(&base64::engine::general_purpose::STANDARD.encode(bytes));
        text.push('\n');
        check_document_size(text.len())?;
    }
    Ok(text)
}

pub fn import_self_contained(text: &str, asset_root: &Path) -> Result<Notebook, NotebookError> {
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
    let (main_text, blocks) = if let Some(index) = first_embedded {
        text.split_at(index)
    } else {
        (text, "")
    };
    let notebook = parse_notebook(main_text)?;
    let references = asset_references(&notebook);
    let mut assets = BTreeMap::new();
    if !blocks.is_empty() {
        let mut lines = blocks.split_inclusive('\n');
        while let Some(raw_header) = lines.next() {
            let header = raw_header.trim_end_matches('\n');
            let words: Vec<&str> = header.split_ascii_whitespace().collect();
            let ["%%", "embedded", hash_field, size_field] = words.as_slice() else {
                return Err(NotebookError::InvalidOutput(format!(
                    "malformed embedded header: {header}"
                )));
            };
            let digest = hash_field
                .strip_prefix("sha256=")
                .ok_or_else(|| NotebookError::InvalidDigest((*hash_field).to_owned()))?;
            if !valid_digest(digest) {
                return Err(NotebookError::InvalidDigest(digest.to_owned()));
            }
            if !references.contains(digest) {
                return Err(NotebookError::InvalidOutput(format!(
                    "unreferenced embedded asset: {digest}"
                )));
            }
            let size: usize = size_field
                .strip_prefix("size=")
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| {
                    NotebookError::InvalidOutput(format!(
                        "invalid embedded asset size: {size_field}"
                    ))
                })?;
            check_asset_size(size)?;
            let raw_body = lines
                .next()
                .ok_or_else(|| NotebookError::InvalidBase64("missing embedded body".into()))?;
            let encoded = raw_body.strip_suffix('\n').unwrap_or(raw_body);
            if encoded.starts_with("%%") {
                return Err(NotebookError::InvalidBase64("missing embedded body".into()));
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|error| NotebookError::InvalidBase64(error.to_string()))?;
            check_asset_size(bytes.len())?;
            if bytes.len() != size {
                return Err(NotebookError::InvalidOutput(format!(
                    "embedded asset size mismatch: {digest}"
                )));
            }
            let actual = digest_bytes(&bytes);
            if actual != digest {
                return Err(NotebookError::HashMismatch {
                    expected: digest.to_owned(),
                    actual,
                });
            }
            if assets.insert(digest.to_owned(), bytes).is_some() {
                return Err(NotebookError::InvalidOutput(format!(
                    "duplicate embedded asset: {digest}"
                )));
            }
        }
    }
    for digest in references {
        if !assets.contains_key(&digest) {
            return Err(NotebookError::MissingAsset(digest));
        }
    }
    for (digest, bytes) in assets {
        let actual = save_asset(asset_root, &bytes)?;
        debug_assert_eq!(actual, digest);
    }
    Ok(notebook)
}

fn asset_references(notebook: &Notebook) -> BTreeSet<String> {
    notebook
        .cells
        .iter()
        .flat_map(|cell| &cell.outputs)
        .filter_map(|output| {
            if let Output::Asset { sha256, .. } = output {
                Some(sha256.clone())
            } else {
                None
            }
        })
        .collect()
}
