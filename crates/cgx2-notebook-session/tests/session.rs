use std::collections::{BTreeMap, BTreeSet};

use cgx2_notebook_format::{Cell, CellKind, Notebook, Output};
use cgx2_notebook_session::{
    CellExecutor, CellFacts, CellResult, CellStatus, NotebookSession, SessionError,
};

#[derive(Debug, Default)]
struct FakeExecutor {
    calls: Vec<String>,
    counter: u64,
    resets: u64,
    fail_reset: bool,
}

impl CellExecutor for FakeExecutor {
    fn execute(&mut self, source: &str) -> Result<CellResult, String> {
        self.calls.push(source.to_owned());
        if source == "fail" {
            return Err("execution failed".into());
        }
        self.counter += 1;
        if source == "silent" {
            return Ok(CellResult { text: None });
        }
        Ok(CellResult {
            text: Some(format!("{}:{}", source, self.counter)),
        })
    }

    fn reset(&mut self) -> Result<(), String> {
        if self.fail_reset {
            return Err("reset failed".into());
        }
        self.resets += 1;
        self.counter = 0;
        Ok(())
    }
}

fn code(id: &str, source: &str) -> Cell {
    Cell {
        id: id.into(),
        kind: CellKind::Code,
        source: source.into(),
        outputs: vec![],
        provenance: None,
    }
}

fn markdown(id: &str) -> Cell {
    Cell {
        id: id.into(),
        kind: CellKind::Markdown,
        source: "notes".into(),
        outputs: vec![],
        provenance: None,
    }
}

fn notebook(cells: Vec<Cell>) -> Notebook {
    Notebook { version: 1, cells }
}

fn facts(defines: &[&str], reads: &[&str], opaque_effects: bool) -> CellFacts {
    CellFacts {
        defines: defines
            .iter()
            .map(|s| (*s).to_owned())
            .collect::<BTreeSet<_>>(),
        reads: reads
            .iter()
            .map(|s| (*s).to_owned())
            .collect::<BTreeSet<_>>(),
        opaque_effects,
    }
}

fn map(entries: &[(&str, CellFacts)]) -> BTreeMap<String, CellFacts> {
    entries
        .iter()
        .map(|(id, f)| ((*id).into(), f.clone()))
        .collect()
}

#[test]
fn constructs_cached_code_and_never_run_markdown() {
    let mut saved = code("a", "x");
    saved.outputs.push(Output::Text {
        mime: "text/plain".into(),
        ordinal: 3,
        body: "saved".into(),
    });
    let mut session = NotebookSession::new(
        notebook(vec![saved, markdown("notes")]),
        map(&[("a", facts(&["x"], &[], false))]),
        FakeExecutor::default(),
    )
    .unwrap();
    assert_eq!(session.status("a").unwrap(), CellStatus::Cached);
    assert_eq!(session.status("notes").unwrap(), CellStatus::NeverRun);
    assert_eq!(session.notebook().cells[0].outputs.len(), 1);
    assert!(matches!(
        session.status("missing"),
        Err(SessionError::UnknownCellId(_))
    ));
    assert!(matches!(
        session.run_cell("notes"),
        Err(SessionError::MarkdownCell(_))
    ));
}

#[test]
fn rejects_inconsistent_ids_and_facts() {
    assert!(matches!(
        NotebookSession::new(
            notebook(vec![code("bad id", "")]),
            map(&[("bad id", facts(&[], &[], false))]),
            FakeExecutor::default(),
        ),
        Err(SessionError::InvalidCellId(_))
    ));
    assert!(matches!(
        NotebookSession::new(
            notebook(vec![code("a", ""), code("a", "")]),
            map(&[("a", facts(&[], &[], false))]),
            FakeExecutor::default(),
        ),
        Err(SessionError::DuplicateCellId(_))
    ));
    assert!(matches!(
        NotebookSession::new(
            notebook(vec![code("a", "")]),
            map(&[("a", facts(&[], &[], false)), ("b", facts(&[], &[], false))]),
            FakeExecutor::default(),
        ),
        Err(SessionError::UnknownFactId(_))
    ));
    assert!(matches!(
        NotebookSession::new(
            notebook(vec![code("a", "")]),
            BTreeMap::new(),
            FakeExecutor::default(),
        ),
        Err(SessionError::MissingFacts(_))
    ));
    assert!(matches!(
        NotebookSession::new(
            notebook(vec![markdown("notes")]),
            map(&[("notes", facts(&[], &[], false))]),
            FakeExecutor::default(),
        ),
        Err(SessionError::FactsForMarkdown(_))
    ));
}

