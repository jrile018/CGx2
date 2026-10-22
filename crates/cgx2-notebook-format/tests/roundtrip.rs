use cgx2_notebook_format::{
    make_provenance, parse_notebook, prefix_hash, source_hash, write_notebook, Cell, CellKind,
    Notebook, NotebookError, Output,
};

#[test]
fn fixture_round_trips_cells_and_text_output() {
    let original = include_str!("fixtures/basic.cgnb");
    let parsed = parse_notebook(original).unwrap();
    assert_eq!(parsed.cells.len(), 3);
    assert_eq!(parsed.cells[0].kind, CellKind::Markdown);
    assert_eq!(parsed.cells[0].source, "# Notes\n%% literal marker\n");
    assert_eq!(parsed.cells[1].source, "let x = 1\n");
    assert_eq!(
        parsed.cells[1].outputs,
        vec![Output::Text {
            mime: "text/plain".into(),
            ordinal: 1,
            body: "loaded\n".into()
        }]
    );
    assert_eq!(write_notebook(&parsed).unwrap(), original);
}

#[test]
fn provenance_roundtrips_exact_unicode_and_escaped_marker_bytes() {
    let mut notebook = Notebook {
        version: 1,
        cells: vec![Cell {
            id: "cell_1".into(),
            kind: CellKind::Code,
            source: "let café = \"雪\"\n%% literal\n\\%% escaped\n".into(),
            outputs: vec![Output::Text {
                mime: "text/plain".into(),
                ordinal: 4,
                body: "雪".into(),
            }],
            provenance: None,
        }],
    };
    notebook.cells[0].provenance = Some(make_provenance(&notebook, 0, 7, Some(4)));
    let serialized = write_notebook(&notebook).unwrap();
    let parsed = parse_notebook(&serialized).unwrap();
    assert_eq!(parsed, notebook);
    assert_eq!(
        parsed.cells[0].provenance.as_ref().unwrap().source_sha256,
        source_hash(&notebook.cells[0].source)
    );
    assert_eq!(
        prefix_hash(&parsed.cells, 0),
        prefix_hash(&notebook.cells, 0)
    );
}

#[test]
fn provenance_rejects_duplicate_bad_digest_and_output_ordinal() {
    let source = "#!cgnb 1\n%% code id=a\nx\n%% output id=a mime=text/plain ordinal=3\ny\n";
    let mut notebook = parse_notebook(source).unwrap();
    notebook.cells[0].provenance = Some(make_provenance(&notebook, 0, 5, Some(3)));
    let valid = write_notebook(&notebook).unwrap();
    let record = valid
        .lines()
        .find(|line| line.starts_with("%% provenance"))
        .unwrap();
    assert!(matches!(
        parse_notebook(&format!("{valid}{record}\n")),
        Err(NotebookError::InvalidProvenance(_))
    ));
    assert!(matches!(
        parse_notebook(&valid.replace("source_sha256=", "source_sha256=BAD")),
        Err(NotebookError::InvalidProvenance(_))
    ));
    assert!(matches!(
        parse_notebook(&valid.replace("output_ordinal=3", "output_ordinal=9")),
        Err(NotebookError::InvalidProvenance(_))
    ));
    assert!(matches!(
        parse_notebook(&valid.replace("schema=1", "schema=2")),
        Err(NotebookError::InvalidProvenance(_))
    ));
}

#[test]
fn no_final_newline_and_leading_markers_survive_multiple_cells() {
    let notebook = Notebook {
        version: 1,
        cells: vec![
            Cell {
                id: "a".into(),
                kind: CellKind::Code,
                source: "%% shown\n\\%% slash\nend".into(),
                outputs: vec![Output::Text {
                    mime: "text/plain".into(),
                    ordinal: 7,
                    body: "%% result".into(),
                }],
                provenance: None,
            },
            Cell {
                id: "b".into(),
                kind: CellKind::Markdown,
                source: "tail".into(),
                outputs: vec![],
                provenance: None,
            },
        ],
    };
    let written = write_notebook(&notebook).unwrap();
    assert!(written.contains("\\%% shown\n\\\\%% slash\nend\n%% end\n"));
    assert_eq!(parse_notebook(&written).unwrap(), notebook);
}

#[test]
fn bad_fixtures_have_typed_errors() {
    assert!(matches!(
        parse_notebook(include_str!("fixtures/duplicate.cgnb")),
        Err(NotebookError::DuplicateCellId(_))
    ));
    assert!(matches!(
        parse_notebook(include_str!("fixtures/bad-version.cgnb")),
        Err(NotebookError::UnsupportedVersion(_))
    ));
    assert!(matches!(
        parse_notebook(include_str!("fixtures/truncated.cgnb")),
        Err(NotebookError::MalformedHeader { .. })
    ));
    assert!(matches!(
        parse_notebook("#!cgnb 1\n%% output id=lost mime=text/plain ordinal=1\noops\n"),
        Err(NotebookError::UnknownCellId(_))
    ));
}

#[test]
fn unfinished_header_at_eof_is_rejected() {
    assert!(matches!(
        parse_notebook("#!cgnb 1\n%% code id=complete"),
        Err(NotebookError::MalformedHeader { .. })
    ));
}

#[test]
fn malformed_mime_is_rejected() {
    let text = "#!cgnb 1\n%% code id=a\n%% output id=a mime=image/png/extra ordinal=1\n";
    assert!(matches!(
        parse_notebook(text),
        Err(NotebookError::MalformedHeader { .. })
    ));
}

#[test]
fn writer_rejects_duplicate_ids_and_markdown_outputs() {
    let cell = Cell {
        id: "a".into(),
        kind: CellKind::Code,
        source: String::new(),
        outputs: vec![],
        provenance: None,
    };
    let duplicate = Notebook {
        version: 1,
        cells: vec![cell.clone(), cell],
    };
    assert!(matches!(
        write_notebook(&duplicate),
        Err(NotebookError::DuplicateCellId(_))
    ));
    let markdown = Notebook {
        version: 1,
        cells: vec![Cell {
            id: "md".into(),
            kind: CellKind::Markdown,
            source: String::new(),
            outputs: vec![Output::Text {
                mime: "text/plain".into(),
                ordinal: 1,
                body: "x".into(),
            }],
            provenance: None,
        }],
    };
    assert!(matches!(
        write_notebook(&markdown),
        Err(NotebookError::InvalidOutput(_))
    ));
}
