//! Additive lock schema 3 profiles; historical schemas/layouts stay frozen.
use crate::Symbol;

fn export(name: &str, parameters: &[&str], result: &str, owned: bool) -> Symbol {
    Symbol {
        name: name.into(),
        parameters: parameters.iter().map(|s| (*s).into()).collect(),
        result: result.into(),
        ownership: if owned { "owned" } else { "none" }.into(),
        owner: None,
        release: owned.then(|| "cgx2_sdk_release".into()),
        error: if result == "i32" { "status" } else { "none" }.into(),
        effects: vec!["foreign".into()],
        capabilities: vec!["native".into()],
    }
}
pub fn native_facade_symbols() -> Vec<Symbol> {
    vec![
        export("cgx2_library_identity", &[], "cstr_ptr", false),
        export("cgx2_library_version", &[], "cstr_ptr", false),
        export("cgx2_abi_version", &[], "u32", false),
        export("cgx2_sdk_api_version", &[], "u32", false),
        export(
            "cgx2_sdk_copy_bytes",
            &["u8_const_ptr", "u64", "u8_mut_ptr", "u64", "u64_out_ptr"],
            "i32",
            false,
        ),
        export(
            "cgx2_sdk_copy_utf8",
            &["u8_const_ptr", "u64", "u8_mut_ptr", "u64", "u64_out_ptr"],
            "i32",
            false,
        ),
        export("cgx2_sdk_create", &["i64", "handle_out_ptr"], "i32", true),
        export(
            "cgx2_sdk_read",
            &["handle_const_ptr", "i64_out_ptr"],
            "i32",
            false,
        ),
        export("cgx2_sdk_release", &["handle_mut_ptr"], "i32", false),
    ]
}

pub fn libtorch_inference_facade_symbols() -> Vec<Symbol> {
    let mut symbols = crate::libtorch_f32_matrix_facade_symbols();
    symbols.push(export(
        "cgx2_torch_inference_api_version",
        &[],
        "u32",
        false,
    ));
    for (name, parameters) in [
        (
            "cgx2_torch_matrix_add_f32",
            vec!["handle_const_ptr", "handle_const_ptr", "handle_out_ptr"],
        ),
        (
            "cgx2_torch_matrix_relu_f32",
            vec!["handle_const_ptr", "handle_out_ptr"],
        ),
    ] {
        let mut symbol = export(name, &parameters, "i32", true);
        symbol.release = Some("cgx2_torch_release".into());
        symbols.push(symbol);
    }
    symbols
}

/// Shared portable declaration for the copied subset of the physical SDK.
pub fn copied_declaration(
    binding: &crate::Binding,
    package: &str,
    utf8: bool,
) -> cgx2_ffi_core::Declaration {
    use cgx2_ffi_core::{Declaration, Ownership, ValueType};
    Declaration {
        schema_version: 2,
        package_id: package.into(),
        library_id: binding.library.clone(),
        library_version: binding.version.clone(),
        artifact_sha256: binding.sha256.clone(),
        target_os: binding.os.clone(),
        target_arch: binding.arch.clone(),
        abi: "c-sdk-v2".into(),
        calling_convention: "c".into(),
        symbol: if utf8 {
            "cgx2_sdk_copy_utf8"
        } else {
            "cgx2_sdk_copy_bytes"
        }
        .into(),
        parameters: vec![ValueType::Bytes],
        result: if utf8 {
            ValueType::Utf8
        } else {
            ValueType::Bytes
        },
        ownership: Ownership::Copied,
        error_convention: "i32_status_zero_ok".into(),
        thread_rule: "calling_thread".into(),
        callbacks: false,
        effects: vec!["foreign".into()],
        capabilities: vec!["native".into()],
    }
}
