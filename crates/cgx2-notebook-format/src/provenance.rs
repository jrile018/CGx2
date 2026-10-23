use sha2::{Digest, Sha256};

use crate::{Cell, CellKind, Notebook, NotebookError, Provenance, ProvenanceResult};

pub const EVALUATOR_PROFILE: &str = "cgx2-pure-scalar-v1";
pub const PROVENANCE_SCHEMA_VERSION: u32 = 1;
/// Schema 2 records a source/dependency digest in `prefix_sha256`.
pub const CAPTURE_SCHEMA_VERSION: u32 = 2;

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn source_hash(source: &str) -> String {
    digest(source.as_bytes())
}

/// SHA-256 over a domain tag, then each earlier code cell in file order.
/// Every ID and source is encoded as a big-endian u64 byte length followed by
/// its exact UTF-8 bytes. Markdown and the selected cell are excluded.
pub fn prefix_hash(cells: &[Cell], selected_index: usize) -> String {
    let mut hash = Sha256::new();
    hash.update(b"CGX2-PREFIX-v1\0");
    for cell in cells.iter().take(selected_index) {
        if cell.kind == CellKind::Code {
            for bytes in [cell.id.as_bytes(), cell.source.as_bytes()] {
                hash.update((bytes.len() as u64).to_be_bytes());
                hash.update(bytes);
            }
        }
    }
    format!("{:x}", hash.finalize())
}

pub fn make_provenance(
    notebook: &Notebook,
    index: usize,
    ordinal: u64,
    output_ordinal: Option<u64>,
) -> Provenance {
    Provenance {
        schema_version: PROVENANCE_SCHEMA_VERSION,
        source_sha256: source_hash(&notebook.cells[index].source),
        prefix_sha256: prefix_hash(&notebook.cells, index),
        evaluator: EVALUATOR_PROFILE.into(),
        ordinal,
        output_ordinal,
        result: ProvenanceResult::Success,
        package: None,
        stale: false,
    }
}

pub(crate) fn validate_provenance(cell: &Cell) -> Result<(), NotebookError> {
    let Some(provenance) = &cell.provenance else {
        return Ok(());
    };
    if cell.kind != CellKind::Code {
        return Err(NotebookError::InvalidProvenance(format!(
            "Markdown cell {} has provenance",
            cell.id
        )));
    }
    if ![PROVENANCE_SCHEMA_VERSION, CAPTURE_SCHEMA_VERSION].contains(&provenance.schema_version)
        || provenance.evaluator != EVALUATOR_PROFILE
        || !crate::assets::valid_digest(&provenance.source_sha256)
        || !crate::assets::valid_digest(&provenance.prefix_sha256)
        || cell.outputs.iter().any(|output| match output {
            crate::Output::Text { ordinal, .. } | crate::Output::Asset { ordinal, .. } => {
                Some(*ordinal) != provenance.output_ordinal
            }
        })
        || (cell.outputs.is_empty() != provenance.output_ordinal.is_none())
        || provenance.package.as_ref().is_some_and(|package| {
            !crate::assets::valid_digest(&package.manifest_sha256)
                || package
                    .root_sha256
                    .as_ref()
                    .is_some_and(|hash| !crate::assets::valid_digest(hash))
                || package.modules.is_empty()
                || package.modules.iter().any(|(name, hash)| {
                    name.split('.').any(|segment| {
                        segment.is_empty()
                            || !segment.chars().all(|c| c.is_alphanumeric() || c == '_')
                    }) || !crate::assets::valid_digest(hash)
                })
        })
    {
        return Err(NotebookError::InvalidProvenance(format!(
            "cell {} has unsupported or mismatched fields",
            cell.id
        )));
    }
    Ok(())
}
