mod emit;
mod error;
mod names;

use cgx2_sema::TypedModule;
use cgx2_syntax::Module;

pub use error::EmitDiagnostic;

pub fn emit_python(module: &Module, typed: &TypedModule) -> Result<String, Vec<EmitDiagnostic>> {
    emit::emit(module, typed)
}
