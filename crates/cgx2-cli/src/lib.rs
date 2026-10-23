mod check;
mod error;
mod eval;
mod memory;
mod native;
mod native_eval;

pub use cgx2_runtime::MemoryReport;
pub use check::{check_source, CheckDiagnostic};
pub use error::RunError;
pub use eval::{RunResult, Value};
pub use memory::FailureSnapshot;
pub use native::NativeDiagnostic;
mod package_eval;

fn checked_package(root: &Path) -> Result<cgx2_package::CheckedPackage, Vec<NativeDiagnostic>> {
    cgx2_package::load_checked_package(root).map_err(|errors| {
        errors
            .into_iter()
            .map(|d| NativeDiagnostic {
                code: d.code.into(),
                message: d.message,
                file: d.file,
                span: d.span,
            })
            .collect()
    })
}

pub fn check_native_package(root: &Path) -> Result<(), Vec<NativeDiagnostic>> {
    let package = checked_package(root)?;
    if package
        .modules()
        .iter()
        .any(|m| m.syntax().functions.iter().any(|f| f.foreign.is_some()))
    {
        native::check_native_package(root)
    } else {
        Ok(())
    }
}
use std::path::Path;

pub fn run_native_package(
    root: &Path,
    args: &[String],
) -> Result<RunResult, Vec<NativeDiagnostic>> {
    let package = checked_package(root)?;
    if !package
        .modules()
        .iter()
        .any(|m| m.syntax().functions.iter().any(|f| f.foreign.is_some()))
    {
        return package_eval::run(&package, args);
    }
    native::run_native_package_value(root, args).map(|value| RunResult {
        value: match value {
            native::NativeValue::I64(value) => Value::I64(value),
            native::NativeValue::Tensor(value) => Value::String(value),
        },
    })
}

use cgx2_syntax::parse_module;

/// Opt-in failure record. `report` is present only after memory execution starts.
pub struct RunFailure {
    pub error: RunError,
    pub report: Option<MemoryReport>,
    pub rollback_preserved: Option<bool>,
    pub operation_failure: Option<Box<FailureSnapshot>>,
    pub final_unwind: Option<Box<FailureSnapshot>>,
    pub cleanup_errors: Vec<String>,
}

fn no_report(error: RunError) -> RunFailure {
    RunFailure {
        error,
        report: None,
        rollback_preserved: None,
        operation_failure: None,
        final_unwind: None,
        cleanup_errors: Vec::new(),
    }
}

fn execute_with_failure(
    source: &str,
    args: &[String],
    check_scalar: bool,
    host_fail_reserve: Option<usize>,
    observe_rollback: bool,
) -> Result<(RunResult, Option<MemoryReport>), Vec<RunFailure>> {
    let module = parse_module(source).map_err(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|d| {
                no_report(RunError {
                    code: d.code,
                    message: d.message,
                    span: Some(d.span),
                })
            })
            .collect::<Vec<_>>()
    })?;
    if let Some(function) = module.functions.iter().find(|function| {
        function
            .foreign
            .as_ref()
            .is_some_and(|binding| binding.route == "native")
    }) {
        return Err(vec![no_report(RunError {
            code: "CGT017",
            message: "native C calls require an explicit schema 2 package and pinned binding lock"
                .into(),
            span: Some(function.span),
        })]);
    }
    let is_memory = module.statements.iter().any(eval::stmt_has_memory)
        || module
            .functions
            .iter()
            .any(|f| f.body.iter().any(eval::stmt_has_memory));
    let requires_static_gate = module.functions.iter().any(|f| {
        !f.requires.is_empty()
            || !f.ensures.is_empty()
            || f.effects.is_some()
            || f.capabilities.is_some()
            || f.foreign.is_some()
    });
    let has_for = module.statements.iter().any(eval::stmt_has_for)
        || module
            .functions
            .iter()
            .any(|f| f.body.iter().any(eval::stmt_has_for));
    if is_memory
        || check_scalar
        || requires_static_gate
        || has_for
        || !module.records.is_empty()
        || cgx2_syntax::s2a_feature_span(&module).is_some()
    {
        cgx2_sema::check_module(&module).map_err(|diagnostics| {
            diagnostics
                .into_iter()
                .map(|d| {
                    no_report(RunError {
                        code: d.code,
                        message: d.message,
                        span: Some(d.span),
                    })
                })
                .collect::<Vec<_>>()
        })?;
    }
    if is_memory {
        let report = memory::run_module_with_failure(&module, host_fail_reserve, observe_rollback)
            .map_err(|failure| {
                vec![RunFailure {
                    error: failure.error,
                    report: failure.report.map(|report| *report),
                    rollback_preserved: failure.rollback_preserved,
                    operation_failure: failure.operation_failure,
                    final_unwind: failure.final_unwind,
                    cleanup_errors: failure.cleanup_errors,
                }]
            })?;
        Ok((RunResult { value: Value::Unit }, Some(report)))
    } else {
        Ok((
            eval::run_module(&module, args).map_err(|error| vec![no_report(error)])?,
            None,
        ))
    }
}

pub fn run_checked_source_with_failure(
    source: &str,
    args: &[String],
) -> Result<(RunResult, Option<MemoryReport>), Vec<RunFailure>> {
    execute_with_failure(source, args, true, None, true)
}

/// Explicit test-only host reserve fault. The one-based index matches
/// `cgx2c build --test-fail-reserve N` for source/native parity fixtures.
#[cfg(feature = "failure-injection-test")]
pub fn run_checked_source_with_failure_at_reserve(
    source: &str,
    args: &[String],
    index: usize,
) -> Result<(RunResult, Option<MemoryReport>), Vec<RunFailure>> {
    execute_with_failure(source, args, true, Some(index), true)
}

fn execute(
    source: &str,
    args: &[String],
    check_scalar: bool,
) -> Result<(RunResult, Option<MemoryReport>), Vec<RunError>> {
    execute_with_failure(source, args, check_scalar, None, false)
        .map_err(|failures| failures.into_iter().map(|failure| failure.error).collect())
}

pub fn run_source(source: &str, args: &[String]) -> Result<RunResult, RunError> {
    execute(source, args, false)
        .map(|(result, _)| result)
        .map_err(|mut errors| errors.remove(0))
}

pub fn run_source_with_report(
    source: &str,
    args: &[String],
) -> Result<(RunResult, Option<MemoryReport>), RunError> {
    execute(source, args, false).map_err(|mut errors| errors.remove(0))
}

pub fn run_checked_source(
    source: &str,
    args: &[String],
) -> Result<(RunResult, Option<MemoryReport>), Vec<RunError>> {
    execute(source, args, true)
}
