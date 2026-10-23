use std::collections::{BTreeMap, HashMap};

use crate::{
    check_document_size, valid_id, valid_mime, Cell, CellKind, Notebook, NotebookError, Output,
    PackageProvenance, Provenance, ProvenanceResult,
};

enum Body {
    Cell(usize),
    TextOutput(usize, usize),
    None,
}

pub fn parse_notebook_bytes(bytes: &[u8]) -> Result<Notebook, NotebookError> {
    check_document_size(bytes.len())?;
    let text = std::str::from_utf8(bytes).map_err(|_| NotebookError::InvalidUtf8)?;
    parse_notebook(text)
}

pub fn parse_notebook(text: &str) -> Result<Notebook, NotebookError> {
    check_document_size(text.len())?;
    let mut lines = text.split_inclusive('\n').enumerate();
    let Some((_, first)) = lines.next() else {
        return Err(NotebookError::MalformedHeader {
            line: 1,
            header: String::new(),
        });
    };
    let version = first.trim_end_matches('\n');
    if version != "#!cgnb 1" {
        return Err(NotebookError::UnsupportedVersion(version.to_owned()));
    }
    if !first.ends_with('\n') {
        return Err(malformed(1, version));
    }

    let mut notebook = Notebook {
        version: 1,
        cells: Vec::new(),
    };
    let mut indices = HashMap::new();
    let mut body = Body::None;
    let mut buffer = String::new();
    let mut ended = false;

    for (zero_line, raw) in lines {
        let line = raw.trim_end_matches('\n');
        if line.starts_with("%%") {
            if !raw.ends_with('\n') {
                return Err(malformed(zero_line + 1, line));
            }
            if line == "%% end" {
                if matches!(body, Body::None) || ended || !buffer.ends_with('\n') {
                    return Err(malformed(zero_line + 1, line));
                }
                buffer.pop();
                ended = true;
                continue;
            }
            flush(&mut notebook, &body, &mut buffer)?;
            body = Body::None;
            ended = false;
            let words: Vec<&str> = line.split_ascii_whitespace().collect();
            match words.as_slice() {
                ["%%", "code" | "markdown", id_field] => {
                    let id = id_field
                        .strip_prefix("id=")
                        .ok_or_else(|| malformed(zero_line + 1, line))?;
                    if id.is_empty() {
                        return Err(malformed(zero_line + 1, line));
                    }
                    if !valid_id(id) {
                        return Err(NotebookError::InvalidCellId(id.to_owned()));
                    }
                    if indices.contains_key(id) {
                        return Err(NotebookError::DuplicateCellId(id.to_owned()));
                    }
                    let kind = if words[1] == "code" {
                        CellKind::Code
                    } else {
                        CellKind::Markdown
                    };
                    notebook.cells.push(Cell {
                        id: id.to_owned(),
                        kind,
                        source: String::new(),
                        outputs: Vec::new(),
                        provenance: None,
                    });
                    let index = notebook.cells.len() - 1;
                    indices.insert(id.to_owned(), index);
                    body = Body::Cell(index);
                }
                ["%%", "output", id_field, mime_field, ordinal_field] => {
                    let (index, mime, ordinal) = output_fields(
                        &notebook,
                        &indices,
                        id_field,
                        mime_field,
                        ordinal_field,
                        zero_line + 1,
                        line,
                    )?;
                    notebook.cells[index].outputs.push(Output::Text {
                        mime,
                        ordinal,
                        body: String::new(),
                    });
                    body = Body::TextOutput(index, notebook.cells[index].outputs.len() - 1);
                }
                ["%%", "asset", id_field, mime_field, ordinal_field, hash_field] => {
                    let (index, mime, ordinal) = output_fields(
                        &notebook,
                        &indices,
                        id_field,
                        mime_field,
                        ordinal_field,
                        zero_line + 1,
                        line,
                    )?;
                    let sha256 = hash_field
                        .strip_prefix("sha256=")
                        .ok_or_else(|| malformed(zero_line + 1, line))?;
                    if !crate::assets::valid_digest(sha256) {
                        return Err(NotebookError::InvalidDigest(sha256.to_owned()));
                    }
                    notebook.cells[index].outputs.push(Output::Asset {
                        mime,
                        ordinal,
                        sha256: sha256.to_owned(),
                    });
                }
                ["%%", "provenance", id_field, schema_field, source_field, prefix_field, evaluator_field, ordinal_field, output_ordinal_field, result_field, state @ ..] =>
                {
                    let mut stale = false;
                    let mut manifest = None;
                    let mut root = None;
                    let mut modules = None;
                    let mut module_paths = None;
                    for field in state {
                        if *field == "state=stale" && !stale {
                            stale = true;
                        } else if let Some(hash) = field.strip_prefix("root_sha256=") {
                            if root.replace(hash.to_owned()).is_some() {
                                return Err(malformed(zero_line + 1, line));
                            }
                        } else if let Some(hash) = field.strip_prefix("manifest_sha256=") {
                            if manifest.replace(hash.to_owned()).is_some() {
                                return Err(malformed(zero_line + 1, line));
                            }
                        } else if let Some(list) = field
                            .strip_prefix("module_sha256=")
                            .or_else(|| field.strip_prefix("module_path_sha256="))
                        {
                            let target = if field.starts_with("module_path_sha256=") {
                                &mut module_paths
                            } else {
                                &mut modules
                            };
                            if target.is_some() {
                                return Err(malformed(zero_line + 1, line));
                            }
                            let mut parsed = BTreeMap::new();
                            for item in list.split(',') {
                                let (name, hash) = item
                                    .split_once(':')
                                    .ok_or_else(|| malformed(zero_line + 1, line))?;
                                if parsed.insert(name.to_owned(), hash.to_owned()).is_some() {
                                    return Err(malformed(zero_line + 1, line));
                                }
                            }
                            *target = Some(parsed);
                        } else {
                            return Err(malformed(zero_line + 1, line));
                        }
                    }
                    let package = match (manifest, modules) {
                        (None, None) if root.is_none() && module_paths.is_none() => None,
                        (Some(manifest_sha256), Some(modules)) => Some(PackageProvenance {
                            manifest_sha256,
                            root_sha256: root,
                            module_paths_sha256: module_paths,
                            modules,
                        }),
                        _ => return Err(malformed(zero_line + 1, line)),
                    };
                    let id = id_field
                        .strip_prefix("id=")
                        .ok_or_else(|| malformed(zero_line + 1, line))?;
                    let index = indices
                        .get(id)
                        .copied()
                        .ok_or_else(|| NotebookError::UnknownCellId(id.into()))?;
                    let cell = &mut notebook.cells[index];
                    if cell.provenance.is_some() {
                        return Err(NotebookError::InvalidProvenance(format!(
                            "duplicate record for {id}"
                        )));
                    }
                    let schema_version = schema_field
                        .strip_prefix("schema=")
                        .and_then(|s| s.parse().ok())
                        .ok_or_else(|| malformed(zero_line + 1, line))?;
                    let source_sha256 = source_field
                        .strip_prefix("source_sha256=")
                        .ok_or_else(|| malformed(zero_line + 1, line))?
                        .to_owned();
                    let prefix_sha256 = prefix_field
                        .strip_prefix("prefix_sha256=")
                        .ok_or_else(|| malformed(zero_line + 1, line))?
                        .to_owned();
                    let evaluator = evaluator_field
                        .strip_prefix("evaluator=")
                        .ok_or_else(|| malformed(zero_line + 1, line))?
                        .to_owned();
                    let ordinal = ordinal_field
                        .strip_prefix("ordinal=")
                        .and_then(|s| s.parse().ok())
                        .ok_or_else(|| malformed(zero_line + 1, line))?;
                    let output_ordinal = output_ordinal_field
                        .strip_prefix("output_ordinal=")
                        .ok_or_else(|| malformed(zero_line + 1, line))?;
                    let output_ordinal = if output_ordinal == "none" {
                        None
                    } else {
                        Some(
                            output_ordinal
                                .parse()
                                .map_err(|_| malformed(zero_line + 1, line))?,
                        )
                    };
                    let result = match *result_field {
                        "result=success" => ProvenanceResult::Success,
                        "result=failed" => ProvenanceResult::Failed,
                        _ => return Err(malformed(zero_line + 1, line)),
                    };
                    cell.provenance = Some(Provenance {
                        schema_version,
                        source_sha256,
                        prefix_sha256,
                        evaluator,
                        ordinal,
                        output_ordinal,
                        result,
                        package,
                        stale,
                    });
                }
                _ => return Err(malformed(zero_line + 1, line)),
            }
        } else {
            if matches!(body, Body::None) || ended {
                return Err(malformed(zero_line + 1, line));
            }
            buffer.push_str(raw);
        }
    }
    flush(&mut notebook, &body, &mut buffer)?;
    for cell in &notebook.cells {
        crate::provenance::validate_provenance(cell)?;
    }
    crate::assets::referenced_asset_digests(&notebook)?;
    Ok(notebook)
}