#[test]
fn rerunning_definition_stales_later_reader_without_replaying_it() {
    let mut session = NotebookSession::new(
        notebook(vec![code("a", "define"), code("b", "read")]),
        map(&[
            ("a", facts(&["value"], &[], false)),
            ("b", facts(&[], &["value"], false)),
        ]),
        FakeExecutor::default(),
    )
    .unwrap();
    session.run_cell("a").unwrap();
    session.run_cell("b").unwrap();
    session.run_cell("a").unwrap();
    assert_eq!(session.status("a").unwrap(), CellStatus::Current);
    assert_eq!(session.status("b").unwrap(), CellStatus::Stale);
    assert_eq!(text_output(&session.notebook().cells[0]), (3, "define:3"));
    assert_eq!(text_output(&session.notebook().cells[1]), (2, "read:2"));
}

#[test]
fn replacement_uses_old_and_new_definitions_for_invalidation() {
    let mut session = NotebookSession::new(
        notebook(vec![
            code("root", "x"),
            code("old", "read x"),
            code("new", "read y"),
        ]),
        map(&[
            ("root", facts(&["x"], &[], false)),
            ("old", facts(&[], &["x"], false)),
            ("new", facts(&[], &["y"], false)),
        ]),
        FakeExecutor::default(),
    )
    .unwrap();
    for id in ["root", "old", "new"] {
        session.run_cell(id).unwrap();
    }
    session
        .replace_cell("root", "y".into(), facts(&["y"], &[], false))
        .unwrap();
    assert_eq!(session.status("old").unwrap(), CellStatus::Stale);
    assert_eq!(session.status("new").unwrap(), CellStatus::Stale);
}

fn text_output(cell: &Cell) -> (u64, &str) {
    match cell.outputs.as_slice() {
        [Output::Text {
            mime,
            ordinal,
            body,
        }] => {
            assert_eq!(mime, "text/plain");
            (*ordinal, body)
        }
        other => panic!("expected one text output, got {other:?}"),
    }
}

#[test]
fn runs_one_cell_persistently_and_replaces_only_its_output() {
    let mut session = NotebookSession::new(
        notebook(vec![code("first", "side effect"), code("later", "value")]),
        map(&[
            ("first", facts(&[], &[], true)),
            ("later", facts(&[], &[], false)),
        ]),
        FakeExecutor::default(),
    )
    .unwrap();
    session.run_cell("later").unwrap();
    assert_eq!(text_output(&session.notebook().cells[1]), (1, "value:1"));
    assert!(session.notebook().cells[0].outputs.is_empty());
    session.run_cell("later").unwrap();
    assert_eq!(text_output(&session.notebook().cells[1]), (2, "value:2"));
    assert!(session.notebook().cells[0].outputs.is_empty());
    assert_eq!(session.status("later").unwrap(), CellStatus::Current);
}

#[test]
fn changed_definition_stales_transitive_dependents_and_preserves_cache() {
    let mut session = NotebookSession::new(
        notebook(vec![
            code("root", "define a"),
            code("unrelated", "define z"),
            code("middle", "b from a"),
            code("leaf", "read b"),
        ]),
        map(&[
            ("root", facts(&["a"], &[], false)),
            ("unrelated", facts(&["z"], &[], false)),
            ("middle", facts(&["b"], &["a"], false)),
            ("leaf", facts(&[], &["b"], false)),
        ]),
        FakeExecutor::default(),
    )
    .unwrap();
    for id in ["root", "unrelated", "middle", "leaf"] {
        session.run_cell(id).unwrap();
    }
    session
        .replace_cell("root", "new a".into(), facts(&["a"], &[], false))
        .unwrap();
    assert_eq!(session.status("root").unwrap(), CellStatus::Stale);
    assert_eq!(session.status("unrelated").unwrap(), CellStatus::Stale);
    assert_eq!(session.status("middle").unwrap(), CellStatus::Stale);
    assert_eq!(session.status("leaf").unwrap(), CellStatus::Stale);
    assert_eq!(text_output(&session.notebook().cells[3]), (4, "read b:4"));
    assert_eq!(session.notebook().cells[0].source, "new a");
}

#[test]
fn opaque_edit_stales_all_later_code_without_a_symbol_edge() {
    let mut session = NotebookSession::new(
        notebook(vec![
            code("before", "x"),
            code("effect", "y"),
            markdown("note"),
            code("after", "z"),
        ]),
        map(&[
            ("before", facts(&[], &[], false)),
            ("effect", facts(&[], &[], true)),
            ("after", facts(&[], &[], false)),
        ]),
        FakeExecutor::default(),
    )
    .unwrap();
    for id in ["before", "effect", "after"] {
        session.run_cell(id).unwrap();
    }
    session
        .replace_cell("effect", "changed".into(), facts(&[], &[], true))
        .unwrap();
    assert_eq!(session.status("before").unwrap(), CellStatus::Current);
    assert_eq!(session.status("effect").unwrap(), CellStatus::Stale);
    assert_eq!(session.status("note").unwrap(), CellStatus::NeverRun);
    assert_eq!(session.status("after").unwrap(), CellStatus::Stale);
    assert_eq!(text_output(&session.notebook().cells[3]), (3, "z:3"));
}

