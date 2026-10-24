use crate::{RunError, RunResult, Value};

pub(crate) fn run_hybrid(
    program: &cgx2_sema::default_managed::hybrid::HybridProgramV1,
    fault: Option<usize>,
) -> Result<(RunResult, cgx2_runtime::MemoryReport), Vec<RunError>> {
    program.authenticate().map_err(|ds| {
        ds.into_iter()
            .map(|d| RunError {
                code: d.code,
                message: d.message,
                span: Some(d.span),
            })
            .collect::<Vec<_>>()
    })?;
    #[cfg(feature = "failure-injection-test")]
    let execution = if let Some(index) = fault {
        let session =
            cgx2_runtime::Session::try_new_with_host_failure_at_reserve(index).map_err(|e| {
                vec![RunError {
                    code: "CGR080",
                    message: format!("session allocation failed: {e:?}"),
                    span: Some(program.entry_span()),
                }]
            })?;
        program.execute_with_session(session)
    } else {
        program.execute()
    };
    #[cfg(not(feature = "failure-injection-test"))]
    let execution = {
        let _ = fault;
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
        eprintln!("CGX2_OWNED_EVIDENCE {}", execution.owned.json_v2());
    }
    if std::env::var_os("CGX2_MANAGED_EVIDENCE").is_some() {
        eprintln!(
            "CGX2_MANAGED_EVIDENCE {}",
            execution.managed.json(&execution.memory)
        );
    }
    execution
        .result
        .map(|v| {
            (
                RunResult {
                    value: crate::ordinary_owned::copied_value(v),
                },
                execution.memory,
            )
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
use cgx2_runtime::default_managed::Snapshot;
use cgx2_sema::{default_managed::ManagedProgramV1, Type};
pub(crate) fn run(
    program: &ManagedProgramV1,
    args: &[String],
    fault: Option<usize>,
) -> Result<(RunResult, cgx2_runtime::MemoryReport), Vec<RunError>> {
    let failure = |ds: Vec<cgx2_sema::TypeDiagnostic>| {
        ds.into_iter()
            .map(|d| RunError {
                code: d.code,
                message: d.message,
                span: Some(d.span),
            })
            .collect::<Vec<_>>()
    };
    program.authenticate().map_err(failure)?;
    #[cfg(feature = "failure-injection-test")]
    let execution = if let Some(index) = fault {
        let session =
            cgx2_runtime::Session::try_new_with_host_failure_at_reserve(index).map_err(|e| {
                vec![RunError {
                    code: "CGR090",
                    message: format!("managed session entry failed: {e:?}"),
                    span: Some(program.entry_span()),
                }]
            })?;
        program.execute_with_session(args, session)
    } else {
        program.execute(args)
    };
    #[cfg(not(feature = "failure-injection-test"))]
    let execution = {
        let _ = fault;
        program.execute(args)
    };
    let execution = execution.map_err(failure)?;
    if std::env::var_os("CGX2_MANAGED_EVIDENCE").is_some() {
        eprintln!(
            "CGX2_MANAGED_EVIDENCE {}",
            execution.evidence.json(&execution.memory)
        );
    }
    let result_ty = program
        .program()
        .functions
        .get(program.program().entry)
        .map_or(&Type::Unit, |f| &program.types()[f.result]);
    execution
        .result
        .map(|s| {
            (
                RunResult {
                    value: copy(s, result_ty, program),
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
fn find<'a>(name: &str, p: &'a ManagedProgramV1) -> &'a Type {
    p.types()
        .iter()
        .find(|t| cgx2_sema::type_key(t) == name)
        .unwrap_or(&Type::Unit)
}
pub(crate) fn copy(s: Snapshot, t: &Type, p: &ManagedProgramV1) -> Value {
    match s {
        Snapshot::Integer(v) => Value::I64(v),
        Snapshot::Boolean(v) => Value::Bool(v),
        Snapshot::Text(v) => Value::String(v),
        Snapshot::Unit => Value::Unit,
        Snapshot::Sequence(array, values) => {
            let inner = match t {
                Type::List(t) | Type::Array(t) => t.as_ref(),
                _ => &Type::Unit,
            };
            Value::Sequence {
                ty: if array {
                    Type::Array(Box::new(inner.clone()))
                } else {
                    Type::List(Box::new(inner.clone()))
                },
                values: values.into_iter().map(|v| copy(v, inner, p)).collect(),
            }
        }
        Snapshot::Record(name, fields) => {
            let ty = find(&name, p);
            let i = p.types().iter().position(|t| t == ty).unwrap_or(0);
            let members = match &p.program().layouts[i].shape {
                cgx2_runtime::default_managed::model::Shape::KRecord(fs) => fs.as_slice(),
                _ => &[],
            };
            Value::Record {
                name,
                fields: fields
                    .into_iter()
                    .enumerate()
                    .map(|(i, (n, v))| {
                        let t = members.get(i).map_or(&Type::Unit, |(_, t)| &p.types()[*t]);
                        (n, copy(v, t, p))
                    })
                    .collect(),
            }
        }
        Snapshot::Variant(name, case, payloads) => {
            let ty = find(&name, p).clone();
            let i = p.types().iter().position(|t| *t == ty).unwrap_or(0);
            let cases = match &p.program().layouts[i].shape {
                cgx2_runtime::default_managed::model::Shape::KVariant(cs) => cs.as_slice(),
                _ => &[],
            };
            let case_index = if matches!(
                p.program().layouts[i].shape,
                cgx2_runtime::default_managed::model::Shape::KError
            ) {
                [
                    "BudgetExceeded",
                    "RegionLimit",
                    "HostAllocation",
                    "InvalidRegion",
                    "ShapeLimit",
                ]
                .iter()
                .position(|n| *n == case)
                .unwrap_or(0)
            } else {
                cases
                    .iter()
                    .position(|(n, _)| p.program().strings[*n] == case)
                    .unwrap_or(0)
            };
            let fields = cases
                .get(case_index)
                .map_or(&[][..], |(_, fs)| fs.as_slice());
            Value::Variant {
                ty,
                case_index,
                case_name: case,
                payloads: payloads
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| copy(v, fields.get(i).map_or(&Type::Unit, |t| &p.types()[*t]), p))
                    .collect(),
            }
        }
    }
}

#[cfg(all(test, feature = "failure-injection-test"))]
mod entry_tests {
    #[test]
    fn managed_and_bridge_entry_failures_retain_original_utf8_spans() {
        for source in [
            "# café🙂\nfn main() -> i64:\n let xs = [42]\n length(xs)\n",
            "# café🙂\nrecord R:\n    value: i64\nfn heap() -> i64:\n let xs = [42]\n length(xs)\nfn main() -> i64:\n let old = owned(R(value: 7))\n heap() + old.value\n",
        ] {
            let module = cgx2_syntax::parse_module(source).unwrap();
            let expected = module.functions.iter().find(|f| f.name == "main").unwrap().span;
            let errors = crate::run_checked_source_with_failure_at_reserve(source, &[], 1).unwrap_err();
            assert_eq!(errors[0].error.span, Some(expected), "{:?}", errors[0].error);
            assert!(matches!(errors[0].error.code, "CGR090" | "CGR080"));
            assert_eq!(&source[expected.start..expected.start+2], "fn");
        }
    }
}
