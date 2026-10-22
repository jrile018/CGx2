use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use cgx2_notebook_format::{
    make_provenance, prefix_hash, source_hash, CellKind, Notebook, Output, ProvenanceResult,
};

use crate::deps::{affected_after, CellFacts};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellStatus {
    NeverRun,
    Cached,
    Current,
    Stale,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellResult {
    pub text: Option<String>,
}

pub trait CellExecutor {
    fn execute(&mut self, source: &str) -> Result<CellResult, String>;
    fn execute_cell(&mut self, _id: &str, source: &str) -> Result<CellResult, String> {
        self.execute(source)
    }
    /// Drop definitions belonging to these stable cell IDs.
    fn invalidate_cells(&mut self, _ids: &[String]) {}
    fn reset(&mut self) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    DuplicateCellId(String),
    InvalidCellId(String),
    UnknownCellId(String),
    UnknownFactId(String),
    MissingFacts(String),
    FactsForMarkdown(String),
    MarkdownCell(String),
    ExecutionFailed { id: String, message: String },
    ResetFailed(String),
    PrerequisiteNotCurrent { id: String, prerequisite: String },
    OrdinalOverflow,
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateCellId(id) => write!(f, "duplicate notebook cell ID: {id}"),
            Self::InvalidCellId(id) => write!(f, "invalid notebook cell ID: {id}"),
            Self::UnknownCellId(id) => write!(f, "unknown notebook cell ID: {id}"),
            Self::UnknownFactId(id) => write!(f, "facts reference unknown cell ID: {id}"),
            Self::MissingFacts(id) => write!(f, "missing code cell facts: {id}"),
            Self::FactsForMarkdown(id) => write!(f, "facts supplied for Markdown cell: {id}"),
            Self::MarkdownCell(id) => write!(f, "cannot execute Markdown cell: {id}"),
            Self::ExecutionFailed { id, message } => write!(f, "cell {id} failed: {message}"),
            Self::ResetFailed(message) => write!(f, "executor reset failed: {message}"),
            Self::PrerequisiteNotCurrent { id, prerequisite } => write!(
                f,
                "cell {id} needs cell {prerequisite} rerun, or Restart and Run All"
            ),
            Self::OrdinalOverflow => write!(f, "notebook output ordinal overflow"),
        }
    }
}

impl std::error::Error for SessionError {}

#[derive(Debug)]
pub struct NotebookSession<E: CellExecutor> {
    notebook: Notebook,
    facts: BTreeMap<String, CellFacts>,
    statuses: BTreeMap<String, CellStatus>,
    executor: E,
    last_ordinal: u64,
    last_execution_ordinal: u64,
}

impl<E: CellExecutor> NotebookSession<E> {
    pub fn new(
        notebook: Notebook,
        facts: BTreeMap<String, CellFacts>,
        executor: E,
    ) -> Result<Self, SessionError> {
        let mut seen = BTreeSet::new();
        let mut statuses = BTreeMap::new();
        let mut last_ordinal = 0;
        let mut last_execution_ordinal = 0;
        for cell in &notebook.cells {
            if !valid_cell_id(&cell.id) {
                return Err(SessionError::InvalidCellId(cell.id.clone()));
            }
            if !seen.insert(cell.id.clone()) {
                return Err(SessionError::DuplicateCellId(cell.id.clone()));
            }
            statuses.insert(cell.id.clone(), CellStatus::NeverRun);
            if cell.kind == CellKind::Code && !facts.contains_key(&cell.id) {
                return Err(SessionError::MissingFacts(cell.id.clone()));
            }
            for output in &cell.outputs {
                let ordinal = match output {
                    cgx2_notebook_format::Output::Text { ordinal, .. }
                    | cgx2_notebook_format::Output::Asset { ordinal, .. } => *ordinal,
                };
                last_ordinal = last_ordinal.max(ordinal);
            }
        }
        for id in facts.keys() {
            let Some(cell) = notebook.cells.iter().find(|cell| cell.id == *id) else {
                return Err(SessionError::UnknownFactId(id.clone()));
            };
            if cell.kind == CellKind::Markdown {
                return Err(SessionError::FactsForMarkdown(id.clone()));
            }
        }
        for (index, cell) in notebook.cells.iter().enumerate() {
            if cell.kind != CellKind::Code {
                continue;
            }
            let status = if let Some(provenance) = &cell.provenance {
                if provenance.source_sha256 != source_hash(&cell.source)
                    || provenance.prefix_sha256 != prefix_hash(&notebook.cells, index)
                {
                    CellStatus::Stale
                } else if provenance.result == ProvenanceResult::Failed {
                    CellStatus::Failed
                } else {
                    CellStatus::Cached
                }
            } else if !cell.outputs.is_empty() {
                CellStatus::Cached
            } else {
                CellStatus::NeverRun
            };
            statuses.insert(cell.id.clone(), status);
            if let Some(provenance) = &cell.provenance {
                last_execution_ordinal = last_execution_ordinal.max(provenance.ordinal);
            }
        }
        Ok(Self {
            notebook,
            facts,
            statuses,
            executor,
            last_ordinal,
            last_execution_ordinal: last_execution_ordinal.max(last_ordinal),
        })
    }

