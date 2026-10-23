mod emit;
mod error;
mod names;
mod package;

use cgx2_sema::TypedModule;
use cgx2_syntax::Module;

pub use error::EmitDiagnostic;
pub use package::{emit_package_python, write_package_python, PackageEmitDiagnostic};

pub fn emit_python(module: &Module, typed: &TypedModule) -> Result<String, Vec<EmitDiagnostic>> {
    emit::emit(module, typed)
}
