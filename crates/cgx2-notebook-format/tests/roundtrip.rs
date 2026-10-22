use cgx2_notebook_format::{
    parse_notebook, write_notebook, Cell, CellKind, Notebook, NotebookError, Output,
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
            },
            Cell {
                id: "b".into(),
                kind: CellKind::Markdown,
                source: "tail".into(),
                outputs: vec![],
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
        }],
    };
    assert!(matches!(
        write_notebook(&markdown),
        Err(NotebookError::InvalidOutput(_))
    ));
}
