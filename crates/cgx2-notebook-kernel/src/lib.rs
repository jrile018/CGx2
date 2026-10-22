//! Checked, bounded retained-state execution of pure scalar CGx2 cells.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
pub const DEFAULT_CELL_TIMEOUT: Duration = Duration::from_secs(2);
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

    /// Recompute the selected cell and only its stale transitive readers.
    /// Independent cells keep their captured output and execution revision.
    pub fn rerun_affected(&mut self, id: &str) -> Result<(), KernelError> {
        let affected = self.session.affected_ids(id).map_err(session_error)?;
        self.rerun_cell(id)?;
        for candidate in affected {
            if self.session.status(&candidate).map_err(session_error)? == CellStatus::Stale {
                if let Err(error) = self.session.run_cell(&candidate) {
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
        let _ = new_session(proposed)?;
        self.session
            .replace_cell(id, source.clone(), facts_or_opaque(&source))
            .map_err(session_error)?;
        self.requires_restart = true;
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

fn new_session(notebook: Notebook) -> Result<NotebookSession<RetainedExecutor>, KernelError> {
    let facts = notebook
        .cells
        .iter()
        .filter(|cell| cell.kind == CellKind::Code)
        .map(|cell| (cell.id.clone(), facts_or_opaque(&cell.source)))
        .collect::<BTreeMap<_, _>>();
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
                return Err(error("CGNK009", format!(
                    "cell {} has a forward dependency on {read}; dependency cycles and forward references are unsupported",
                    cell.id
                )));
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
        ..RetainedExecutor::default()
    };
    NotebookSession::new(notebook, facts, executor).map_err(session_error)
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
    let mut locals = BTreeSet::new();
    for statement in &module.statements {
        match &statement.kind {
            StmtKind::Let {
                mutable: false,
                name,
                value,
                ..
            } => {
                collect_external_reads(value, &locals, &mut facts)?;
                facts.defines.insert(name.clone());
                locals.insert(name.clone());
            }
            StmtKind::Expr(value) => collect_external_reads(value, &locals, &mut facts)?,
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

fn collect_external_reads(
    expression: &Expr,
    locals: &BTreeSet<String>,
    facts: &mut CellFacts,
) -> Result<(), KernelError> {
    let mut reads = CellFacts::default();
    collect_reads(expression, &mut reads)?;
    facts.reads.extend(reads.reads.difference(locals).cloned());
    Ok(())
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
                "CGNK011",
                "calls are unsupported in notebook scalar execution; file, network, and foreign effects cannot run in this worker",
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
    bindings: BTreeMap<String, Vec<RetainedBinding>>,
    order: BTreeMap<String, usize>,
    evaluation_bytes: usize,
    control: Option<ExecutionControl>,
}

impl RetainedExecutor {
    fn prefix(&self, id: &str, pending: &BTreeMap<String, Value>) -> Result<String, String> {
        let mut script = String::new();
        let selected_index = self.order.get(id).copied().unwrap_or(usize::MAX);
        for (name, bindings) in &self.bindings {
            if let Some(binding) = bindings
                .iter()
                .filter(|binding| self.order[&binding.cell_id] < selected_index)
                .max_by_key(|binding| self.order[&binding.cell_id])
            {
                append_binding(&mut script, name, &binding.value)?;
            }
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
        let deadline = self
            .control
            .as_ref()
            .map(|control| Instant::now() + control.timeout);
        facts_for_source(source).map_err(|error| error.to_string())?;
        let module = parse_module(source).map_err(|diagnostics| {
            let first = &diagnostics[0];
            format!("{}: {}", first.code, first.message)
        })?;
        let prefix = self.prefix(id, &BTreeMap::new())?;
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
            let prefix = self.prefix(id, &pending)?;
            let (script, defined) = match &statement.kind {
                StmtKind::Let { name, .. } => {
                    (format!("{prefix}{statement_source}\n{name}"), Some(name))
                }
                StmtKind::Expr(_) => (format!("{prefix}{statement_source}"), None),
                _ => unreachable!("facts_for_source rejects unsupported statements"),
            };
            self.charge(script.len())?;
            let value = match &self.control {
                Some(control) => {
                    run_worker(&script, control, deadline.expect("controlled deadline"))?
                }
                None => {
                    run_source(&script, &[])
                        .map_err(|error| format!("{}: {}", error.code, error.message))?
                        .value
                }
            };
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
            .flat_map(|(name, bindings)| {
                bindings.iter().map(move |binding| {
                    name.len() + binding.cell_id.len() + value_size(&binding.value)
                })
            })
            .sum::<usize>()
            + pending
                .iter()
                .map(|(name, value)| name.len() + id.len() + value_size(value))
                .sum::<usize>();
        if retained_bytes > MAX_REPLAY_BYTES {
            return Err(limit_error());
        }
        if self
            .control
            .as_ref()
            .is_some_and(|control| control.interrupted.load(Ordering::Acquire))
            || deadline.is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err("CGNK010: cell interrupted or timed out".into());
        }
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
        self.bindings.retain(|_, bindings| {
            bindings.retain(|binding| !invalidated.contains(&binding.cell_id));
            !bindings.is_empty()
        });
    }

    fn reset(&mut self) -> Result<(), String> {
        self.bindings.clear();
        self.evaluation_bytes = 0;
        Ok(())
    }
}
