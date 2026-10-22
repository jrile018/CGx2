//! One lexical `.cg` tensor call, using the frozen direct LibTorch facade.
use std::fs::File;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use cgx2_bind::{checked_artifact, checked_lock};
use cgx2_libtorch::Torch;
use cgx2_package::{load_checked_package, CheckedPackage};
use cgx2_sema::{tensor_profile, TensorProfile};

use super::{diagnostic, NativeDiagnostic};

struct Prepared {
    package: CheckedPackage,
    entry: usize,
    profile: TensorProfile,
    artifact: File,
}

fn prepare(root: &Path) -> Result<Option<Prepared>, Vec<NativeDiagnostic>> {
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
    let source = package.modules()[entry].source();
    let profile = tensor_profile(package.modules()[entry].syntax()).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| diagnostic(error.code, error.message, source, Some(error.span)))
            .collect::<Vec<_>>()
    })?;
    let Some(profile) = profile else {
        return Ok(None);
    };
    if package.modules().len() != 1 {
        return Err(vec![diagnostic(
            "CGN001",
            "tensor profile requires a single source package",
            source,
            Some(profile.declaration_span),
        )]);
    }
    let lock = checked_lock(package.root()).map_err(|report| {
        report
            .diagnostics
            .into_iter()
            .map(|item| {
                diagnostic(
                    item.code,
                    format!("binding {}: {}", item.binding, item.message),
                    source,
                    Some(profile.declaration_span),
                )
            })
            .collect::<Vec<_>>()
    })?;
    let binding = lock
        .bindings
        .iter()
        .find(|binding| binding.id == profile.binding_id)
        .ok_or_else(|| {
            vec![diagnostic(
                "CGN002",
                format!("missing tensor binding ID {}", profile.binding_id),
                source,
                Some(profile.declaration_span),
            )]
        })?;
    if binding.route != "libtorch"
        || binding.library != "cgx2.libtorch.cpu"
        || binding.version != profile.version
        || binding.abi != "cgx2-libtorch-facade-v1"
        || binding.layout != "tensor-f64-cpu-v1"
    {
        return Err(vec![diagnostic(
            "CGN004",
            "source tensor adapter differs from the pinned CPU float64 facade",
            source,
            Some(profile.declaration_span),
        )]);
    }
    let artifact = checked_artifact(package.root(), binding).map_err(|message| {
        vec![diagnostic(
            "CGN006",
            format!("binding {}: {message}", binding.id),
            source,
            Some(profile.declaration_span),
        )]
    })?;
    package.verify_unchanged().map_err(|errors| {
        errors
            .into_iter()
            .map(|error| diagnostic(error.code, error.message, &error.file, error.span))
            .collect::<Vec<_>>()
    })?;
    Ok(Some(Prepared {
        package,
        entry,
        profile,
        artifact,
    }))
}

pub(super) fn maybe_check(root: &Path) -> Result<Option<()>, Vec<NativeDiagnostic>> {
    prepare(root).map(|prepared| prepared.map(|_| ()))
}

pub(super) fn maybe_run(
    root: &Path,
    args: &[String],
) -> Result<Option<String>, Vec<NativeDiagnostic>> {
    let Some(prepared) = prepare(root)? else {
        return Ok(None);
    };
    let source = prepared.package.modules()[prepared.entry].source();
    if !args.is_empty() {
        return Err(vec![diagnostic(
            "CGN009",
            "tensor profile does not accept program arguments",
            source,
            Some(prepared.profile.call_span),
        )]);
    }
    // Keep the verified descriptor alive until Torch and every tensor handle have dropped.
    let fd_path = PathBuf::from(format!("/proc/self/fd/{}", prepared.artifact.as_raw_fd()));
    // SAFETY: the source explicitly declares unsafe native execution. The checked lock pins
    // facade bytes, target, ABI and signatures; only reviewed facade.h implementations may be
    // used. Native metadata is checked again by Torch::open.
    let torch = unsafe { Torch::open(&fd_path, &prepared.profile.version) }.map_err(|error| {
        vec![diagnostic(
            "CGN007",
            format!("tensor facade load failed: {error:?}"),
            source,
            Some(prepared.profile.declaration_span),
        )]
    })?;
    let call_error = |error| {
        vec![diagnostic(
            "CGR024",
            format!("tensor call failed: {error:?}"),
            source,
            Some(prepared.profile.call_span),
        )]
    };
    let left = torch.from_f64(&prepared.profile.left).map_err(call_error)?;
    let right = torch
        .from_f64(&prepared.profile.right)
        .map_err(call_error)?;
    let sum = left.add(&right).map_err(call_error)?;
    let values = sum.to_vec().map_err(call_error)?;
    if values.len() != prepared.profile.left.len() {
        return Err(vec![diagnostic(
            "CGR024",
            "tensor facade returned unexpected shape",
            source,
            Some(prepared.profile.call_span),
        )]);
    }
    if values.iter().any(|value| !value.is_finite()) {
        return Err(vec![diagnostic(
            "CGR024",
            "tensor result is not finite float64",
            source,
            Some(prepared.profile.call_span),
        )]);
    }
    Ok(Some(format!("{values:?}")))
}
