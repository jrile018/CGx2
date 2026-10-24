use crate::{RunError, RunResult, Value};
use cgx2_runtime::default_managed::{model::Shape, Snapshot};
use cgx2_sema::{ordinary_memory::MemoryProgramV1, Type};
fn errors(ds: Vec<cgx2_sema::TypeDiagnostic>) -> Vec<RunError> {
    ds.into_iter()
        .map(|d| RunError {
            code: d.code,
            message: d.message,
            span: Some(d.span),
        })
        .collect()
}
pub(crate) fn run(
    program: &MemoryProgramV1,
    args: &[String],
    fault: Option<usize>,
) -> Result<(RunResult, Option<cgx2_runtime::MemoryReport>), Vec<RunError>> {
    program.authenticate().map_err(errors)?;
    #[cfg(feature = "failure-injection-test")]
    let execution = if let Some(index) = fault {
        cgx2_runtime::ordinary_memory::run_with_host_failure_v1(program.runtime(), args, index)
    } else {
        program.execute(args).map_err(errors)?
    };
    #[cfg(not(feature = "failure-injection-test"))]
    let execution = {
        let _ = fault;
        program.execute(args).map_err(errors)?
    };
    if std::env::var_os("CGX2_MEMORY_EVIDENCE").is_some() {
        match (&execution.evidence, &execution.memory) {
            (Some(e), Some(m)) => eprintln!("CGX2_MEMORY_EVIDENCE {}", e.json(m)),
            _ => eprintln!("CGX2_MEMORY_EVIDENCE {{\"session_entered\":false,\"metrics\":null}}"),
        }
    }
    execution
        .result
        .map(|snapshot| {
            let p = &program.runtime().program;
            let ty = p
                .functions
                .get(p.entry)
                .and_then(|f| program.types().get(f.result))
                .unwrap_or(&Type::Unit);
            (
                RunResult {
                    value: copy(snapshot, ty, program),
                },
                execution.memory,
            )
        })
        .map_err(|e| {
            vec![RunError {
                code: e.code,
                message: e.message,
                span: Some(cgx2_syntax::Span {
                    start: e.loc.start,
                    end: e.loc.end,
                }),
            }]
        })
}
fn find<'a>(program: &'a MemoryProgramV1, name: &str) -> &'a Type {
    program
        .types()
        .iter()
        .find(|t| t.to_string() == name)
        .unwrap_or(&Type::Unit)
}
pub(crate) fn copy(v: Snapshot, ty: &Type, program: &MemoryProgramV1) -> Value {
    match v {
        Snapshot::Integer(n) => Value::I64(n),
        Snapshot::Boolean(b) => Value::Bool(b),
        Snapshot::Text(s) => Value::String(s),
        Snapshot::Unit => Value::Unit,
        Snapshot::Sequence(array, values) => {
            let inner = match ty {
                Type::Array(t) | Type::List(t) => t.as_ref(),
                _ => &Type::Unit,
            };
            Value::Sequence {
                ty: if array {
                    Type::Array(Box::new(inner.clone()))
                } else {
                    Type::List(Box::new(inner.clone()))
                },
                values: values
                    .into_iter()
                    .map(|v| copy(v, inner, program))
                    .collect(),
            }
        }
        Snapshot::Record(name, values) => {
            let index = program.types().iter().position(|t| t == ty).unwrap_or(0);
            let fields =
                if let Shape::KRecord(fields) = &program.runtime().program.layouts[index].shape {
                    fields.as_slice()
                } else {
                    &[]
                };
            Value::Record {
                name,
                fields: values
                    .into_iter()
                    .zip(fields)
                    .map(|((name, v), (_, t))| (name, copy(v, &program.types()[*t], program)))
                    .collect(),
            }
        }
        Snapshot::Variant(name, case, values) => {
            let ty = find(program, &name);
            let index = program.types().iter().position(|t| t == ty).unwrap_or(0);
            let cases =
                if let Shape::KVariant(cases) = &program.runtime().program.layouts[index].shape {
                    cases.as_slice()
                } else {
                    &[]
                };
            let case_index = cases
                .iter()
                .position(|(n, _)| program.runtime().program.strings[*n] == case)
                .unwrap_or(0);
            let payloads = cases
                .get(case_index)
                .map(|(_, ts)| ts.as_slice())
                .unwrap_or(&[]);
            Value::Variant {
                ty: ty.clone(),
                case_index,
                case_name: case,
                payloads: values
                    .into_iter()
                    .zip(payloads)
                    .map(|(v, t)| copy(v, &program.types()[*t], program))
                    .collect(),
            }
        }
    }
}
