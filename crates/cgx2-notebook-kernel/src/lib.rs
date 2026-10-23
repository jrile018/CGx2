//! Checked, bounded retained-state execution of pure scalar CGx2 cells.
mod project;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{Read, Write};
use std::path::{Path as FsPath, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cgx2_cli::{check_source, run_source, Value};
use cgx2_notebook_format::{
    source_hash, CellKind, Notebook, ProvenanceResult, CAPTURE_SCHEMA_VERSION,
};
use cgx2_notebook_session::{
    CellExecutor, CellFacts, CellResult, CellStatus, NotebookSession, SessionError,
};
use cgx2_syntax::{parse_module, Expr, ExprKind, Function, Path, StmtKind};
use project::Project;

pub const MAX_CODE_CELLS: usize = 128;
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
pub const MAX_EVALUATION_BYTES: usize = 1024 * 1024;
pub const MAX_PURE_FUNCTIONS: usize = 32;
pub const MAX_FUNCTION_PARAMETERS: usize = 8;
pub const MAX_STATIC_STEPS: usize = 100_000;
/// Compatibility alias for the cumulative evaluation budget.
pub const MAX_REPLAY_BYTES: usize = MAX_EVALUATION_BYTES;
pub const DEFAULT_CELL_TIMEOUT: Duration = Duration::from_secs(2);
pub const MAX_SWEEP_VALUES: usize = 32;
pub const MAX_SWEEP_VALUE_BYTES: usize = 4096;
const WORKER_ARGUMENT: &str = "--cgx2-notebook-scalar-worker";

/// Execute one scalar script in the calling notebook binary's private worker mode.
/// The mode is deliberately not part of the public notebook command grammar.
pub fn is_worker_argument(argument: &str) -> bool {
    argument == WORKER_ARGUMENT
}

pub fn scalar_worker_main() -> i32 {
    let mut script = Vec::new();
    if std::io::stdin()
        .take((MAX_EVALUATION_BYTES + 1) as u64)
        .read_to_end(&mut script)
        .is_err()
        || script.len() > MAX_EVALUATION_BYTES
    {
        return 2;
    }
    let Ok(script) = String::from_utf8(script) else {
        return 2;
    };
    let result = match run_source(&script, &[]) {
        Ok(result) => match result.value {
            Value::I64(number) => format!("I{number}").into_bytes(),
            Value::Bool(boolean) => format!("B{boolean}").into_bytes(),
            Value::String(value) => {
                let mut bytes = vec![b'S'];
                bytes.extend(value.into_bytes());
                bytes
            }
            Value::Unit => vec![b'U'],
            Value::Record { name, fields } => encode_worker_record(&name, &fields)
                .unwrap_or_else(|message| format!("E{message}").into_bytes()),
        },
        Err(error) => format!("E{}: {}", error.code, error.message).into_bytes(),
    };
    if std::io::stdout().write_all(&result).is_ok() {
        0
    } else {
        2
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionControl {
    pub worker_executable: PathBuf,
    pub timeout: Duration,
    pub interrupted: Arc<AtomicBool>,
}

impl ExecutionControl {
    pub fn new(worker_executable: PathBuf, timeout: Duration) -> Self {
        Self {
            worker_executable,
            timeout,
            interrupted: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn interrupt(&self) {
        self.interrupted.store(true, Ordering::Release);
    }

    pub fn clear_interrupt(&self) {
        self.interrupted.store(false, Ordering::Release);
    }
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SweepScalarKind {
    I64,
    Bool,
    String,
}

#[derive(Clone, Debug)]
pub struct SweepParameter {
    source: String,
    literal_start: usize,
    literal_end: usize,
    kind: SweepScalarKind,
}

impl SweepParameter {
    pub fn parse(source: &str) -> Result<Self, KernelError> {
        let module = parse_module(source).map_err(|diagnostics| {
            let first = &diagnostics[0];
            error("CGNK011", format!("{}: {}", first.code, first.message))
        })?;
        if module.name.is_some()
            || !module.imports.is_empty()
            || !module.functions.is_empty()
            || module.statements.len() != 1
        {
            return Err(error(
                "CGNK011",
                "parameter cell must contain one immutable scalar literal binding",
            ));
        }
        let StmtKind::Let {
            mutable: false,
            value,
            ..
        } = &module.statements[0].kind
        else {
            return Err(error(
                "CGNK011",
                "parameter cell must contain one immutable scalar literal binding",
            ));
        };
        let kind = scalar_kind(&value.kind)?;
        check_source(source).map_err(|diagnostics| {
            let first = &diagnostics[0];
            error("CGNK011", format!("{}: {}", first.code, first.message))
        })?;
        Ok(Self {
            source: source.to_owned(),
            literal_start: value.span.start,
            literal_end: value.span.end,
            kind,
        })
    }

    pub fn replacement(&self, literal: &str) -> Result<(String, String), KernelError> {
        if literal.len() > MAX_SWEEP_VALUE_BYTES {
            return Err(error("CGNK011", "sweep value exceeds 4096 bytes"));
        }
        let parsed = Self::parse(&format!("let sweep_value = {literal}\n"))?;
        if parsed.kind != self.kind {
            return Err(error(
                "CGNK011",
                "sweep value changes the parameter scalar type",
            ));
        }
        let value_module =
            parse_module(literal).map_err(|_| error("CGNK011", "invalid sweep literal"))?;
        let StmtKind::Expr(value) = &value_module.statements[0].kind else {
            return Err(error("CGNK011", "sweep value must be one scalar literal"));
        };
        let key = match &value.kind {
            ExprKind::Integer(raw) => raw
                .replace('_', "")
                .parse::<i64>()
                .map(|number| format!("i:{number}"))
                .map_err(|_| error("CGNK011", "sweep integer must fit i64"))?,
            ExprKind::Boolean(boolean) => format!("b:{boolean}"),
            ExprKind::StringLiteral(value) => format!("s:{value}"),
            _ => return Err(error("CGNK011", "sweep value must be one scalar literal")),
        };
        let mut source = self.source.clone();
        source.replace_range(self.literal_start..self.literal_end, literal);
        if source.len() > MAX_SOURCE_BYTES {
            return Err(error(
                "CGNK011",
                "sweep source exceeds notebook source limit",
            ));
        }
        check_source(&source).map_err(|diagnostics| {
            let first = &diagnostics[0];
            error("CGNK011", format!("{}: {}", first.code, first.message))
        })?;
        Ok((source, key))
    }
}

fn scalar_kind(expr: &ExprKind) -> Result<SweepScalarKind, KernelError> {
    match expr {
        ExprKind::Integer(_) => Ok(SweepScalarKind::I64),
        ExprKind::Boolean(_) => Ok(SweepScalarKind::Bool),
        ExprKind::StringLiteral(_) => Ok(SweepScalarKind::String),
        _ => Err(error(
            "CGNK011",
            "parameter value must be an i64, Bool, or String literal",
        )),
    }
}

/// Reject opaque or effectful source before any sweep files are published.
pub fn validate_pure_sweep_notebook(notebook: &Notebook) -> Result<(), KernelError> {
    validate_pure_sweep_notebook_with_project(notebook, None)
}

/// Validate a clean sweep against the same checked package boundary as cell execution.
pub fn validate_pure_sweep_notebook_with_project(
    notebook: &Notebook,
    root: Option<&FsPath>,
) -> Result<(), KernelError> {
    validate_limits(notebook)?;
    let mut known_functions = BTreeSet::new();
    for cell in notebook
        .cells
        .iter()
        .filter(|cell| cell.kind == CellKind::Code)
    {
        facts_for_source(&cell.source)?;
        if facts_or_opaque(&cell.source, &known_functions, false).opaque_effects {
            return Err(error(
                "CGNK001",
                format!("cell {} has an unknown or effectful call", cell.id),
            ));
        }
        let module =
            parse_module(&cell.source).map_err(|_| error("CGNK001", "invalid cell source"))?;
        known_functions.extend(
            module
                .functions
                .iter()
                .map(|function| function.name.clone()),
        );
        known_functions.extend(module.records.iter().map(|record| record.name.clone()));
        known_functions.extend(
            module
                .imports
                .iter()
                .filter_map(|path| path.segments.last().cloned()),
        );
        known_functions.extend(module.imports.iter().map(|path| path.segments.join(".")));
    }
    let project = root
        .map(Project::load)
        .transpose()
        .map_err(|message| error("CGNK012", message))?;
    new_session(notebook.clone(), project)?;
    Ok(())
}

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
            } else if let Some(reason) = message.strip_prefix("CGNK010: ") {
                ("CGNK010", reason.to_owned())
            } else if let Some(reason) = message.strip_prefix("CGNK011: ") {
                ("CGNK011", reason.to_owned())
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
        Self::open_with_project(notebook, None)
    }

    /// Open with an explicit local project root. The package checker remains the
    /// source of truth; execution uses only a narrowed notebook scalar lowering.
    pub fn open_with_project(
        notebook: Notebook,
        root: Option<&FsPath>,
    ) -> Result<Self, KernelError> {
        validate_limits(&notebook)?;
        let project = root.map(Project::load).transpose().map_err(|message| {
            let imports: Vec<_> = notebook
                .cells
                .iter()
                .filter_map(|cell| {
                    parse_module(&cell.source)
                        .ok()
                        .map(|module| (cell, module.imports))
                })
                .flat_map(|(cell, imports)| imports.into_iter().map(move |path| (cell, path)))
                .collect();
            if let Some((cell, path)) = imports
                .iter()
                .find(|(_, path)| {
                    path.segments
                        .last()
                        .is_some_and(|name| message.contains(&format!("/{name}.cg")))
                })
                .or_else(|| imports.first())
            {
                return KernelError {
                    code: "CGNK012",
                    message: format!(
                        "imported package boundary at {}..{}: {message}",
                        path.span.start, path.span.end
                    ),
                    cell_id: Some(cell.id.clone()),
                };
            }
            error("CGNK012", message)
        })?;
        let code_order = notebook
            .cells
            .iter()
            .filter(|cell| cell.kind == CellKind::Code)
            .map(|cell| cell.id.clone())
            .collect();
        let session = new_session(notebook, project)?;
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

    /// Use a killable child process for each scalar statement. Callers may share
    /// `control.interrupted` with their request handler to stop an active cell.
    pub fn set_execution_control(&mut self, control: ExecutionControl) {
        self.session.executor_mut().control = Some(control);
    }

    pub fn interrupt(&mut self) {
        if let Some(control) = &self.session.executor_mut().control {
            control.interrupt();
        }
    }

    /// Drop live bindings and reset file-order execution without evaluating cells.
    /// Saved outputs and provenance remain observations and display as stale.
    pub fn restart(&mut self) -> Result<(), KernelError> {
        self.session.restart().map_err(session_error)?;
        if let Some(control) = &self.session.executor_mut().control {
            control.clear_interrupt();
        }
        self.next_code = 0;
        self.requires_restart = false;
        Ok(())
    }

    pub fn status(&self, id: &str) -> Result<CellStatus, KernelError> {
        self.session.status(id).map_err(session_error)
    }

    pub fn next_cell_id(&self) -> Option<&str> {
        self.code_order.get(self.next_code).map(String::as_str)
    }

    fn check_project(&mut self, id: &str) -> Result<(), KernelError> {
        let failure = self
            .session
            .executor_mut()
            .project
            .as_ref()
            .and_then(|project| {
                project
                    .verify_unchanged()
                    .err()
                    .map(|message| (message, project.changed_modules()))
            });
        if let Some((message, modules)) = failure {
            self.session.invalidate_package_modules(&modules);
            let end = self
                .notebook()
                .cells
                .iter()
                .find(|cell| cell.id == id)
                .map_or(0, |cell| cell.source.len());
            return Err(KernelError {
                code: "CGNK012",
                message: format!(
                    "{message} at 0..{end}; reopen the checked project before execution"
                ),
                cell_id: Some(id.to_owned()),
            });
        }
        Ok(())
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
        self.check_project(id)?;
        let result = self.session.run_cell(id);
        self.check_project(id)?;
        result.map_err(session_error)?;
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
        self.check_project(id)?;
        let result = self.session.rerun_cell(id);
        self.check_project(id)?;
        if let Err(error) = result {
            self.requires_restart = true;
            return Err(session_error(error));
        }
        self.next_code = index + 1;
        self.requires_restart = false;
        Ok(())
    }

    /// Recompute the selected cell and only its stale transitive readers.
    /// Independent cells keep their captured output and execution revision.
    pub fn rerun_affected(&mut self, id: &str) -> Result<(), KernelError> {
        let affected = self.session.affected_ids(id).map_err(session_error)?;
        self.rerun_cell(id)?;
        for candidate in affected {
            if self.session.status(&candidate).map_err(session_error)? == CellStatus::Stale {
                self.check_project(&candidate)?;
                let result = self.session.run_cell(&candidate);
                self.check_project(&candidate)?;
                if let Err(error) = result {
                    self.requires_restart = true;
                    return Err(session_error(error));
                }
            }
        }
        self.next_code = self
            .code_order
            .iter()
            .position(|candidate| self.session.status(candidate).ok() != Some(CellStatus::Current))
            .unwrap_or(self.code_order.len());
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
        let known_functions = known_functions_before(&proposed, id);
        let project = self.session.executor_mut().project.clone();
        let validated = new_session(proposed, project)?;
        self.session
            .replace_cell(
                id,
                source.clone(),
                facts_or_opaque(&source, &known_functions, false),
            )
            .map_err(session_error)?;
        self.session
            .replace_package_provenance(validated.package_provenance().clone());
        self.requires_restart = true;
        Ok(())
    }

    /// Delete a cell without replaying other cells. Outputs that used its
    /// definitions remain visible as stale saved observations.
    pub fn remove_cell(&mut self, id: &str) -> Result<(), KernelError> {
        let code_index = self.code_order.iter().position(|candidate| candidate == id);
        self.session.remove_cell(id).map_err(session_error)?;
        if let Some(index) = code_index {
            self.code_order.remove(index);
            self.session.executor_mut().order.remove(id);
            if index < self.next_code {
                self.next_code -= 1;
            }
            self.requires_restart = true;
        }
        Ok(())
    }

    /// Clears retained state and runs all code cells in file order. Old output
    /// observations remain visible if a later cell fails or is interrupted.
    pub fn restart_and_run_all(&mut self) -> Result<(), KernelError> {
        self.restart()?;
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

fn new_session(
    notebook: Notebook,
    project: Option<Project>,
) -> Result<NotebookSession<RetainedExecutor>, KernelError> {
    let mut facts = BTreeMap::new();
    let mut known_functions = BTreeSet::new();
    let mut imported = BTreeMap::<String, String>::new();
    let mut package_provenance = BTreeMap::new();
    let mut local_roots = BTreeSet::new();
    for cell in notebook
        .cells
        .iter()
        .filter(|cell| cell.kind == CellKind::Code)
    {
        // A schema-2 call invalidated or failed with unchanged source cannot
        // have executed an unknown effect through this kernel. A removed pure
        // definition makes its result stale without tainting unrelated cells.
        let safe_unresolved_call = cell.provenance.as_ref().is_some_and(|provenance| {
            provenance.schema_version == CAPTURE_SCHEMA_VERSION
                && provenance.source_sha256 == source_hash(&cell.source)
                && (provenance.result == ProvenanceResult::Failed || provenance.stale)
        });
        let cell_facts = facts_or_opaque(&cell.source, &known_functions, safe_unresolved_call);
        if let Ok(module) = parse_module(&cell.source) {
            if module.name.is_some() {
                return Err(error(
                    "CGNK012",
                    format!("cell {} cannot declare a module", cell.id),
                ));
            }
            if !module.imports.is_empty() && project.is_none() {
                return Err(error(
                    "CGNK012",
                    format!("cell {} import requires --project ROOT", cell.id),
                ));
            }
            if let Some(project) = &project {
                for root in &cell_facts.defines {
                    if !module.imports.is_empty() {
                        continue;
                    }
                    if imported.contains_key(root)
                        || imported
                            .values()
                            .any(|name| name.split('.').next() == Some(root.as_str()))
                    {
                        return Err(error(
                            "CGNK012",
                            format!(
                                "cell {}: ambiguous local/module root {root} at 0..{}",
                                cell.id,
                                cell.source.len()
                            ),
                        ));
                    }
                    local_roots.insert(root.clone());
                }
                if cell.source.contains("__cgx2_pkg") {
                    return Err(error(
                        "CGNK012",
                        format!("cell {} uses reserved package symbol prefix", cell.id),
                    ));
                }
                let mut used = BTreeSet::new();
                for read in &cell_facts.reads {
                    for (short, name) in &imported {
                        if read == short || read == name {
                            used.insert(name.clone());
                        }
                    }
                }
                if !module.imports.is_empty()
                    && (!module.statements.is_empty() || !module.functions.is_empty())
                {
                    return Err(error(
                        "CGNK012",
                        format!("cell {}: imports must occupy their own code cell", cell.id),
                    ));
                }
                project
                    .lower_cell(&cell.source, &module, &imported)
                    .map_err(|message| error("CGNK012", format!("cell {}: {message}", cell.id)))?;
                for path in &module.imports {
                    let (short, full) = project.resolve_import(path).map_err(|message| {
                        error("CGNK012", format!("cell {}: {message}", cell.id))
                    })?;
                    if local_roots.contains(&short)
                        || full
                            .split('.')
                            .next()
                            .is_some_and(|root| local_roots.contains(root))
                    {
                        return Err(error(
                            "CGNK012",
                            format!(
                                "cell {}: ambiguous local/module root {short} at {}..{}",
                                cell.id, path.span.start, path.span.end
                            ),
                        ));
                    }
                    if imported.contains_key(&short) {
                        return Err(error(
                            "CGNK012",
                            format!(
                                "cell {}: duplicate module short name {short} at {}..{}",
                                cell.id, path.span.start, path.span.end
                            ),
                        ));
                    }
                    imported.insert(short, full.clone());
                    used.insert(full);
                }
                project
                    .functions_for(&imported)
                    .map_err(|message| error("CGNK012", format!("cell {}: {message}", cell.id)))?;
                if let Some(provenance) = project
                    .provenance(&used)
                    .map_err(|message| error("CGNK012", message))?
                {
                    package_provenance.insert(cell.id.clone(), provenance);
                }
            }
            if !cell_facts.opaque_effects {
                known_functions.extend(
                    module
                        .functions
                        .iter()
                        .map(|function| function.name.clone()),
                );
                known_functions.extend(module.records.iter().map(|record| record.name.clone()));
                known_functions.extend(module.imports.iter().map(|path| path.segments.join(".")));
                known_functions.extend(
                    module
                        .imports
                        .iter()
                        .filter_map(|path| path.segments.last().cloned()),
                );
            }
        }
        facts.insert(cell.id.clone(), cell_facts);
    }
    for (index, cell) in notebook.cells.iter().enumerate() {
        if cell.kind != CellKind::Code {
            continue;
        }
        for read in &facts[&cell.id].reads {
            if notebook.cells[..index].iter().any(|earlier| {
                earlier.kind == CellKind::Code && facts[&earlier.id].defines.contains(read)
            }) {
                continue;
            }
            if notebook.cells[index + 1..].iter().any(|later| {
                later.kind == CellKind::Code && facts[&later.id].defines.contains(read)
            }) {
                return Err(KernelError {
                    code: "CGNK009",
                    message: format!(
                        "cell {} has a forward dependency on {}; dependency cycles and forward references are unsupported at 0..{}",
                        cell.id, read.strip_prefix("type:").unwrap_or(read), cell.source.len()
                    ),
                    cell_id: Some(cell.id.clone()),
                });
            }
        }
    }
    let order = notebook
        .cells
        .iter()
        .enumerate()
        .filter(|(_, cell)| cell.kind == CellKind::Code)
        .map(|(index, cell)| (cell.id.clone(), index))
        .collect();
    let executor = RetainedExecutor {
        order,
        project,
        ..RetainedExecutor::default()
    };
    NotebookSession::new_with_packages(notebook, facts, executor, package_provenance)
        .map_err(session_error)
}

fn known_functions_before(notebook: &Notebook, id: &str) -> BTreeSet<String> {
    let mut known = BTreeSet::new();
    for cell in notebook.cells.iter().take_while(|cell| cell.id != id) {
        if cell.kind != CellKind::Code
            || facts_or_opaque(&cell.source, &known, false).opaque_effects
        {
            continue;
        }
        if let Ok(module) = parse_module(&cell.source) {
            known.extend(
                module
                    .functions
                    .iter()
                    .map(|function| function.name.clone()),
            );
            known.extend(module.records.iter().map(|record| record.name.clone()));
            known.extend(module.imports.iter().map(|path| path.segments.join(".")));
            known.extend(
                module
                    .imports
                    .iter()
                    .filter_map(|path| path.segments.last().cloned()),
            );
        }
    }
    known
}

fn facts_or_opaque(
    source: &str,
    known_functions: &BTreeSet<String>,
    safe_unresolved_call: bool,
) -> CellFacts {
    let Ok(mut facts) = facts_for_source(source) else {
        return CellFacts {
            opaque_effects: true,
            ..CellFacts::default()
        };
    };
    if let Ok(module) = parse_module(source) {
        let mut calls = BTreeSet::new();
        for function in &module.functions {
            for statement in &function.body {
                collect_call_names_statement(statement, &mut calls);
            }
        }
        for statement in &module.statements {
            collect_call_names_statement(statement, &mut calls);
        }
        if !safe_unresolved_call
            && calls
                .iter()
                .any(|call| !known_functions.contains(call) && !facts.defines.contains(call))
        {
            facts.opaque_effects = true;
        }
    }
    facts
}

fn collect_call_names_statement(statement: &cgx2_syntax::Stmt, names: &mut BTreeSet<String>) {
    match &statement.kind {
        StmtKind::If {
            condition,
            then_body,
            else_body,
        } => {
            collect_call_names(condition, names);
            for stmt in then_body.iter().chain(else_body) {
                collect_call_names_statement(stmt, names);
            }
        }
        StmtKind::Let { value, .. } | StmtKind::Expr(value) | StmtKind::Return(Some(value)) => {
            collect_call_names(value, names)
        }
        _ => {}
    }
}

fn collect_call_names(expression: &Expr, names: &mut BTreeSet<String>) {
    match &expression.kind {
        ExprKind::Binary { left, right, .. } => {
            collect_call_names(left, names);
            collect_call_names(right, names);
        }
        ExprKind::Call { callee, arguments } => {
            if let ExprKind::Name(path) = &callee.kind {
                if let Some(name) = path.segments.first() {
                    if !name.starts_with("$cgx2_cmp_") {
                        names.insert(if path.segments.len() > 2 {
                            path.segments[..path.segments.len() - 1].join(".")
                        } else {
                            name.to_owned()
                        });
                    }
                }
            }
            for argument in arguments {
                collect_call_names(&argument.value, names);
            }
        }
        _ => {}
    }
}

/// Extracts simple definitions and reads from the syntax tree; unsupported forms fail closed.
pub fn facts_for_source(source: &str) -> Result<CellFacts, KernelError> {
    let module = parse_module(source).map_err(|diagnostics| {
        let first = &diagnostics[0];
        error(
            "CGNK008",
            format!(
                "{}: {} at {}..{}",
                first.code, first.message, first.span.start, first.span.end
            ),
        )
    })?;
    if module.name.is_some() {
        return Err(error(
            "CGNK001",
            "module declarations unsupported in notebook execution",
        ));
    }
    let mut facts = CellFacts::default();
    if !module.imports.is_empty() {
        if !module.functions.is_empty()
            || !module.records.is_empty()
            || !module.statements.is_empty()
        {
            return Err(error("CGNK012", "imports must occupy own code cell"));
        }
        for path in &module.imports {
            let short = path
                .segments
                .last()
                .ok_or_else(|| error("CGNK012", "empty import"))?;
            if !facts.defines.insert(short.clone()) {
                return Err(error(
                    "CGNK012",
                    format!("duplicate module short name {short}"),
                ));
            }
        }
        for path in &module.imports {
            facts.defines.insert(path.segments.join("."));
        }
        return Ok(facts);
    }
    for record in &module.records {
        if record.fields.is_empty() || record.fields.len() > 8 {
            return Err(error(
                "CGNK001",
                format!(
                    "record {} requires one to eight fields at {}..{}",
                    record.name, record.span.start, record.span.end
                ),
            ));
        }
        for field in &record.fields {
            if !matches!(field.ty.to_string().as_str(), "i64" | "Bool") {
                return Err(error(
                    "CGNK001",
                    format!(
                        "record field {} must be i64 or Bool at {}..{}",
                        field.name, field.span.start, field.span.end
                    ),
                ));
            }
        }
        if !facts.defines.insert(record.name.clone()) {
            return Err(error(
                "CGNK001",
                format!(
                    "duplicate definition {} at {}..{}",
                    record.name, record.span.start, record.span.end
                ),
            ));
        }
        // Type names occupy their own namespace: a value binding named Point
        // must not hide the producer of a Point annotation from invalidation.
        facts.defines.insert(format!("type:{}", record.name));
    }
    if module.functions.len() > MAX_PURE_FUNCTIONS {
        return Err(error("CGNK006", "too many pure functions in one cell"));
    }
    for function in &module.functions {
        validate_function(function, &mut facts)?;
    }
    let mut locals = BTreeSet::new();
    for statement in &module.statements {
        match &statement.kind {
            StmtKind::If { .. } => {
                return Err(error(
                    "CGNK001",
                    "top-level conditional cells are unsupported; use a pure function body",
                ))
            }
            StmtKind::Let {
                mutable: false,
                name,
                annotation,
                value,
            } => {
                if let Some(ty) = annotation {
                    function_type(ty, &mut facts)?;
                }
                collect_external_reads(value, &locals, &mut facts)?;
                facts.defines.insert(name.clone());
                locals.insert(name.clone());
            }
            StmtKind::Expr(value) => collect_external_reads(value, &locals, &mut facts)?,
            _ => {
                return Err(error(
                    "CGNK001",
                    format!(
                        "only immutable let bindings and pure expressions supported at {}..{}",
                        statement.span.start, statement.span.end
                    ),
                ))
            }
        }
    }
    facts.reads.retain(|name| {
        !module
            .records
            .iter()
            .any(|record| &record.name == name || format!("type:{}", record.name) == *name)
            && !module
                .functions
                .iter()
                .any(|function| &function.name == name)
    });
    Ok(facts)
}

fn validate_function(function: &Function, facts: &mut CellFacts) -> Result<(), KernelError> {
    if function.name == "main" || matches!(function.name.as_str(), "arg" | "arg_count") {
        return Err(error(
            "CGNK011",
            "main and runner builtins are reserved in notebooks",
        ));
    }
    if function.foreign.is_some()
        || function
            .effects
            .as_ref()
            .is_some_and(|decl| !decl.names.is_empty())
        || function
            .capabilities
            .as_ref()
            .is_some_and(|decl| !decl.names.is_empty())
        || !function.requires.is_empty()
        || !function.ensures.is_empty()
    {
        return Err(error("CGNK011", format!("effect, capability, foreign, and contract declarations are unsupported in pure notebook functions at {}..{}", function.span.start, function.span.end)));
    }
    if function.parameters.len() > MAX_FUNCTION_PARAMETERS {
        return Err(error("CGNK006", "too many pure function parameters"));
    }
    let mut locals = BTreeSet::new();
    for parameter in &function.parameters {
        function_type(&parameter.ty, facts)?;
        if !locals.insert(parameter.name.clone()) {
            return Err(error("CGNK001", "duplicate pure function parameter"));
        }
    }
    let Some(return_type) = &function.return_type else {
        return Err(error(
            "CGNK001",
            "pure notebook functions require a scalar or local record return type",
        ));
    };
    function_type(return_type, facts)?;
    if !facts.defines.insert(function.name.clone()) {
        return Err(error("CGNK001", "duplicate pure function name in cell"));
    }
    collect_function_block(&function.body, &mut locals, facts)?;
    Ok(())
}

fn collect_function_block(
    body: &[cgx2_syntax::Stmt],
    locals: &mut BTreeSet<String>,
    facts: &mut CellFacts,
) -> Result<(), KernelError> {
    for statement in body {
        match &statement.kind {
            StmtKind::If { condition, then_body, else_body } => {
                collect_expression(condition, locals, facts, true)?;
                collect_function_block(then_body, &mut locals.clone(), facts)?;
                collect_function_block(else_body, &mut locals.clone(), facts)?;
            }
            StmtKind::Let { mutable: false, name, annotation, value } => {
                if let Some(ty) = annotation { function_type(ty, facts)?; }
                collect_expression(value, locals, facts, true)?;
                locals.insert(name.clone());
            }
            StmtKind::Expr(value) | StmtKind::Return(Some(value)) => collect_expression(value, locals, facts, true)?,
            StmtKind::Return(None) => {},
            _ => return Err(error("CGNK011", "pure function bodies support immutable bindings, if/else, expressions and returns only")),
        }
    }
    Ok(())
}

fn function_type(ty: &cgx2_syntax::TypeSyntax, facts: &mut CellFacts) -> Result<(), KernelError> {
    if ty.arguments.is_empty() && !ty.path.segments.is_empty() {
        let name = &ty.path.segments[0];
        if ty.path.segments.len() > 1 {
            facts
                .reads
                .insert(ty.path.segments[..ty.path.segments.len() - 1].join("."));
        } else if !matches!(name.as_str(), "i64" | "Bool" | "String") {
            // Resolve nominal names with the shared checker against the records
            // visible at execution. Even an unused annotation is a dependency.
            facts.reads.insert(format!("type:{name}"));
        }
        return Ok(());
    }
    Err(error(
        "CGNK011",
        format!(
            "unsupported pure notebook type {ty}; expected a scalar or local record at {}..{}",
            ty.span.start, ty.span.end
        ),
    ))
}

fn collect_external_reads(
    expression: &Expr,
    locals: &BTreeSet<String>,
    facts: &mut CellFacts,
) -> Result<(), KernelError> {
    let mut reads = CellFacts::default();
    collect_expression(expression, locals, &mut reads, false)?;
    facts.reads.extend(reads.reads.difference(locals).cloned());
    Ok(())
}

fn collect_expression(
    expression: &Expr,
    locals: &BTreeSet<String>,
    facts: &mut CellFacts,
    function_body: bool,
) -> Result<(), KernelError> {
    match &expression.kind {
        ExprKind::Name(path) => {
            let name = simple_name(path)?;
            if !locals.contains(name) {
                if function_body {
                    return Err(error(
                        "CGNK011",
                        format!("pure notebook functions cannot capture binding {name}"),
                    ));
                }
                facts.reads.insert(name.to_owned());
            }
        }
        ExprKind::Binary { left, right, .. } => {
            collect_expression(left, locals, facts, function_body)?;
            collect_expression(right, locals, facts, function_body)?;
        }
        ExprKind::Integer(_) | ExprKind::StringLiteral(_) | ExprKind::Boolean(_) => {}
        ExprKind::Call { callee, arguments } => {
            let ExprKind::Name(path) = &callee.kind else {
                return Err(error(
                    "CGNK011",
                    "computed calls are unsupported in pure notebook cells",
                ));
            };
            let full_root = path.segments[..path.segments.len().saturating_sub(1)].join(".");
            let name = if path.segments.len() > 2 {
                full_root.as_str()
            } else {
                path.segments
                    .first()
                    .ok_or_else(|| error("CGNK001", "empty call path"))?
                    .as_str()
            };
            if matches!(name, "arg" | "arg_count") {
                return Err(error(
                    "CGNK011",
                    "argument and host calls are unsupported in pure notebook cells",
                ));
            }
            if !name.starts_with("$cgx2_cmp_") {
                facts.reads.insert(name.to_owned());
            }
            for argument in arguments {
                collect_expression(&argument.value, locals, facts, function_body)?;
            }
        }
    }
    Ok(())
}

fn simple_name(path: &Path) -> Result<&str, KernelError> {
    match path.segments.as_slice() {
        [name] | [name, _] => Ok(name),
        _ => Err(error(
            "CGNK001",
            "qualified names are unsupported in notebook scalar execution",
        )),
    }
}

fn validate_static_bounds(
    function_source: &str,
    module: &cgx2_syntax::Module,
    retained_bindings: usize,
) -> Result<(), String> {
    let functions = parse_module(function_source).map_err(|diagnostics| {
        let first = &diagnostics[0];
        format!("{}: {}", first.code, first.message)
    })?;
    if functions.functions.len() > MAX_PURE_FUNCTIONS {
        return Err("CGNK006: too many retained pure functions".into());
    }
    let records = functions
        .records
        .iter()
        .map(|record| record.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut by_name = BTreeMap::new();
    for function in &functions.functions {
        if by_name.insert(function.name.as_str(), function).is_some() {
            return Err(format!(
                "CGNK011: duplicate pure function {}",
                function.name
            ));
        }
    }
    for name in by_name.keys() {
        let cost = function_cost(name, &by_name, &records, &mut Vec::new())?;
        if cost > MAX_STATIC_STEPS {
            return Err("CGNK006: pure function step budget exceeded".into());
        }
    }
    let statement_count = module.statements.len();
    let mut cell_cost = statement_count
        .saturating_mul(retained_bindings.saturating_add(statement_count))
        .saturating_mul(3);
    for statement in &module.statements {
        cell_cost = cell_cost.saturating_add(statement_cost(
            statement,
            &by_name,
            &records,
            &mut Vec::new(),
        )?);
    }
    if cell_cost > MAX_STATIC_STEPS {
        return Err("CGNK006: cell step budget exceeded".into());
    }
    Ok(())
}

fn function_cost(
    name: &str,
    functions: &BTreeMap<&str, &Function>,
    records: &BTreeSet<&str>,
    stack: &mut Vec<String>,
) -> Result<usize, String> {
    if stack.iter().any(|item| item == name) {
        return Err(format!(
            "CGNK011: recursive pure function {name} is unsupported"
        ));
    }
    if stack.len() >= 32 {
        return Err("CGNK006: pure function call depth exceeded".into());
    }
    let function = functions.get(name).ok_or_else(|| {
        format!("CGNK011: unresolved call {name}; define or rerun a pure function before this cell; file, network, and foreign effects cannot run")
    })?;
    stack.push(name.to_owned());
    let mut cost = 0usize;
    for statement in &function.body {
        cost = cost.saturating_add(statement_cost(statement, functions, records, stack)?);
        if cost > MAX_STATIC_STEPS {
            break;
        }
    }
    stack.pop();
    Ok(cost)
}

fn statement_cost(
    statement: &cgx2_syntax::Stmt,
    functions: &BTreeMap<&str, &Function>,
    records: &BTreeSet<&str>,
    stack: &mut Vec<String>,
) -> Result<usize, String> {
    if let StmtKind::If {
        condition,
        then_body,
        else_body,
    } = &statement.kind
    {
        let mut cost =
            1usize.saturating_add(expression_cost(condition, functions, records, stack)?);
        // Count both branches conservatively for the replay admission bound.
        for stmt in then_body.iter().chain(else_body) {
            cost = cost.saturating_add(statement_cost(stmt, functions, records, stack)?);
        }
        return Ok(cost);
    }
    if matches!(statement.kind, StmtKind::Return(None)) {
        return Ok(1);
    }
    let expression = match &statement.kind {
        StmtKind::Let { value, .. } | StmtKind::Expr(value) | StmtKind::Return(Some(value)) => {
            value
        }
        _ => return Err("CGNK011: unsupported statement in pure notebook cell".into()),
    };
    Ok(1usize.saturating_add(expression_cost(expression, functions, records, stack)?))
}

fn expression_cost(
    expression: &Expr,
    functions: &BTreeMap<&str, &Function>,
    records: &BTreeSet<&str>,
    stack: &mut Vec<String>,
) -> Result<usize, String> {
    match &expression.kind {
        ExprKind::Name(_)
        | ExprKind::Integer(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::Boolean(_) => Ok(1),
        ExprKind::Binary { left, right, .. } => Ok(1usize
            .saturating_add(expression_cost(left, functions, records, stack)?)
            .saturating_add(expression_cost(right, functions, records, stack)?)),
        ExprKind::Call { callee, arguments } => {
            let ExprKind::Name(path) = &callee.kind else {
                return Err("CGNK011: computed calls are unsupported".into());
            };
            let name = simple_name(path).map_err(|error| error.to_string())?;
            let mut cost = 1usize;
            for argument in arguments {
                cost = cost.saturating_add(expression_cost(
                    &argument.value,
                    functions,
                    records,
                    stack,
                )?);
            }
            if !matches!(
                name,
                "$cgx2_cmp_eq"
                    | "$cgx2_cmp_ne"
                    | "$cgx2_cmp_lt"
                    | "$cgx2_cmp_le"
                    | "$cgx2_cmp_gt"
                    | "$cgx2_cmp_ge"
            ) && !records.contains(name)
            {
                cost = cost.saturating_add(function_cost(name, functions, records, stack)?);
            }
            Ok(cost)
        }
    }
}

fn wrapped_script(functions: &str, prefix: &str, body: &str, result: Option<&str>) -> String {
    let mut script = format!("{functions}fn main():\n");
    let mut has_statement = false;
    for line in prefix.lines().chain(body.lines()) {
        has_statement |= !line.trim().is_empty();
        script.push_str("    ");
        script.push_str(line);
        script.push('\n');
    }
    if let Some(result) = result {
        has_statement = true;
        script.push_str("    ");
        script.push_str(result);
        script.push('\n');
    }
    if !has_statement {
        script.push_str("    0\n");
    }
    script
}

#[derive(Debug, Clone)]
struct RetainedBinding {
    value: Value,
    cell_id: String,
}

#[derive(Debug, Clone)]
struct RetainedFunction {
    source: String,
    cell_id: String,
}
#[derive(Debug, Clone)]
struct RetainedRecord {
    source: String,
    cell_id: String,
}

#[derive(Debug, Clone)]
struct RetainedImport {
    module: String,
    cell_id: String,
}

#[derive(Debug, Default)]
struct RetainedExecutor {
    bindings: BTreeMap<String, Vec<RetainedBinding>>,
    functions: BTreeMap<String, RetainedFunction>,
    records: BTreeMap<String, RetainedRecord>,
    imports: BTreeMap<String, RetainedImport>,
    project: Option<Project>,
    order: BTreeMap<String, usize>,
    evaluation_bytes: usize,
    control: Option<ExecutionControl>,
}

impl RetainedExecutor {
    fn function_sources(
        &self,
        id: &str,
        module: &cgx2_syntax::Module,
        source: &str,
    ) -> Result<String, String> {
        let selected_index = self.order.get(id).copied().unwrap_or(usize::MAX);
        let visible_records: BTreeSet<_> = self
            .records
            .iter()
            .filter(|(_, record)| self.order[&record.cell_id] < selected_index)
            .map(|(name, _)| name.as_str())
            .chain(module.records.iter().map(|record| record.name.as_str()))
            .collect();
        for function in &module.functions {
            for ty in function
                .parameters
                .iter()
                .map(|parameter| &parameter.ty)
                .chain(function.return_type.iter())
            {
                let name = ty.to_string();
                if !matches!(name.as_str(), "i64" | "Bool" | "String")
                    && !visible_records.contains(name.as_str())
                    && !self.project.as_ref().is_some_and(|project| {
                        project.record(&project.canonical_identity(&name)).is_some()
                    })
                {
                    return Err(format!(
                        "CGNK011: type {ty} is not a visible local record at {}..{}",
                        ty.span.start, ty.span.end
                    ));
                }
            }
        }
        let mut retained: Vec<_> = self
            .functions
            .values()
            .filter(|function| self.order[&function.cell_id] < selected_index)
            .collect();
        retained.sort_by_key(|function| self.order[&function.cell_id]);
        let mut result = if let Some(project) = &self.project {
            project.functions_for(
                &self
                    .imports
                    .iter()
                    .filter(|(_, binding)| self.order[&binding.cell_id] < selected_index)
                    .map(|(short, binding)| (short.clone(), binding.module.clone()))
                    .collect(),
            )?
        } else {
            String::new()
        };
        for function in retained {
            result.push_str(&function.source);
            result.push('\n');
        }
        for function in &module.functions {
            if self.functions.contains_key(&function.name) {
                return Err(format!(
                    "CGNK011: duplicate retained function {}",
                    function.name
                ));
            }
            result.push_str(&source[function.span.start..function.span.end]);
            result.push('\n');
        }
        let mut prior_records: Vec<_> = self
            .records
            .values()
            .filter(|record| self.order[&record.cell_id] < selected_index)
            .collect();
        prior_records.sort_by_key(|record| self.order[&record.cell_id]);
        let mut record_source = String::new();
        for record in prior_records {
            record_source.push_str(&record.source);
            record_source.push('\n');
        }
        for record in &module.records {
            if self.records.contains_key(&record.name) {
                return Err(format!(
                    "CGNK001: duplicate retained record {} at {}..{}",
                    record.name, record.span.start, record.span.end
                ));
            }
            record_source.push_str(&source[record.span.start..record.span.end]);
            record_source.push('\n');
        }
        result.insert_str(0, &record_source);
        if result.len() > MAX_REPLAY_BYTES {
            return Err(limit_error());
        }
        Ok(result)
    }

    fn prefix(&self, id: &str, pending: &BTreeMap<String, Value>) -> Result<String, String> {
        let mut script = String::new();
        let selected_index = self.order.get(id).copied().unwrap_or(usize::MAX);
        for (name, bindings) in &self.bindings {
            if let Some(binding) = bindings
                .iter()
                .filter(|binding| self.order[&binding.cell_id] < selected_index)
                .max_by_key(|binding| self.order[&binding.cell_id])
            {
                append_binding(&mut script, name, &self.replay_value(&binding.value))?;
            }
        }
        for (name, value) in pending {
            append_binding(&mut script, name, &self.replay_value(value))?;
        }
        if script.len() > MAX_REPLAY_BYTES {
            return Err(limit_error());
        }
        Ok(script)
    }

    fn replay_value(&self, value: &Value) -> Value {
        let mut value = value.clone();
        if let Value::Record { name, .. } = &mut value {
            if let Some(lowered) = self
                .project
                .as_ref()
                .and_then(|project| project.lowered_record(name))
            {
                *name = lowered;
            }
        }
        value
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
        let Some(project) = &self.project else {
            return self.execute_pure_named(id, source);
        };
        project.verify_unchanged()?;
        facts_for_source(source).map_err(|error| error.to_string())?;
        let module = parse_module(source).map_err(|diagnostics| {
            let first = &diagnostics[0];
            format!(
                "{}: {} at {}..{}",
                first.code, first.message, first.span.start, first.span.end
            )
        })?;
        let selected_index = self.order.get(id).copied().unwrap_or(usize::MAX);
        let mut imported: BTreeMap<String, String> = self
            .imports
            .iter()
            .filter(|(_, binding)| self.order[&binding.cell_id] < selected_index)
            .map(|(short, binding)| (short.clone(), binding.module.clone()))
            .collect();
        if !module.imports.is_empty() {
            for path in &module.imports {
                let (short, full) = project.resolve_import(path)?;
                if imported.insert(short.clone(), full.clone()).is_some() {
                    return Err(format!(
                        "CGNK012: duplicate module short name {short} at {}..{}",
                        path.span.start, path.span.end
                    ));
                }
            }
            project.functions_for(&imported)?;
            for path in &module.imports {
                let (short, full) = project.resolve_import(path)?;
                self.imports.insert(
                    short,
                    RetainedImport {
                        module: full,
                        cell_id: id.to_owned(),
                    },
                );
            }
            return Ok(CellResult { text: None });
        }
        let lowered = project.lower_cell(source, &module, &imported)?;
        self.execute_pure_named(id, &lowered.source)
            .map_err(|message| lowered.diagnostic(message))
    }

    fn execute_pure_named(&mut self, id: &str, source: &str) -> Result<CellResult, String> {
        let deadline = self
            .control
            .as_ref()
            .map(|control| Instant::now() + control.timeout);
        facts_for_source(source).map_err(|error| error.to_string())?;
        let module = parse_module(source).map_err(|diagnostics| {
            let first = &diagnostics[0];
            format!("{}: {}", first.code, first.message)
        })?;
        let functions = self.function_sources(id, &module, source)?;
        validate_static_bounds(&functions, &module, self.bindings.len())?;

        let body_source = module
            .statements
            .iter()
            .map(|statement| &source[statement.span.start..statement.span.end])
            .collect::<Vec<_>>()
            .join("\n");
        if body_source.is_empty() && (!module.functions.is_empty() || !module.records.is_empty()) {
            let checked = format!("{functions}fn main() -> i64:\n    0\n");
            if checked.len() > MAX_REPLAY_BYTES {
                return Err(limit_error());
            }
            self.charge(checked.len())?;
            check_source(&checked).map_err(|diagnostics| {
                let first = &diagnostics[0];
                cell_diagnostic(
                    &first.code,
                    &first.message,
                    first.span,
                    &checked,
                    source,
                    &module,
                )
            })?;
            self.check_deadline(deadline)?;
            if self.binding_bytes() + self.definition_bytes(id, &module) > MAX_REPLAY_BYTES {
                return Err(limit_error());
            }
            if let Some(project) = &self.project {
                project.verify_unchanged()?;
            }
            self.commit_definitions(id, source, &module);
            return Ok(CellResult { text: None });
        }
        let prefix = self.prefix(id, &BTreeMap::new())?;
        let checked = if functions.is_empty() {
            format!("{prefix}{body_source}")
        } else {
            wrapped_script(&functions, &prefix, &body_source, None)
        };
        if checked.len() > MAX_REPLAY_BYTES {
            return Err(limit_error());
        }
        self.charge(checked.len())?;
        check_source(&checked).map_err(|diagnostics| {
            let first = &diagnostics[0];
            cell_diagnostic(
                &first.code,
                &first.message,
                first.span,
                &checked,
                source,
                &module,
            )
        })?;
        let mut pending = BTreeMap::new();
        let mut tail = Value::Unit;
        for statement in &module.statements {
            let statement_source = &source[statement.span.start..statement.span.end];
            let prefix = self.prefix(id, &pending)?;
            let defined = match &statement.kind {
                StmtKind::Let { name, .. } => Some(name.as_str()),
                StmtKind::Expr(_) => None,
                _ => unreachable!("facts_for_source rejects unsupported statements"),
            };
            let script = if functions.is_empty() {
                match defined {
                    Some(name) => format!("{prefix}{statement_source}\n{name}"),
                    None => format!("{prefix}{statement_source}"),
                }
            } else {
                wrapped_script(&functions, &prefix, statement_source, defined)
            };
            if script.len() > MAX_REPLAY_BYTES {
                return Err(limit_error());
            }
            self.charge(script.len())?;
            let typed = check_source(&script).map_err(|diagnostics| {
                let first = &diagnostics[0];
                format!(
                    "{}: {} at {}..{}",
                    first.code, first.message, statement.span.start, statement.span.end
                )
            })?;
            let syntax = parse_module(&script).map_err(|_| "CGNK010: invalid checked script")?;
            let expected = if let Some(main) = typed.functions.get("main") {
                main.return_type.to_string()
            } else {
                let span = match &syntax
                    .statements
                    .last()
                    .ok_or("CGNK010: missing result")?
                    .kind
                {
                    StmtKind::Expr(expr) => expr.span,
                    _ => return Err("CGNK010: missing result expression".into()),
                };
                typed
                    .expressions
                    .get(span)
                    .ok_or("CGNK010: missing checked result type")?
                    .to_string()
            };
            let mut value = match &self.control {
                Some(control) => {
                    run_worker(&script, control, deadline.expect("controlled deadline")).map_err(
                        |message| {
                            format!(
                                "{message} at {}..{}",
                                statement.span.start, statement.span.end
                            )
                        },
                    )?
                }
                None => {
                    run_source(&script, &[])
                        .map_err(|error| {
                            format!(
                                "{}: {} at {}..{}",
                                error.code, error.message, statement.span.start, statement.span.end
                            )
                        })?
                        .value
                }
            };
            if let Value::Record { name, .. } = &mut value {
                if let Some(project) = &self.project {
                    *name = project.canonical_identity(name);
                }
            }
            let expected = self.project.as_ref().map_or_else(
                || expected.clone(),
                |project| project.canonical_identity(&expected),
            );
            let actual = match &value {
                Value::Record { name, .. } => name.as_str(),
                Value::I64(_) => "i64",
                Value::Bool(_) => "Bool",
                Value::String(_) => "String",
                Value::Unit => "Unit",
            };
            if actual != expected {
                return Err(format!("CGNK010: worker result type {actual} does not match checked type {expected} at {}..{}", statement.span.start, statement.span.end));
            }
            self.validate_record_value(&value, &module)?;
            if let Some(name) = defined {
                if value == Value::Unit {
                    return Err("CGNK001: Unit bindings are unsupported".into());
                }
                pending.insert(name.to_owned(), value);
                tail = Value::Unit;
            } else {
                tail = value;
            }
        }
        let retained_bytes = self.binding_bytes()
            + pending
                .iter()
                .map(|(name, value)| name.len() + id.len() + value_size(value))
                .sum::<usize>()
            + self.definition_bytes(id, &module);
        if retained_bytes > MAX_REPLAY_BYTES {
            return Err(limit_error());
        }
        self.check_deadline(deadline)?;
        if let Some(project) = &self.project {
            project.verify_unchanged()?;
        }
        self.commit_definitions(id, source, &module);
        for (name, value) in pending {
            self.bindings
                .entry(name)
                .or_default()
                .push(RetainedBinding {
                    value,
                    cell_id: id.to_owned(),
                });
        }
        Ok(CellResult {
            text: (tail != Value::Unit).then(|| tail.to_string()),
        })
    }

    fn validate_record_value(
        &self,
        value: &Value,
        module: &cgx2_syntax::Module,
    ) -> Result<(), String> {
        let Value::Record { name, fields } = value else {
            return Ok(());
        };
        let local = self
            .project
            .as_ref()
            .and_then(|project| project.record(name))
            .or_else(|| module.records.iter().find(|record| &record.name == name));
        let retained = self
            .records
            .get(name)
            .and_then(|item| parse_module(&item.source).ok());
        let schema = local
            .or_else(|| retained.as_ref().and_then(|parsed| parsed.records.first()))
            .ok_or_else(|| format!("CGNK010: record {name} has no retained definition"))?;
        if schema.fields.len() != fields.len() {
            return Err(format!("CGNK010: record {name} field count mismatch"));
        }
        for (expected, (actual_name, actual_value)) in schema.fields.iter().zip(fields) {
            let correct_type = matches!(
                (expected.ty.to_string().as_str(), actual_value),
                ("i64", Value::I64(_)) | ("Bool", Value::Bool(_))
            );
            if expected.name != *actual_name || !correct_type {
                return Err(format!(
                    "CGNK010: record {name} field {} mismatch",
                    expected.name
                ));
            }
        }
        Ok(())
    }

    fn binding_bytes(&self) -> usize {
        self.bindings
            .iter()
            .flat_map(|(name, bindings)| {
                bindings.iter().map(move |binding| {
                    name.len() + binding.cell_id.len() + value_size(&binding.value)
                })
            })
            .sum()
    }

    fn definition_bytes(&self, id: &str, module: &cgx2_syntax::Module) -> usize {
        self.functions
            .values()
            .map(|item| item.source.len() + item.cell_id.len())
            .sum::<usize>()
            + self
                .records
                .values()
                .map(|item| item.source.len() + item.cell_id.len())
                .sum::<usize>()
            + module
                .functions
                .iter()
                .map(|item| item.span.end - item.span.start + id.len())
                .sum::<usize>()
            + module
                .records
                .iter()
                .map(|item| item.span.end - item.span.start + id.len())
                .sum::<usize>()
    }

    fn commit_definitions(&mut self, id: &str, source: &str, module: &cgx2_syntax::Module) {
        for function in &module.functions {
            self.functions.insert(
                function.name.clone(),
                RetainedFunction {
                    source: source[function.span.start..function.span.end].to_owned(),
                    cell_id: id.to_owned(),
                },
            );
        }
        for record in &module.records {
            self.records.insert(
                record.name.clone(),
                RetainedRecord {
                    source: source[record.span.start..record.span.end].to_owned(),
                    cell_id: id.to_owned(),
                },
            );
        }
    }

    fn check_deadline(&self, deadline: Option<Instant>) -> Result<(), String> {
        if self
            .control
            .as_ref()
            .is_some_and(|control| control.interrupted.load(Ordering::Acquire))
            || deadline.is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err("CGNK010: cell interrupted or timed out".into());
        }
        Ok(())
    }
}

fn cell_diagnostic(
    code: &str,
    message: &str,
    span: cgx2_syntax::Span,
    checked: &str,
    source: &str,
    module: &cgx2_syntax::Module,
) -> String {
    let spans = module
        .records
        .iter()
        .map(|item| item.span)
        .chain(module.functions.iter().map(|item| item.span))
        .chain(module.statements.iter().map(|item| item.span));
    for cell_span in spans {
        let Some(fragment) = source.get(cell_span.start..cell_span.end) else {
            continue;
        };
        if fragment.is_empty() {
            continue;
        }
        for (offset, _) in checked.match_indices(fragment) {
            if (offset..=offset + fragment.len()).contains(&span.start) {
                let start = cell_span.start + (span.start - offset);
                let end = cell_span.start + span.end.saturating_sub(offset).min(fragment.len());
                return format!("{code}: {message} at {start}..{end}");
            }
        }
    }
    format!("{code}: {message} at 0..{}", source.len())
}

const MAX_RECORD_IDENTIFIER_BYTES: usize = u16::MAX as usize;

fn valid_record_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_alphanumeric())
        && name.len() <= MAX_RECORD_IDENTIFIER_BYTES
}

fn valid_record_identity(name: &str) -> bool {
    name.len() <= MAX_RECORD_IDENTIFIER_BYTES && name.split('.').all(valid_record_identifier)
}

fn encode_worker_record(name: &str, fields: &[(String, Value)]) -> Result<Vec<u8>, String> {
    if !valid_record_identity(name) || fields.is_empty() || fields.len() > 8 {
        return Err("CGNK010: invalid record result".into());
    }
    let mut bytes = Vec::with_capacity(1 + 2 + name.len() + 1 + fields.len() * 140);
    bytes.push(b'R');
    bytes.extend_from_slice(&(name.len() as u16).to_be_bytes());
    bytes.extend_from_slice(name.as_bytes());
    bytes.push(fields.len() as u8);
    let mut seen = BTreeSet::new();
    for (field, value) in fields {
        if !valid_record_identifier(field) || !seen.insert(field) {
            return Err("CGNK010: invalid record field".into());
        }
        bytes.extend_from_slice(&(field.len() as u16).to_be_bytes());
        bytes.extend_from_slice(field.as_bytes());
        match value {
            Value::I64(number) => {
                bytes.push(b'I');
                bytes.extend_from_slice(&number.to_be_bytes());
            }
            Value::Bool(boolean) => {
                bytes.push(b'B');
                bytes.push(u8::from(*boolean));
            }
            _ => return Err("CGNK010: non-scalar record field".into()),
        }
    }
    if bytes.len() > MAX_EVALUATION_BYTES {
        return Err("CGNK010: oversized record result".into());
    }
    Ok(bytes)
}

fn decode_worker_record(bytes: &[u8]) -> Result<Value, String> {
    fn take<'a>(bytes: &'a [u8], offset: &mut usize, len: usize) -> Result<&'a [u8], String> {
        let end = offset
            .checked_add(len)
            .ok_or("CGNK010: invalid record length")?;
        let slice = bytes
            .get(*offset..end)
            .ok_or("CGNK010: invalid record length")?;
        *offset = end;
        Ok(slice)
    }
    fn identifier(bytes: &[u8], offset: &mut usize, qualified: bool) -> Result<String, String> {
        let size =
            u16::from_be_bytes(take(bytes, offset, 2)?.try_into().expect("two bytes")) as usize;
        if size == 0 || size > MAX_RECORD_IDENTIFIER_BYTES {
            return Err("CGNK010: invalid record identifier length".into());
        }
        let name = std::str::from_utf8(take(bytes, offset, size)?)
            .map_err(|_| "CGNK010: invalid record UTF-8")?;
        if !(if qualified {
            valid_record_identity(name)
        } else {
            valid_record_identifier(name)
        }) {
            return Err("CGNK010: invalid record identifier".into());
        }
        Ok(name.to_owned())
    }
    if bytes.len() > MAX_EVALUATION_BYTES || bytes.first() != Some(&b'R') {
        return Err("CGNK010: invalid record result".into());
    }
    let mut offset = 1;
    let name = identifier(bytes, &mut offset, true)?;
    let count = take(bytes, &mut offset, 1)?[0] as usize;
    if !(1..=8).contains(&count) {
        return Err("CGNK010: invalid record field count".into());
    }
    let mut fields = Vec::with_capacity(count);
    let mut seen = BTreeSet::new();
    for _ in 0..count {
        let field = identifier(bytes, &mut offset, false)?;
        if !seen.insert(field.clone()) {
            return Err("CGNK010: duplicate record field".into());
        }
        let value = match take(bytes, &mut offset, 1)?[0] {
            b'I' => Value::I64(i64::from_be_bytes(
                take(bytes, &mut offset, 8)?
                    .try_into()
                    .expect("eight bytes"),
            )),
            b'B' => match take(bytes, &mut offset, 1)?[0] {
                0 => Value::Bool(false),
                1 => Value::Bool(true),
                _ => return Err("CGNK010: invalid Boolean field".into()),
            },
            _ => return Err("CGNK010: invalid record field type".into()),
        };
        fields.push((field, value));
    }
    if offset != bytes.len() {
        return Err("CGNK010: trailing record bytes".into());
    }
    Ok(Value::Record { name, fields })
}

fn run_worker(
    script: &str,
    control: &ExecutionControl,
    deadline: Instant,
) -> Result<Value, String> {
    if control.interrupted.load(Ordering::Acquire) || Instant::now() >= deadline {
        return Err("CGNK010: cell interrupted or timed out".into());
    }
    let mut child = Command::new(&control.worker_executable)
        .arg(WORKER_ARGUMENT)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("CGNK010: cannot start scalar worker: {error}"))?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let script_bytes = script.as_bytes().to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&script_bytes));
    let stdout = child.stdout.take().expect("piped stdout");
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take((MAX_EVALUATION_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let status = loop {
        if control.interrupted.load(Ordering::Acquire) || Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = writer.join();
            let _ = reader.join();
            return Err("CGNK010: cell interrupted or timed out".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = writer.join();
                let _ = reader.join();
                return Err(format!("CGNK010: scalar worker wait failed: {error}"));
            }
        }
    };
    writer
        .join()
        .map_err(|_| "CGNK010: scalar worker writer panicked".to_owned())?
        .map_err(|error| format!("CGNK010: cannot send scalar script: {error}"))?;
    let bytes = reader
        .join()
        .map_err(|_| "CGNK010: scalar worker reader panicked".to_owned())?
        .map_err(|error| format!("CGNK010: scalar worker output failed: {error}"))?;
    if !status.success() || bytes.is_empty() || bytes.len() > MAX_EVALUATION_BYTES {
        return Err("CGNK010: scalar worker returned an invalid result".into());
    }
    if bytes[0] == b'R' {
        return decode_worker_record(&bytes);
    }
    let text = std::str::from_utf8(&bytes[1..])
        .map_err(|_| "CGNK010: scalar worker returned invalid UTF-8".to_owned())?;
    match bytes[0] {
        b'I' => text
            .parse()
            .map(Value::I64)
            .map_err(|_| "CGNK010: invalid integer result".into()),
        b'B' => text
            .parse()
            .map(Value::Bool)
            .map_err(|_| "CGNK010: invalid Boolean result".into()),
        b'S' => Ok(Value::String(text.into())),
        b'U' if text.is_empty() => Ok(Value::Unit),
        b'E' => Err(text.into()),
        _ => Err("CGNK010: invalid scalar worker result".into()),
    }
}

#[cfg(all(test, unix))]
mod worker_io_tests {
    use super::{run_worker, ExecutionControl};
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    #[test]
    fn full_stdin_pipe_does_not_block_worker_timeout() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "cgx2-notebook-no-read-{}-{nonce}",
            std::process::id()
        ));
        std::fs::write(&path, "#!/bin/sh\nexec sleep 2\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let control = ExecutionControl::new(path.clone(), Duration::from_millis(40));
        let script = "x".repeat(128 * 1024);
        let start = Instant::now();
        let result = run_worker(&script, &control, start + control.timeout);
        let _ = std::fs::remove_file(path);
        assert!(result.unwrap_err().contains("interrupted or timed out"));
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}

fn limit_error() -> String {
    format!("CGNK006: retained-state evaluation budget of {MAX_REPLAY_BYTES} bytes exceeded")
}

fn value_size(value: &Value) -> usize {
    match value {
        Value::String(text) => text.len(),
        Value::Record { name, fields } => fields.iter().fold(name.len(), |size, (field, value)| {
            size.saturating_add(field.len())
                .saturating_add(value_size(value))
        }),
        _ => 8,
    }
}

fn append_value(script: &mut String, value: &Value) -> Result<(), String> {
    match value {
        Value::I64(number) if *number == i64::MIN => script.push_str("0 - 9223372036854775807 - 1"),
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
        Value::Unit => return Err("CGNK001: Unit bindings unsupported".into()),
        Value::Record { name, fields } => {
            if fields.is_empty() || fields.len() > 8 {
                return Err("CGNK001: malformed record binding".into());
            }
            script.push_str(name);
            script.push('(');
            for (index, (field, value)) in fields.iter().enumerate() {
                if index > 0 {
                    script.push_str(", ");
                }
                if !matches!(value, Value::I64(_) | Value::Bool(_)) {
                    return Err("CGNK001: non-scalar record field unsupported".into());
                }
                script.push_str(field);
                script.push_str(": ");
                append_value(script, value)?;
            }
            script.push(')');
        }
    }
    Ok(())
}

fn append_binding(script: &mut String, name: &str, value: &Value) -> Result<(), String> {
    script.push_str("let ");
    script.push_str(name);
    script.push_str(" = ");
    append_value(script, value)?;
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
        self.bindings.retain(|_, bindings| {
            bindings.retain(|binding| !invalidated.contains(&binding.cell_id));
            !bindings.is_empty()
        });
        self.functions
            .retain(|_, function| !invalidated.contains(&function.cell_id));
        self.records
            .retain(|_, record| !invalidated.contains(&record.cell_id));
        self.imports
            .retain(|_, binding| !invalidated.contains(&binding.cell_id));
    }

    fn reset(&mut self) -> Result<(), String> {
        self.bindings.clear();
        self.functions.clear();
        self.records.clear();
        self.imports.clear();
        self.evaluation_bytes = 0;
        Ok(())
    }
}