    pub fn status(&self, id: &str) -> Result<CellStatus, SessionError> {
        self.statuses
            .get(id)
            .copied()
            .ok_or_else(|| SessionError::UnknownCellId(id.into()))
    }

    pub fn notebook(&self) -> &Notebook {
        &self.notebook
    }

    pub fn run_cell(&mut self, id: &str) -> Result<(), SessionError> {
        let index = self.cell_index(id)?;
        if self.notebook.cells[index].kind == CellKind::Markdown {
            return Err(SessionError::MarkdownCell(id.into()));
        }
        self.check_prerequisites(index, id)?;
        if self.last_execution_ordinal == u64::MAX || self.last_ordinal == u64::MAX {
            self.fail_cell(index);
            return Err(SessionError::OrdinalOverflow);
        }
        let source = self.notebook.cells[index].source.clone();
        let result = match self.executor.execute_cell(id, &source) {
            Ok(result) => result,
            Err(message) => {
                self.statuses.insert(id.into(), CellStatus::Failed);
                self.record_failed_attempt(index);
                self.invalidate_after(
                    index,
                    &self.facts[id].defines.clone(),
                    self.facts[id].opaque_effects,
                );
                return Err(SessionError::ExecutionFailed {
                    id: id.into(),
                    message,
                });
            }
        };
        let execution_ordinal = match self.last_execution_ordinal.checked_add(1) {
            Some(ordinal) => ordinal,
            None => {
                self.fail_cell(index);
                return Err(SessionError::OrdinalOverflow);
            }
        };
        let mut output_ordinal = None;
        let outputs = if let Some(body) = result.text {
            let ordinal = match self.last_ordinal.checked_add(1) {
                Some(ordinal) => ordinal,
                None => {
                    self.statuses.insert(id.into(), CellStatus::Failed);
                    self.invalidate_after(
                        index,
                        &self.facts[id].defines.clone(),
                        self.facts[id].opaque_effects,
                    );
                    return Err(SessionError::OrdinalOverflow);
                }
            };
            self.last_ordinal = ordinal;
            output_ordinal = Some(ordinal);
            vec![Output::Text {
                mime: "text/plain".into(),
                ordinal,
                body,
            }]
        } else {
            vec![]
        };
        self.last_execution_ordinal = execution_ordinal;
        self.notebook.cells[index].outputs = outputs;
        self.notebook.cells[index].provenance = Some(make_provenance(
            &self.notebook,
            index,
            execution_ordinal,
            output_ordinal,
        ));
        self.statuses.insert(id.into(), CellStatus::Current);
        self.invalidate_after(
            index,
            &self.facts[id].defines.clone(),
            self.facts[id].opaque_effects,
        );
        Ok(())
    }

    /// Recompute the selected cell without executing earlier cells.
    pub fn rerun_cell(&mut self, id: &str) -> Result<(), SessionError> {
        let index = self.cell_index(id)?;
        if self.notebook.cells[index].kind == CellKind::Markdown {
            return Err(SessionError::MarkdownCell(id.into()));
        }
        self.check_prerequisites(index, id)?;
        self.invalidate_suffix(index);
        self.run_cell(id)
    }

