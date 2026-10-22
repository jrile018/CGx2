use std::collections::BTreeMap;
use std::fs::File;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use cgx2_bind::{checked_artifact, checked_lock, BindingLock};
use cgx2_ffi_native::{Declaration, Function as NativeFunction, Library, LibraryDeclaration};
use cgx2_package::{load_checked_package, CheckedPackage};
use cgx2_syntax::{ForeignBinding, Span};

use crate::native_eval;

#[path = "tensor_native.rs"]
mod tensor_native;

pub enum NativeValue {
    I64(i64),
    Tensor(String),
}

impl std::fmt::Display for NativeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::I64(value) => write!(f, "{value}"),
            Self::Tensor(value) => f.write_str(value),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeDiagnostic {
    pub code: String,
    pub message: String,
    pub file: PathBuf,
    pub span: Option<Span>,
}

fn diagnostic(
    code: &str,
    message: impl Into<String>,
    file: &Path,
    span: Option<Span>,
) -> NativeDiagnostic {
    NativeDiagnostic {
        code: code.into(),
        message: message.into(),
        file: file.into(),
        span,
    }
}

struct Prepared {
    package: CheckedPackage,
    entry: usize,
    adapters: BTreeMap<String, NativeFunction>,
    // Keep every verified descriptor open until its adapter is dropped. dlopen caches by
    // pathname, so reusing /proc/self/fd/N for another artifact can load the wrong code.
    _artifact_files: Vec<File>,
}

fn prepare(root: &Path, load_code: bool) -> Result<Prepared, Vec<NativeDiagnostic>> {
    let package = load_checked_package(root).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| diagnostic(error.code, error.message, &error.file, error.span))
            .collect::<Vec<_>>()
    })?;
    let entry = package
        .modules()
        .iter()
        .position(|module| module.source() == package.index().entry)
        .ok_or_else(|| {
            vec![diagnostic(
                "CGN001",
                "package entry source is missing",
                root,
                None,
            )]
        })?;
    let source = package.modules()[entry].source().to_path_buf();
    let native_declarations: Vec<_> = package.modules()[entry]
        .syntax()
        .functions
        .iter()
        .filter_map(|function| {
            function
                .foreign
                .as_ref()
                .filter(|binding| binding.route == "native")
                .map(|binding| (function.name.clone(), binding.clone()))
        })
        .collect();
    if native_declarations.is_empty() {
        return Err(vec![diagnostic(
            "CGN001",
            "package entry has no native C declaration",
            &source,
            None,
        )]);
    }
    if package.modules().len() != 1 || !package.modules()[entry].syntax().imports.is_empty() {
        return Err(vec![diagnostic(
            "CGN001",
            "native C execution currently requires a single source package without imports",
            &source,
            None,
        )]);
    }
    native_eval::validate(package.modules()[entry].syntax())
        .map_err(|error| vec![diagnostic(error.code, error.message, &source, error.span)])?;
    let lock = checked_lock(package.root()).map_err(|report| {
        report
            .diagnostics
            .into_iter()
            .map(|item| {
                let span = native_declarations
                    .iter()
                    .find(|(_, declaration)| {
                        declaration.target.segments.first() == Some(&item.binding)
                    })
                    .map(|(_, declaration)| declaration.span);
                diagnostic(
                    item.code,
                    format!("binding {}: {}", item.binding, item.message),
                    &source,
                    span,
                )
            })
            .collect::<Vec<_>>()
    })?;
    let mut adapters = BTreeMap::new();
    let mut artifact_files = Vec::new();
    for (name, declaration) in &native_declarations {
        let binding = validated_binding(&lock, declaration, &source)?;
        let file = checked_artifact(package.root(), binding).map_err(|message| {
            vec![diagnostic(
                "CGN006",
                format!("binding {}: {message}", binding.id),
                &source,
                Some(declaration.span),
            )]
        })?;
        if load_code {
            let adapter = bind_one(binding, &lock, declaration, &source, &file)?;
            adapters.insert(name.clone(), adapter);
            artifact_files.push(file);
        }
    }
    package.verify_unchanged().map_err(|errors| {
        errors
            .into_iter()
            .map(|error| diagnostic(error.code, error.message, &error.file, error.span))
            .collect::<Vec<_>>()
    })?;
    Ok(Prepared {
        package,
        entry,
        adapters,
        _artifact_files: artifact_files,
    })
}

