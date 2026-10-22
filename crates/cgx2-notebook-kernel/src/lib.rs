//! Checked, bounded replay of pure scalar CGx2 notebook cells.

use std::collections::BTreeMap;
use std::fmt;

use cgx2_cli::{check_source, run_source, Value};
use cgx2_notebook_format::{CellKind, Notebook};
use cgx2_notebook_session::{
    CellExecutor, CellFacts, CellResult, CellStatus, NotebookSession, SessionError,
};
use cgx2_syntax::{parse_module, Expr, ExprKind, Path, StmtKind};

pub const MAX_CODE_CELLS: usize = 128;
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
pub const MAX_REPLAY_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KernelError {
    pub code: &'static str,
    pub message: String,
    pub cell_id: Option<String>,
}

impl fmt::Display for KernelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.cell_id {
            Some(id) => write!(f, "{} in cell {}: {}", self.code, id, self.message),
            None => write!(f, "{}: {}", self.code, self.message),
        }
    }
}

impl std::error::Error for KernelError {}

fn error(code: &'static str, message: impl Into<String>) -> KernelError {
    KernelError {
        code,
        message: message.into(),
        cell_id: None,
    }
}

fn session_error(source: SessionError) -> KernelError {
    match source {
        SessionError::ExecutionFailed { id, message } => {
            let (code, message) = if let Some(reason) = message.strip_prefix("CGNK001: ") {
                ("CGNK001", reason.to_owned())
            } else if let Some(reason) = message.strip_prefix("CGNK006: ") {
                ("CGNK006", reason.to_owned())
            } else if let Some(reason) = message.strip_prefix("CGNK008: ") {
                ("CGNK008", reason.to_owned())
            } else {
                ("CGNK008", message)
            };
            KernelError {
                code,
                message,
                cell_id: Some(id),
            }
        }
        other => error("CGNK007", other.to_string()),
    }
}

/// A clean, file-order notebook session. Running an earlier cell again requires restart.
#[derive(Debug)]
pub struct NotebookKernel {
    session: NotebookSession<ReplayExecutor>,
    code_order: Vec<String>,
    next_code: usize,
    requires_restart: bool,
}

impl NotebookKernel {
    pub fn open(mut notebook: Notebook) -> Result<Self, KernelError> {
        validate_limits(&notebook)?;
        let code_order = notebook
            .cells
            .iter()
            .filter(|cell| cell.kind == CellKind::Code)
            .map(|cell| cell.id.clone())
            .collect();
        // Saved outputs are cached observations, not live results in this clean session.
        for cell in &mut notebook.cells {
            if cell.kind == CellKind::Code {
                cell.outputs.clear();
            }
        }
        let session = new_session(notebook)?;
        Ok(Self {
            session,
            code_order,
            next_code: 0,
            requires_restart: false,
        })
    }

    pub fn notebook(&self) -> &Notebook {
        self.session.notebook()
    }

    pub fn status(&self, id: &str) -> Result<CellStatus, KernelError> {
        self.session.status(id).map_err(session_error)
    }

    pub fn run_cell(&mut self, id: &str) -> Result<(), KernelError> {
        let Some(index) = self.code_order.iter().position(|candidate| candidate == id) else {
            return Err(session_error(match self.session.status(id) {
                Ok(_) => SessionError::MarkdownCell(id.into()),
                Err(source) => source,
            }));
        };
        if self.requires_restart || index < self.next_code {
            return Err(error(
                "CGNK004",
                "cell rerun or edit requires Restart and Run All",
            ));
        }
        if index != self.next_code {
            return Err(error("CGNK005", "code cells must execute in file order"));
        }
        self.session.run_cell(id).map_err(session_error)?;
        self.next_code += 1;
        Ok(())
    }

    pub fn replace_cell(&mut self, id: &str, source: String) -> Result<(), KernelError> {
        let mut proposed = self.session.notebook().clone();
        let cell = proposed
            .cells
            .iter_mut()
            .find(|cell| cell.id == id)
            .ok_or_else(|| error("CGNK007", format!("unknown notebook cell ID: {id}")))?;
        if cell.kind != CellKind::Code {
            return Err(error(
                "CGNK007",
                format!("cannot edit Markdown with replace_cell: {id}"),
            ));
        }
        cell.source = source.clone();
        validate_limits(&proposed)?;
        self.session
            .replace_cell(id, source.clone(), facts_or_opaque(&source))
            .map_err(session_error)?;
        self.requires_restart = true;
        Ok(())
    }

    /// Clears old code outputs, resets replay, and runs all code cells in file order.
    pub fn restart_and_run_all(&mut self) -> Result<(), KernelError> {
        let mut notebook = self.session.notebook().clone();
        for cell in &mut notebook.cells {
            if cell.kind == CellKind::Code {
                cell.outputs.clear();
            }
        }
        self.session = new_session(notebook)?;
        self.next_code = 0;
        self.requires_restart = false;
        for id in self.code_order.clone() {
            self.run_cell(&id)?;
        }
        Ok(())
    }
}

