mod data;
mod emit;
mod error;
mod names;
mod package;

use cgx2_sema::TypedModule;
use cgx2_syntax::Module;

pub use error::EmitDiagnostic;
pub use package::{emit_package_python, write_package_python, PackageEmitDiagnostic};

pub fn emit_python(module: &Module, typed: &TypedModule) -> Result<String, Vec<EmitDiagnostic>> {
    if let Some(span) = cgx2_syntax::ordinary_memory_span(module) {
        return Err(vec![EmitDiagnostic {
            code: "CGE006",
            message: "Python does not support ordinary memory ownership, regions or entry recovery"
                .into(),
            span,
        }]);
    }

    if cgx2_sema::default_managed::resolved_explicit(module, typed) {
        cgx2_sema::default_managed::compile_ast(module).map_err(|ds| {
            ds.into_iter()
                .map(|d| EmitDiagnostic {
                    code: d.code,
                    message: d.message,
                    span: d.span,
                })
                .collect::<Vec<_>>()
        })?;
        return Err(vec![EmitDiagnostic {
            code: "CGE006",
            message: "explicit managed observables are unsupported by Python".into(),
            span: module.span,
        }]);
    }
    if let Some(span) = cgx2_syntax::ordinary_owned_span(module) {
        return Err(vec![EmitDiagnostic::new(
            "CGE006",
            "ordinary owned storage and loans are outside the Python target",
            span,
        )]);
    }
    if typed.finite_values
        || cgx2_syntax::s2a_feature_span(module).is_some()
        || cgx2_sema::foundation_profile(module)
    {
        data::emit(module, typed)
    } else {
        emit::emit(module, typed)
    }
}