fn validated_binding<'a>(
    lock: &'a BindingLock,
    source_binding: &ForeignBinding,
    source: &Path,
) -> Result<&'a cgx2_bind::Binding, Vec<NativeDiagnostic>> {
    let binding_id = &source_binding.target.segments[0];
    let symbol_name = &source_binding.target.segments[1];
    let fail = |code: &str, message: String| {
        vec![diagnostic(code, message, source, Some(source_binding.span))]
    };
    let binding = lock
        .bindings
        .iter()
        .find(|binding| &binding.id == binding_id)
        .ok_or_else(|| fail("CGN002", format!("missing native binding ID {binding_id}")))?;
    if binding.route != "native-c" || binding.abi != "c" || binding.layout != "status-i64-out-v1" {
        return Err(fail("CGN003", format!("binding {binding_id} does not declare the fixed native C status-i64-out-v1 profile")));
    }
    let expected_runtime = format!("{}@{}", binding.library, binding.version);
    if source_binding.runtime != expected_runtime {
        return Err(fail(
            "CGN004",
            format!(
                "source runtime {} differs from pinned {expected_runtime}",
                source_binding.runtime
            ),
        ));
    }
    if !binding
        .symbols
        .iter()
        .any(|symbol| &symbol.name == symbol_name)
    {
        return Err(fail(
            "CGN005",
            format!("symbol {symbol_name} is absent from binding {binding_id}"),
        ));
    }
    Ok(binding)
}

fn bind_one(
    binding: &cgx2_bind::Binding,
    lock: &BindingLock,
    source_binding: &ForeignBinding,
    source: &Path,
    file: &File,
) -> Result<NativeFunction, Vec<NativeDiagnostic>> {
    let symbol_name = &source_binding.target.segments[1];
    let fail = |code: &str, message: String| {
        vec![diagnostic(code, message, source, Some(source_binding.span))]
    };
    let fd_path = PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()));
    let mut declaration = LibraryDeclaration::new(&binding.library, &binding.version);
    declaration.package_id = &lock.package;
    declaration.target_os = &binding.os;
    declaration.target_arch = &binding.arch;
    // SAFETY: This is an explicitly unsafe source declaration. Preflight pins artifact bytes,
    // verifies exports and profile assertions; the caller remains responsible for the physical
    // C signatures and behavior, which neither the lock nor dlsym can prove.
    let library = unsafe { Library::open(&fd_path, &declaration) }.map_err(|error| {
        fail(
            "CGN007",
            format!("native library identity/version/ABI check failed: {error:?}"),
        )
    })?;
    // SAFETY: The same unsafe contract applies to the declared fixed-shape export.
    let adapter = unsafe { library.bind(&Declaration::scalar(symbol_name)) }
        .map_err(|error| fail("CGN008", format!("native symbol binding failed: {error:?}")))?;
    Ok(adapter)
}

/// Statically validate a pinned native package without loading or executing foreign code.
pub fn check_native_package(root: &Path) -> Result<(), Vec<NativeDiagnostic>> {
    if tensor_native::maybe_check(root)?.is_some() {
        return Ok(());
    }
    prepare(root, false).map(|_| ())
}

/// Execute the checked entry through the fixed C adapter, without starting Python.
pub fn run_native_package_i64(root: &Path, args: &[String]) -> Result<i64, Vec<NativeDiagnostic>> {
    let prepared = prepare(root, true)?;
    let entry = &prepared.package.modules()[prepared.entry];
    native_eval::run(entry.syntax(), args, &prepared.adapters).map_err(|error| {
        vec![diagnostic(
            error.code,
            error.message,
            entry.source(),
            error.span,
        )]
    })
}

pub fn run_native_package_value(
    root: &Path,
    args: &[String],
) -> Result<NativeValue, Vec<NativeDiagnostic>> {
    if let Some(value) = tensor_native::maybe_run(root, args)? {
        return Ok(NativeValue::Tensor(value));
    }
    run_native_package_i64(root, args).map(NativeValue::I64)
}
