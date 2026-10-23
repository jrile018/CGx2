//! One lexical `.cg` tensor call, using the frozen direct LibTorch facade.
use std::path::Path;

use cgx2_bind::checked_lock;
use cgx2_libtorch::Torch;
use cgx2_package::{load_checked_package, CheckedPackage};
use cgx2_sema::{tensor_profile, MatrixLiteral, TensorOperation, TensorProfile};

use super::{diagnostic, NativeDiagnostic};

struct Prepared {
    package: CheckedPackage,
    entry: usize,
    profile: TensorProfile,
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
    if lock.schema_version != 2 {
        return Err(vec![diagnostic(
            "CGN004",
            "source tensor calls require a schema-2 binding and complete LibTorch closure lock",
            source,
            Some(profile.declaration_span),
        )]);
    }
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
    let layout = match profile.operation {
        TensorOperation::VectorAdd { .. } => "tensor-f64-cpu-v1",
        TensorOperation::MatrixMultiply { .. } => "tensor-f64-cpu-matrix-v2",
    };
    if binding.route != "libtorch"
        || binding.library != "cgx2.libtorch.cpu"
        || binding.version != profile.version
        || binding.abi != "cgx2-libtorch-facade-v1"
        || binding.layout != layout
    {
        return Err(vec![diagnostic(
            "CGN004",
            "source tensor adapter differs from the pinned CPU float64 facade",
            source,
            Some(profile.declaration_span),
        )]);
    }
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
    // SAFETY: the source declares an unsafe native call, and the package has a schema-2
    // closure lock. The adapter rechecks the ELF closure and mapped modules before returning.
    // Native constructors may run during dlopen, so only reviewed artifacts are permitted.
    let torch =
        unsafe { Torch::open_pinned(prepared.package.root(), &prepared.profile.binding_id) }
            .map_err(|error| {
                vec![diagnostic(
                    "CGN007",
                    format!("tensor facade load failed: {error:?}"),
                    source,
                    Some(prepared.profile.declaration_span),
                )]
            })?;
    let value = match &prepared.profile.operation {
        TensorOperation::VectorAdd { left, right } => {
            let call_error = |error| {
                vec![diagnostic(
                    "CGR024",
                    format!("tensor call failed: {error:?}"),
                    source,
                    Some(prepared.profile.call_span),
                )]
            };
            let left_handle = torch.from_f64(left).map_err(call_error)?;
            let right_handle = torch.from_f64(right).map_err(call_error)?;
            let sum = left_handle.add(&right_handle).map_err(call_error)?;
            let shape = sum.shape().map_err(call_error)?;
            if shape[0] != left.len() {
                return Err(vec![diagnostic(
                    "CGR024",
                    "tensor facade returned unexpected shape",
                    source,
                    Some(prepared.profile.call_span),
                )]);
            }
            let values = sum.to_vec().map_err(call_error)?;
            if values.len() != left.len() || values.iter().any(|value| !value.is_finite()) {
                return Err(vec![diagnostic(
                    "CGR024",
                    "tensor facade returned invalid float64 result",
                    source,
                    Some(prepared.profile.call_span),
                )]);
            }
            format!("{values:?}")
        }
        TensorOperation::MatrixMultiply { left, right } => {
            run_matrix(&torch, left, right, source, prepared.profile.call_span)?
        }
    };
    Ok(Some(value))
}

fn run_matrix(
    torch: &Torch,
    left: &MatrixLiteral,
    right: &MatrixLiteral,
    source: &Path,
    span: cgx2_syntax::Span,
) -> Result<String, Vec<NativeDiagnostic>> {
    let failure = |message| diagnostic("CGR024", message, source, Some(span));
    let mut left_handle = None;
    let mut right_handle = None;
    let mut product = None;
    let result = (|| {
        left_handle = Some(
            torch
                .from_f64_matrix(left.rows, left.cols, &left.values)
                .map_err(|error| failure(format!("matrix input failed: {error:?}")))?,
        );
        right_handle = Some(
            torch
                .from_f64_matrix(right.rows, right.cols, &right.values)
                .map_err(|error| failure(format!("matrix input failed: {error:?}")))?,
        );
        product = Some(
            left_handle
                .as_ref()
                .expect("created left matrix")
                .matmul(right_handle.as_ref().expect("created right matrix"))
                .map_err(|error| failure(format!("matrix multiplication failed: {error:?}")))?,
        );
        let product_ref = product.as_ref().expect("created product matrix");
        let expected = [left.rows, right.cols];
        let shape = product_ref
            .shape()
            .map_err(|error| failure(format!("matrix shape failed: {error:?}")))?;
        if shape != expected {
            return Err(failure(
                "matrix facade returned unexpected output shape".into(),
            ));
        }
        let count = expected[0] * expected[1];
        let mut values = vec![0.0; count];
        product_ref
            .copy_into(&mut values)
            .map_err(|error| failure(format!("matrix copy failed: {error:?}")))?;
        if values.iter().any(|value| !value.is_finite()) {
            return Err(failure("matrix result is not finite float64".into()));
        }
        let rows: Vec<_> = values
            .chunks(expected[1])
            .map(|row| format!("{row:?}"))
            .collect();
        Ok(format!("[{}]", rows.join(", ")))
    })();
    let mut errors: Vec<_> = result.as_ref().err().cloned().into_iter().collect();
    for matrix in [&mut product, &mut right_handle, &mut left_handle]
        .into_iter()
        .flatten()
    {
        if let Err(error) = matrix.release() {
            errors.push(diagnostic(
                "CGR025",
                format!("matrix handle release failed: {error:?}"),
                source,
                Some(span),
            ));
        }
    }
    if errors.is_empty() {
        Ok(result.expect("result has no error"))
    } else {
        Err(errors)
    }
}