#[cfg(test)]
mod record_protocol_tests {
    use super::{decode_worker_record, encode_worker_record, Value, MAX_EVALUATION_BYTES};

    #[test]
    fn record_protocol_preserves_source_unicode_identifiers() {
        let fields = vec![("位置".to_owned(), Value::I64(7))];
        let encoded = encode_worker_record("座標", &fields).unwrap();
        assert_eq!(
            decode_worker_record(&encoded).unwrap(),
            Value::Record {
                name: "座標".to_owned(),
                fields,
            }
        );
        assert!(encode_worker_record("1bad", &[("x".into(), Value::I64(1))]).is_err());
    }

    #[test]
    fn record_roundtrip_and_malformed_payloads() {
        let fields = vec![
            ("x".to_owned(), Value::I64(i64::MIN)),
            ("active".to_owned(), Value::Bool(true)),
        ];
        let encoded = encode_worker_record("Point", &fields).unwrap();
        assert_eq!(
            decode_worker_record(&encoded).unwrap(),
            Value::Record {
                name: "Point".into(),
                fields
            }
        );
        let mut extra = encoded.clone();
        extra.push(0);
        assert!(decode_worker_record(&extra).is_err());
        let mut short = encoded.clone();
        short.pop();
        assert!(decode_worker_record(&short).is_err());
        let mut huge_name = encoded.clone();
        huge_name[1..3].copy_from_slice(&u16::MAX.to_be_bytes());
        assert!(decode_worker_record(&huge_name).is_err());
        let mut invalid_utf8 = encoded.clone();
        invalid_utf8[3] = 0xff;
        assert!(decode_worker_record(&invalid_utf8).is_err());
        let mut invalid_count = encoded.clone();
        invalid_count[3 + "Point".len()] = 9;
        assert!(decode_worker_record(&invalid_count).is_err());
        let duplicate = encode_worker_record(
            "Point",
            &[("x".into(), Value::I64(1)), ("x".into(), Value::I64(2))],
        );
        assert!(duplicate.is_err());
        assert!(decode_worker_record(&vec![b'R'; MAX_EVALUATION_BYTES + 1]).is_err());
        assert!(
            encode_worker_record("Point", &[("x".into(), Value::String("bad".into()))]).is_err()
        );
    }
}

#[cfg(test)]
mod qualified_record_protocol_tests {
    use super::*;

    #[test]
    fn qualified_identity_preserves_unicode_and_exact_scalar_values() {
        let fields = vec![
            ("位置".to_owned(), Value::I64(i64::MIN)),
            ("有効".to_owned(), Value::Bool(true)),
        ];
        let frame = encode_worker_record("demo.座標.点", &fields).unwrap();
        assert_eq!(
            decode_worker_record(&frame).unwrap(),
            Value::Record {
                name: "demo.座標.点".into(),
                fields
            }
        );
        for bad in [
            ".Point",
            "demo..Point",
            "demo.Point.",
            "demo.7Point",
            "demo.Point(x:1)",
        ] {
            assert!(encode_worker_record(bad, &[("x".into(), Value::I64(7))]).is_err());
        }
        assert!(encode_worker_record("demo.Point", &[("x.y".into(), Value::I64(7))]).is_err());
    }
}
