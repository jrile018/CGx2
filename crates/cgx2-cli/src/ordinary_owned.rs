use crate::{RunError, RunResult, Value};
use cgx2_sema::ordinary_owned::{compile_source_v2 as compile_source, Scalar};

pub fn validate_owned_source(source: &str) -> Result<(), Vec<crate::CheckDiagnostic>> {
    if cgx2_syntax::parse_module(source).is_ok_and(|m| {
        !cgx2_sema::default_managed::hybrid::is_hybrid(&m)
            && cgx2_sema::default_managed::profile(&m)
    }) {
        return cgx2_sema::default_managed::compile_source(source)
            .map(|_| ())
            .or_else(|ds| {
                if ds.iter().all(|d| d.code == "CGRS003") {
                    Ok(())
                } else {
                    Err(ds)
                }
            })
            .map_err(|ds| {
                ds.into_iter()
                    .map(|d| crate::CheckDiagnostic {
                        code: d.code.into(),
                        message: d.message,
                        span: d.span,
                    })
                    .collect()
            });
    }
    if cgx2_syntax::parse_module(source)
        .is_ok_and(|m| cgx2_sema::default_managed::hybrid::is_hybrid(&m))
    {
        return cgx2_sema::default_managed::hybrid::compile_source(source)
            .map(|_| ())
            .map_err(|ds| {
                ds.into_iter()
                    .map(|d| crate::CheckDiagnostic {
                        code: d.code.into(),
                        message: d.message,
                        span: d.span,
                    })
                    .collect()
            });
    }
    compile_source(source).map(|_| ()).map_err(|ds| {
        ds.into_iter()
            .map(|d| crate::CheckDiagnostic {
                code: d.code.into(),
                message: d.message,
                span: d.span,
            })
            .collect()
    })
}
pub(super) fn run(
    source: &str,
    host_fail_reserve: Option<usize>,
) -> Result<RunResult, Vec<RunError>> {
    let program = compile_source(source).map_err(|ds| {
        ds.into_iter()
            .map(|d| RunError {
                code: d.code,
                message: d.message,
                span: Some(d.span),
            })
            .collect::<Vec<_>>()
    })?;
    #[cfg(feature = "failure-injection-test")]
    let execution = if let Some(index) = host_fail_reserve {
        match cgx2_runtime::Session::try_new_with_host_failure_at_reserve(index) {
            Ok(session) => program.execute_with_session(session),
            Err(error) => Ok(cgx2_sema::ordinary_owned::ExecutionV2 {
                result: Err(cgx2_sema::ordinary_owned::Failure {
                    code: "CGR080",
                    message: format!("session allocation failed: {error:?}"),
                    span: program.model().functions[program.model().entry].span,
                }),
                evidence: Default::default(),
            }),
        }
    } else {
        program.execute()
    };
    #[cfg(not(feature = "failure-injection-test"))]
    let execution = {
        let _ = host_fail_reserve;
        program.execute()
    };
    let execution = execution.map_err(|ds| {
        ds.into_iter()
            .map(|d| RunError {
                code: d.code,
                message: d.message,
                span: Some(d.span),
            })
            .collect::<Vec<_>>()
    })?;
    if std::env::var_os("CGX2_OWNED_EVIDENCE").is_some() {
        eprintln!(
            "CGX2_OWNED_EVIDENCE {}",
            if program.schema_version() == 1 {
                execution.evidence.json()
            } else {
                execution.evidence.json_v2()
            }
        );
    }
    execution
        .result
        .map(|v| RunResult {
            value: copied_value(v),
        })
        .map_err(|e| {
            vec![RunError {
                code: e.code,
                message: e.message,
                span: Some(cgx2_syntax::Span {
                    start: e.span.start,
                    end: e.span.end,
                }),
            }]
        })
}

pub(super) fn copied_value(v: Scalar) -> Value {
    match v {
        Scalar::I64(v) => Value::I64(v),
        Scalar::Bool(v) => Value::Bool(v),
        Scalar::Unit => Value::Unit,
        Scalar::Record { name, fields } => Value::Record {
            name,
            fields: fields
                .into_iter()
                .map(|(n, v)| (n, copied_value(v)))
                .collect(),
        },
    }
}