fn output_fields(
    notebook: &Notebook,
    indices: &HashMap<String, usize>,
    id_field: &str,
    mime_field: &str,
    ordinal_field: &str,
    line_no: usize,
    line: &str,
) -> Result<(usize, String, u64), NotebookError> {
    let id = id_field
        .strip_prefix("id=")
        .ok_or_else(|| malformed(line_no, line))?;
    let index = indices
        .get(id)
        .copied()
        .ok_or_else(|| NotebookError::UnknownCellId(id.to_owned()))?;
    if notebook.cells[index].kind != CellKind::Code {
        return Err(NotebookError::InvalidOutput(
            "Markdown cells cannot have outputs".into(),
        ));
    }
    let mime = mime_field
        .strip_prefix("mime=")
        .ok_or_else(|| malformed(line_no, line))?;
    if !valid_mime(mime) {
        return Err(NotebookError::InvalidOutput(format!(
            "invalid MIME type: {mime}"
        )));
    }
    let ordinal = ordinal_field
        .strip_prefix("ordinal=")
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| malformed(line_no, line))?;
    Ok((index, mime.to_owned(), ordinal))
}

fn flush(notebook: &mut Notebook, body: &Body, buffer: &mut String) -> Result<(), NotebookError> {
    let decoded = unescape_body(buffer);
    match body {
        Body::Cell(index) => notebook.cells[*index].source = decoded,
        Body::TextOutput(cell, output) => {
            if let Output::Text { mime, body, .. } = &mut notebook.cells[*cell].outputs[*output] {
                crate::validate_captured_text(mime, &decoded)?;
                *body = decoded
            }
        }
        Body::None if !buffer.is_empty() => {
            return Err(NotebookError::InvalidOutput("unexpected body".into()))
        }
        Body::None => {}
    }
    buffer.clear();
    Ok(())
}

fn unescape_body(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    for line in body.split_inclusive('\n') {
        let slashes = line.bytes().take_while(|byte| *byte == b'\\').count();
        if slashes > 0 && line[slashes..].starts_with("%%") {
            out.push_str(&line[1..]);
        } else {
            out.push_str(line);
        }
    }
    out
}

fn malformed(line: usize, header: &str) -> NotebookError {
    NotebookError::MalformedHeader {
        line,
        header: header.to_owned(),
    }
}