fn validate_limits(notebook: &Notebook) -> Result<(), KernelError> {
    let code_cells = notebook
        .cells
        .iter()
        .filter(|cell| cell.kind == CellKind::Code)
        .count();
    if code_cells > MAX_CODE_CELLS {
        return Err(error(
            "CGNK002",
            format!("notebook has {code_cells} code cells; limit is {MAX_CODE_CELLS}"),
        ));
    }
    let bytes = notebook
        .cells
        .iter()
        .filter(|cell| cell.kind == CellKind::Code)
        .try_fold(0usize, |sum, cell| sum.checked_add(cell.source.len()))
        .unwrap_or(usize::MAX);
    if bytes > MAX_SOURCE_BYTES {
        return Err(error(
            "CGNK003",
            format!("code source has {bytes} bytes; limit is {MAX_SOURCE_BYTES}"),
        ));
    }
    Ok(())
}

fn new_session(notebook: Notebook) -> Result<NotebookSession<ReplayExecutor>, KernelError> {
    let facts = notebook
        .cells
        .iter()
        .filter(|cell| cell.kind == CellKind::Code)
        .map(|cell| (cell.id.clone(), facts_or_opaque(&cell.source)))
        .collect::<BTreeMap<_, _>>();
    NotebookSession::new(notebook, facts, ReplayExecutor::default()).map_err(session_error)
}

fn facts_or_opaque(source: &str) -> CellFacts {
    facts_for_source(source).unwrap_or(CellFacts {
        opaque_effects: true,
        ..CellFacts::default()
    })
}

/// Extracts simple definitions and reads from the syntax tree; unsupported forms fail closed.
pub fn facts_for_source(source: &str) -> Result<CellFacts, KernelError> {
    let module = parse_module(source).map_err(|diagnostics| {
        let first = &diagnostics[0];
        error("CGNK008", format!("{}: {}", first.code, first.message))
    })?;
    if module.name.is_some() || !module.functions.is_empty() {
        return Err(error(
            "CGNK001",
            "module declarations and functions are unsupported in notebook replay",
        ));
    }
    let mut facts = CellFacts::default();
    for statement in &module.statements {
        match &statement.kind {
            StmtKind::Let {
                mutable: false,
                name,
                value,
                ..
            } => {
                facts.defines.insert(name.clone());
                collect_reads(value, &mut facts)?;
            }
            StmtKind::Expr(value) => collect_reads(value, &mut facts)?,
            _ => {
                return Err(error(
                    "CGNK001",
                    "only immutable let bindings and pure scalar expressions are supported",
                ))
            }
        }
    }
    Ok(facts)
}

fn collect_reads(expression: &Expr, facts: &mut CellFacts) -> Result<(), KernelError> {
    match &expression.kind {
        ExprKind::Name(path) => {
            facts.reads.insert(simple_name(path)?.to_owned());
        }
        ExprKind::Binary { left, right, .. } => {
            collect_reads(left, facts)?;
            collect_reads(right, facts)?;
        }
        ExprKind::Integer(_) | ExprKind::StringLiteral(_) | ExprKind::Boolean(_) => {}
        ExprKind::Call { .. } => {
            return Err(error(
                "CGNK001",
                "function calls are unsupported in notebook replay",
            ))
        }
    }
    Ok(())
}

fn simple_name(path: &Path) -> Result<&str, KernelError> {
    match path.segments.as_slice() {
        [name] => Ok(name),
        _ => Err(error(
            "CGNK001",
            "qualified names are unsupported in notebook replay",
        )),
    }
}

#[derive(Debug, Default)]
struct ReplayExecutor {
    accepted: Vec<String>,
    replay_bytes: usize,
}

impl CellExecutor for ReplayExecutor {
    fn execute(&mut self, source: &str) -> Result<CellResult, String> {
        facts_for_source(source).map_err(|error| error.to_string())?;
        let mut script = self.accepted.join("\n");
        if !script.is_empty() {
            script.push('\n');
        }
        script.push_str(source);
        self.replay_bytes = self
            .replay_bytes
            .checked_add(script.len())
            .filter(|total| *total <= MAX_REPLAY_BYTES)
            .ok_or_else(|| {
                format!("CGNK006: replay budget of {MAX_REPLAY_BYTES} bytes exceeded")
            })?;
        check_source(&script).map_err(|diagnostics| {
            let first = &diagnostics[0];
            format!("{}: {}", first.code, first.message)
        })?;
        let result = run_source(&script, &[])
            .map_err(|error| format!("{}: {}", error.code, error.message))?;
        self.accepted.push(source.to_owned());
        let text = match result.value {
            Value::Unit => None,
            value => Some(value.to_string()),
        };
        Ok(CellResult { text })
    }

    fn reset(&mut self) -> Result<(), String> {
        self.accepted.clear();
        self.replay_bytes = 0;
        Ok(())
    }
}
