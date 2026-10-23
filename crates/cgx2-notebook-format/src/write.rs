use std::collections::HashSet;

use crate::{check_document_size, valid_id, valid_mime, CellKind, Notebook, NotebookError, Output};

pub fn write_notebook(notebook: &Notebook) -> Result<String, NotebookError> {
    crate::assets::referenced_asset_digests(notebook)?;
    if notebook.version != 1 {
        return Err(NotebookError::UnsupportedVersion(
            notebook.version.to_string(),
        ));
    }
    let mut out = String::from("#!cgnb 1\n");
    let mut seen = HashSet::new();
    for cell in &notebook.cells {
        if !valid_id(&cell.id) {
            return Err(NotebookError::InvalidCellId(cell.id.clone()));
        }
        if !seen.insert(&cell.id) {
            return Err(NotebookError::DuplicateCellId(cell.id.clone()));
        }
        if cell.kind == CellKind::Markdown && !cell.outputs.is_empty() {
            return Err(NotebookError::InvalidOutput(
                "Markdown cells cannot have outputs".into(),
            ));
        }
        crate::provenance::validate_provenance(cell)?;
        out.push_str(match cell.kind {
            CellKind::Code => "%% code id=",
            CellKind::Markdown => "%% markdown id=",
        });
        out.push_str(&cell.id);
        out.push('\n');
        write_body(&mut out, &cell.source);
        for output in &cell.outputs {
            match output {
                Output::Text {
                    mime,
                    ordinal,
                    body,
                } => {
                    crate::validate_captured_text(mime, body)?;
                    out.push_str(&format!(
                        "%% output id={} mime={} ordinal={}\n",
                        cell.id, mime, ordinal
                    ));
                    write_body(&mut out, body);
                }
                Output::Asset {
                    mime,
                    ordinal,
                    sha256,
                } => {
                    validate_mime(mime)?;
                    if !crate::assets::valid_digest(sha256) {
                        return Err(NotebookError::InvalidDigest(sha256.clone()));
                    }
                    out.push_str(&format!(
                        "%% asset id={} mime={} ordinal={} sha256={}\n",
                        cell.id, mime, ordinal, sha256
                    ));
                }
            }
        }
        if let Some(p) = &cell.provenance {
            let package = p.package.as_ref().map_or_else(String::new, |package| {
                format!(
                    " manifest_sha256={} module_sha256={}{}",
                    package.manifest_sha256,
                    package
                        .modules
                        .iter()
                        .map(|(name, hash)| format!("{name}:{hash}"))
                        .collect::<Vec<_>>()
                        .join(","),
                    package
                        .root_sha256
                        .as_ref()
                        .map_or_else(String::new, |root| format!(" root_sha256={root}"))
                )
            });
            out.push_str(&format!("%% provenance id={} schema={} source_sha256={} prefix_sha256={} evaluator={} ordinal={} output_ordinal={} result={}{}{}\n",
                cell.id, p.schema_version, p.source_sha256, p.prefix_sha256, p.evaluator, p.ordinal,
                p.output_ordinal.map_or_else(|| "none".into(), |ordinal| ordinal.to_string()),
                match p.result { crate::ProvenanceResult::Success => "success", crate::ProvenanceResult::Failed => "failed" },
                package,
                if p.stale { " state=stale" } else { "" }));
        }
        check_document_size(out.len())?;
    }
    Ok(out)
}

fn validate_mime(mime: &str) -> Result<(), NotebookError> {
    if valid_mime(mime) {
        Ok(())
    } else {
        Err(NotebookError::InvalidOutput(format!(
            "invalid MIME type: {mime}"
        )))
    }
}

fn write_body(out: &mut String, body: &str) {
    for line in body.split_inclusive('\n') {
        if line.is_empty() {
            continue;
        }
        let slashes = line.bytes().take_while(|byte| *byte == b'\\').count();
        if line[slashes..].starts_with("%%") {
            out.push('\\');
        }
        out.push_str(line);
    }
    if !body.is_empty() && !body.ends_with('\n') {
        out.push_str("\n%% end\n");
    }
}