    pub fn replace_cell(
        &mut self,
        id: &str,
        source: String,
        facts: CellFacts,
    ) -> Result<(), SessionError> {
        let index = self.cell_index(id)?;
        if self.notebook.cells[index].kind == CellKind::Markdown {
            return Err(SessionError::FactsForMarkdown(id.into()));
        }
        self.facts.insert(id.into(), facts);
        self.notebook.cells[index].source = source;
        self.invalidate_suffix(index);
        self.mark_stale(index);
        Ok(())
    }

    pub fn restart_and_run_all(&mut self) -> Result<(), SessionError> {
        if let Err(message) = self.executor.reset() {
            self.stale_all_code();
            return Err(SessionError::ResetFailed(message));
        }
        self.stale_all_code();
        let code_ids: Vec<String> = self
            .notebook
            .cells
            .iter()
            .filter(|cell| cell.kind == CellKind::Code)
            .map(|cell| cell.id.clone())
            .collect();
        for id in code_ids {
            self.run_cell(&id)?;
        }
        Ok(())
    }

    fn cell_index(&self, id: &str) -> Result<usize, SessionError> {
        self.notebook
            .cells
            .iter()
            .position(|cell| cell.id == id)
            .ok_or_else(|| SessionError::UnknownCellId(id.into()))
    }

    fn check_prerequisites(&self, index: usize, id: &str) -> Result<(), SessionError> {
        for read in &self.facts[id].reads {
            if self.facts[id].defines.contains(read) {
                continue;
            }
            if let Some(prerequisite) = self.notebook.cells[..index].iter().rev().find(|cell| {
                cell.kind == CellKind::Code && self.facts[&cell.id].defines.contains(read)
            }) {
                if self.statuses[&prerequisite.id] != CellStatus::Current {
                    return Err(SessionError::PrerequisiteNotCurrent {
                        id: id.into(),
                        prerequisite: prerequisite.id.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    fn mark_stale(&mut self, index: usize) {
        let cell = &self.notebook.cells[index];
        if !cell.outputs.is_empty() || self.statuses[&cell.id] != CellStatus::NeverRun {
            self.statuses.insert(cell.id.clone(), CellStatus::Stale);
        }
    }

    fn fail_cell(&mut self, index: usize) {
        let id = self.notebook.cells[index].id.clone();
        self.statuses.insert(id.clone(), CellStatus::Failed);
        self.invalidate_after(
            index,
            &self.facts[&id].defines.clone(),
            self.facts[&id].opaque_effects,
        );
    }

    fn record_failed_attempt(&mut self, index: usize) {
        let Some(ordinal) = self.last_execution_ordinal.checked_add(1) else {
            return;
        };
        let outputs = &self.notebook.cells[index].outputs;
        let output_ordinal = outputs.first().map(|output| match output {
            Output::Text { ordinal, .. } | Output::Asset { ordinal, .. } => *ordinal,
        });
        if outputs.iter().any(|output| match output {
            Output::Text { ordinal, .. } | Output::Asset { ordinal, .. } => {
                Some(*ordinal) != output_ordinal
            }
        }) {
            return;
        }
        let mut provenance = make_provenance(&self.notebook, index, ordinal, output_ordinal);
        provenance.result = ProvenanceResult::Failed;
        self.notebook.cells[index].provenance = Some(provenance);
        self.last_execution_ordinal = ordinal;
    }

    fn invalidate_after(&mut self, index: usize, defines: &BTreeSet<String>, opaque: bool) {
        for affected in affected_after(&self.notebook.cells, &self.facts, index, defines, opaque) {
            self.mark_stale(affected);
        }
    }

    fn stale_all_code(&mut self) {
        for index in 0..self.notebook.cells.len() {
            if self.notebook.cells[index].kind == CellKind::Code {
                self.mark_stale(index);
            }
        }
    }

    fn invalidate_suffix(&mut self, index: usize) {
        let ids: Vec<_> = self.notebook.cells[index..]
            .iter()
            .filter(|cell| cell.kind == CellKind::Code)
            .map(|cell| cell.id.clone())
            .collect();
        self.executor.invalidate_cells(&ids);
        for later in index + 1..self.notebook.cells.len() {
            if self.notebook.cells[later].kind == CellKind::Code {
                self.mark_stale(later);
            }
        }
    }
}

fn valid_cell_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}
