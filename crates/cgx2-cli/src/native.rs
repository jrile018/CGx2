use std::collections::BTreeMap;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use cgx2_bind::{checked_artifact, checked_lock, BindingLock};
use cgx2_ffi_native::{Declaration, Function as NativeFunction, Library, LibraryDeclaration};
use cgx2_package::{load_checked_package, CheckedPackage};
use cgx2_syntax::{ForeignBinding, Span};

use crate::native_eval;

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
}

fn prepare(root: &Path) -> Result<Prepared, Vec<NativeDiagnostic>> {
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
    for (name, declaration) in &native_declarations {
        let adapter = bind_one(&package, &lock, declaration, &source)?;
        adapters.insert(name.clone(), adapter);
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
    })
}

fn bind_one(
    package: &CheckedPackage,
    lock: &BindingLock,
    source_binding: &ForeignBinding,
    source: &Path,
) -> Result<NativeFunction, Vec<NativeDiagnostic>> {
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
    let file = checked_artifact(package.root(), binding)
        .map_err(|message| fail("CGN006", format!("binding {binding_id}: {message}")))?;
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

/// Validate a pinned native package, including loaded library metadata, without calling its scalar export.
pub fn check_native_package(root: &Path) -> Result<(), Vec<NativeDiagnostic>> {
    prepare(root).map(|_| ())
}

/// Execute the checked entry through the fixed C adapter, without starting Python.
pub fn run_native_package_i64(root: &Path, args: &[String]) -> Result<i64, Vec<NativeDiagnostic>> {
    let prepared = prepare(root)?;
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
