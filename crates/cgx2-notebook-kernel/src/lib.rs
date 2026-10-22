//! Checked, bounded retained-state execution of pure scalar CGx2 cells.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use cgx2_cli::{check_source, run_source, Value};
use cgx2_notebook_format::{CellKind, Notebook};
use cgx2_notebook_session::{
    CellExecutor, CellFacts, CellResult, CellStatus, NotebookSession, SessionError,
};
use cgx2_syntax::{parse_module, Expr, ExprKind, Path, StmtKind};

pub const MAX_CODE_CELLS: usize = 128;
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
pub const MAX_EVALUATION_BYTES: usize = 1024 * 1024;
/// Compatibility alias for the cumulative evaluation budget.
pub const MAX_REPLAY_BYTES: usize = MAX_EVALUATION_BYTES;

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
        SessionError::PrerequisiteNotCurrent { .. } => error("CGNK004", source.to_string()),
        SessionError::ExecutionFailed { id, message } => {
            let (code, message) = if let Some(reason) = message.strip_prefix("CGNK001: ") {
                ("CGNK001", reason.to_owned())
            } else if let Some(reason) = message.strip_prefix("CGNK004: ") {
                ("CGNK004", reason.to_owned())
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

/// A bounded, file-order notebook session with selected pure-scalar rerun.
#[derive(Debug)]
pub struct NotebookKernel {
    session: NotebookSession<RetainedExecutor>,
    code_order: Vec<String>,
    next_code: usize,
    requires_restart: bool,
}

impl NotebookKernel {
    pub fn open(notebook: Notebook) -> Result<Self, KernelError> {
        validate_limits(&notebook)?;
        let code_order = notebook
            .cells
            .iter()
            .filter(|cell| cell.kind == CellKind::Code)
            .map(|cell| cell.id.clone())
            .collect();
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

    pub fn next_cell_id(&self) -> Option<&str> {
        self.code_order.get(self.next_code).map(String::as_str)
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
                "use Run this cell or Restart and Run All after a rerun or edit",
            ));
        }
        if index != self.next_code {
            return Err(error("CGNK005", "code cells must execute in file order"));
        }
        self.session.run_cell(id).map_err(session_error)?;
        self.next_code += 1;
        Ok(())
    }

    /// Runs only `id` from retained preceding scalar bindings. Later bindings are invalidated.
    pub fn rerun_cell(&mut self, id: &str) -> Result<(), KernelError> {
        let Some(index) = self.code_order.iter().position(|candidate| candidate == id) else {
            return Err(session_error(match self.session.status(id) {
                Ok(_) => SessionError::MarkdownCell(id.into()),
                Err(source) => source,
            }));
        };
        if let Err(error) = self.session.rerun_cell(id) {
            self.requires_restart = true;
            return Err(session_error(error));
        }
        self.next_code = index + 1;
        self.requires_restart = false;
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

    /// Clears old code outputs and retained state, then runs all code cells in file order.
    pub fn restart_and_run_all(&mut self) -> Result<(), KernelError> {
        let mut notebook = self.session.notebook().clone();
        for cell in &mut notebook.cells {
            if cell.kind == CellKind::Code {
                cell.outputs.clear();
                cell.provenance = None;
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

fn new_session(notebook: Notebook) -> Result<NotebookSession<RetainedExecutor>, KernelError> {
    let facts = notebook
        .cells
        .iter()
        .filter(|cell| cell.kind == CellKind::Code)
        .map(|cell| (cell.id.clone(), facts_or_opaque(&cell.source)))
        .collect::<BTreeMap<_, _>>();
    NotebookSession::new(notebook, facts, RetainedExecutor::default()).map_err(session_error)
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
            "module declarations and functions are unsupported in notebook scalar execution",
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
                "function calls are unsupported in notebook scalar execution",
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
            "qualified names are unsupported in notebook scalar execution",
        )),
    }
}

#[derive(Debug, Clone)]
struct RetainedBinding {
    value: Value,
    cell_id: String,
}

#[derive(Debug, Default)]
struct RetainedExecutor {
    bindings: BTreeMap<String, RetainedBinding>,
    evaluation_bytes: usize,
}

impl RetainedExecutor {
    fn prefix(&self, pending: &BTreeMap<String, Value>) -> Result<String, String> {
        let mut script = String::new();
        for (name, binding) in &self.bindings {
            append_binding(&mut script, name, &binding.value)?;
        }
        for (name, value) in pending {
            append_binding(&mut script, name, value)?;
        }
        if script.len() > MAX_REPLAY_BYTES {
            return Err(limit_error());
        }
        Ok(script)
    }

    fn charge(&mut self, bytes: usize) -> Result<(), String> {
        self.evaluation_bytes = self
            .evaluation_bytes
            .checked_add(bytes)
            .filter(|total| *total <= MAX_REPLAY_BYTES)
            .ok_or_else(limit_error)?;
        Ok(())
    }

    fn execute_named(&mut self, id: &str, source: &str) -> Result<CellResult, String> {
        facts_for_source(source).map_err(|error| error.to_string())?;
        let module = parse_module(source).map_err(|diagnostics| {
            let first = &diagnostics[0];
            format!("{}: {}", first.code, first.message)
        })?;
        let prefix = self.prefix(&BTreeMap::new())?;
        let checked = format!("{prefix}{source}");
        if checked.len() > MAX_REPLAY_BYTES {
            return Err(limit_error());
        }
        self.charge(checked.len())?;
        check_source(&checked).map_err(|diagnostics| {
            let first = &diagnostics[0];
            format!("{}: {}", first.code, first.message)
        })?;

        let mut pending = BTreeMap::new();
        let mut tail = Value::Unit;
        for statement in &module.statements {
            let statement_source = &source[statement.span.start..statement.span.end];
            let prefix = self.prefix(&pending)?;
            let (script, defined) = match &statement.kind {
                StmtKind::Let { name, .. } => {
                    (format!("{prefix}{statement_source}\n{name}"), Some(name))
                }
                StmtKind::Expr(_) => (format!("{prefix}{statement_source}"), None),
                _ => unreachable!("facts_for_source rejects unsupported statements"),
            };
            self.charge(script.len())?;
            let value = run_source(&script, &[])
                .map_err(|error| format!("{}: {}", error.code, error.message))?
                .value;
            if let Some(name) = defined {
                if value == Value::Unit {
                    return Err("CGNK001: Unit bindings are unsupported".into());
                }
                pending.insert(name.clone(), value);
                tail = Value::Unit;
            } else {
                tail = value;
            }
        }

        let retained_bytes = self
            .bindings
            .iter()
            .map(|(name, binding)| name.len() + binding.cell_id.len() + value_size(&binding.value))
            .sum::<usize>()
            + pending
                .iter()
                .map(|(name, value)| name.len() + id.len() + value_size(value))
                .sum::<usize>();
        if retained_bytes > MAX_REPLAY_BYTES {
            return Err(limit_error());
        }
        for (name, value) in pending {
            self.bindings.insert(
                name,
                RetainedBinding {
                    value,
                    cell_id: id.to_owned(),
                },
            );
        }
        Ok(CellResult {
            text: (tail != Value::Unit).then(|| tail.to_string()),
        })
    }
}

fn limit_error() -> String {
    format!("CGNK006: retained-state evaluation budget of {MAX_REPLAY_BYTES} bytes exceeded")
}

fn value_size(value: &Value) -> usize {
    match value {
        Value::String(text) => text.len(),
        _ => 8,
    }
}

fn append_binding(script: &mut String, name: &str, value: &Value) -> Result<(), String> {
    script.push_str("let ");
    script.push_str(name);
    script.push_str(" = ");
    match value {
        Value::I64(number) if *number == i64::MIN => {
            script.push_str("0 - 9223372036854775807 - 1");
        }
        Value::I64(number) if *number < 0 => {
            script.push_str("0 - ");
            script.push_str(&(-number).to_string());
        }
        Value::I64(number) => script.push_str(&number.to_string()),
        Value::Bool(boolean) => script.push_str(&boolean.to_string()),
        Value::String(text) => {
            script.push('"');
            for character in text.chars() {
                match character {
                    '\n' => script.push_str("\\n"),
                    '\r' => script.push_str("\\r"),
                    '\t' => script.push_str("\\t"),
                    '"' => script.push_str("\\\""),
                    '\\' => script.push_str("\\\\"),
                    other => script.push(other),
                }
            }
            script.push('"');
        }
        Value::Unit => return Err("CGNK001: Unit bindings are unsupported".into()),
    }
    script.push('\n');
    Ok(())
}

impl CellExecutor for RetainedExecutor {
    fn execute(&mut self, source: &str) -> Result<CellResult, String> {
        self.execute_named("", source)
    }

    fn execute_cell(&mut self, id: &str, source: &str) -> Result<CellResult, String> {
        self.execute_named(id, source)
    }

    fn invalidate_cells(&mut self, ids: &[String]) {
        let invalidated: BTreeSet<_> = ids.iter().collect();
        self.bindings
            .retain(|_, binding| !invalidated.contains(&binding.cell_id));
    }

    fn reset(&mut self) -> Result<(), String> {
        self.bindings.clear();
        self.evaluation_bytes = 0;
        Ok(())
    }
}
