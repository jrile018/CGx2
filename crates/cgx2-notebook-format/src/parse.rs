use std::collections::HashMap;

use crate::{
    check_document_size, valid_id, valid_mime, Cell, CellKind, Notebook, NotebookError, Output,
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
        return Err(malformed(line_no, line));
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
            if let Output::Text { body, .. } = &mut notebook.cells[*cell].outputs[*output] {
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