#[test]
fn failure_keeps_cached_output_visible_but_not_current() {
    let mut session = NotebookSession::new(
        notebook(vec![code("a", "good"), code("b", "depends")]),
        map(&[
            ("a", facts(&["a"], &[], false)),
            ("b", facts(&[], &["a"], false)),
        ]),
        FakeExecutor::default(),
    )
    .unwrap();
    session.run_cell("a").unwrap();
    session.run_cell("b").unwrap();
    session
        .replace_cell("a", "fail".into(), facts(&["a"], &[], false))
        .unwrap();
    assert!(matches!(
        session.run_cell("a"),
        Err(SessionError::ExecutionFailed { .. })
    ));
    assert_eq!(session.status("a").unwrap(), CellStatus::Failed);
    assert_eq!(session.status("b").unwrap(), CellStatus::Stale);
    assert_eq!(text_output(&session.notebook().cells[0]), (1, "good:1"));
}

#[test]
fn restart_runs_code_in_order_with_fresh_executor_and_new_ordinals() {
    let mut session = NotebookSession::new(
        notebook(vec![
            code("a", "first"),
            markdown("note"),
            code("b", "second"),
        ]),
        map(&[("a", facts(&[], &[], false)), ("b", facts(&[], &[], false))]),
        FakeExecutor::default(),
    )
    .unwrap();
    session.run_cell("b").unwrap();
    assert_eq!(text_output(&session.notebook().cells[2]), (1, "second:1"));
    session.restart_and_run_all().unwrap();
    assert_eq!(text_output(&session.notebook().cells[0]), (2, "first:1"));
    assert_eq!(text_output(&session.notebook().cells[2]), (3, "second:2"));
    assert_eq!(session.status("a").unwrap(), CellStatus::Current);
    assert_eq!(session.status("note").unwrap(), CellStatus::NeverRun);
    assert_eq!(session.status("b").unwrap(), CellStatus::Current);
    assert!(session.notebook().cells[1].outputs.is_empty());
}

#[test]
fn reset_failure_reports_error_and_does_not_replay_cells() {
    let mut session = NotebookSession::new(
        notebook(vec![code("a", "first")]),
        map(&[("a", facts(&[], &[], false))]),
        FakeExecutor {
            fail_reset: true,
            ..FakeExecutor::default()
        },
    )
    .unwrap();
    session.run_cell("a").unwrap();
    assert!(matches!(
        session.restart_and_run_all(),
        Err(SessionError::ResetFailed(_))
    ));
    assert_eq!(session.status("a").unwrap(), CellStatus::Stale);
    assert_eq!(text_output(&session.notebook().cells[0]), (1, "first:1"));
}

#[test]
fn restart_stops_on_failure_and_leaves_later_cached_outputs_stale() {
    let mut session = NotebookSession::new(
        notebook(vec![code("a", "first"), code("b", "later")]),
        map(&[("a", facts(&[], &[], false)), ("b", facts(&[], &[], false))]),
        FakeExecutor::default(),
    )
    .unwrap();
    session.run_cell("b").unwrap();
    session
        .replace_cell("a", "fail".into(), facts(&[], &[], false))
        .unwrap();
    assert!(matches!(
        session.restart_and_run_all(),
        Err(SessionError::ExecutionFailed { .. })
    ));
    assert_eq!(session.status("a").unwrap(), CellStatus::Failed);
    assert_eq!(session.status("b").unwrap(), CellStatus::Stale);
    assert_eq!(text_output(&session.notebook().cells[1]), (1, "later:1"));
}

#[test]
fn silent_success_clears_old_output_and_ordinal_overflow_is_checked() {
    let mut saved = code("a", "silent");
    saved.outputs.push(Output::Text {
        mime: "text/plain".into(),
        ordinal: 7,
        body: "old".into(),
    });
    let mut session = NotebookSession::new(
        notebook(vec![saved]),
        map(&[("a", facts(&[], &[], false))]),
        FakeExecutor::default(),
    )
    .unwrap();
    session.run_cell("a").unwrap();
    assert!(session.notebook().cells[0].outputs.is_empty());
    assert_eq!(session.status("a").unwrap(), CellStatus::Current);

    let mut maxed = code("a", "value");
    maxed.outputs.push(Output::Text {
        mime: "text/plain".into(),
        ordinal: u64::MAX,
        body: "old".into(),
    });
    let mut maxed = NotebookSession::new(
        notebook(vec![maxed]),
        map(&[("a", facts(&[], &[], false))]),
        FakeExecutor::default(),
    )
    .unwrap();
    assert!(matches!(
        maxed.run_cell("a"),
        Err(SessionError::OrdinalOverflow)
    ));
    assert_eq!(maxed.status("a").unwrap(), CellStatus::Failed);
    assert_eq!(text_output(&maxed.notebook().cells[0]), (u64::MAX, "old"));
}
