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
pub use native::{check_native_package, NativeDiagnostic};
use std::path::Path;

pub fn run_native_package(
    root: &Path,
    args: &[String],
) -> Result<RunResult, Vec<NativeDiagnostic>> {
    native::run_native_package_value(root, args).map(|value| RunResult {
        value: match value {
            native::NativeValue::I64(value) => Value::I64(value),
            native::NativeValue::Tensor(value) => Value::String(value),
        },
    })
}

use cgx2_syntax::parse_module;

fn execute(
    source: &str,
    args: &[String],
    check_scalar: bool,
) -> Result<(RunResult, Option<MemoryReport>), Vec<RunError>> {
    let module = parse_module(source).map_err(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|d| RunError {
                code: d.code,
                message: d.message,
                span: Some(d.span),
            })
            .collect::<Vec<_>>()
    })?;
    if let Some(function) = module.functions.iter().find(|function| {
        function
            .foreign
            .as_ref()
            .is_some_and(|binding| binding.route == "native")
    }) {
        return Err(vec![RunError {
            code: "CGT017",
            message: "native C calls require an explicit schema 2 package and pinned binding lock"
                .into(),
            span: Some(function.span),
        }]);
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
    if is_memory || check_scalar || requires_static_gate {
        cgx2_sema::check_module(&module).map_err(|diagnostics| {
            diagnostics
                .into_iter()
                .map(|d| RunError {
                    code: d.code,
                    message: d.message,
                    span: Some(d.span),
                })
                .collect::<Vec<_>>()
        })?;
    }
    if is_memory {
        let report = memory::run_module(&module).map_err(|e| vec![e])?;
        Ok((RunResult { value: Value::Unit }, Some(report)))
    } else {
        Ok((eval::run_module(&module, args).map_err(|e| vec![e])?, None))
    }
}

pub fn run_source(source: &str, args: &[String]) -> Result<RunResult, RunError> {
    execute(source, args, false)
        .map(|(result, _)| result)
        .map_err(|mut errors| errors.remove(0))
}

/// Check memory scripts before execution and return their actual CGx2 memory snapshot.
/// Scalar scripts retain the low-level `run_source` runtime boundary.
pub fn run_source_with_report(
    source: &str,
    args: &[String],
) -> Result<(RunResult, Option<MemoryReport>), RunError> {
    execute(source, args, false).map_err(|mut errors| errors.remove(0))
}

/// Parse and statically check before running, for user-facing commands.
pub fn run_checked_source(
    source: &str,
    args: &[String],
) -> Result<(RunResult, Option<MemoryReport>), Vec<RunError>> {
    execute(source, args, true)
}
