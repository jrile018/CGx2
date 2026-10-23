//! Direct, version-pinned LibTorch CPU facade. No Python runtime is used.

use cgx2_bind::{ClosureError, Report, VerifiedClosure, checked_lock, verify_libtorch_closure};
use cgx2_ffi_native::{Error as NativeError, Library, LibraryDeclaration};
use std::cell::RefCell;
use std::collections::HashSet;
use std::ffi::{c_char, c_void};
use std::path::Path;
use std::ptr::NonNull;
use std::rc::Rc;

type Create = unsafe extern "C" fn(*const f64, u64, *mut *mut c_void) -> i32;
type Add = unsafe extern "C" fn(*const c_void, *const c_void, *mut *mut c_void) -> i32;
type Length = unsafe extern "C" fn(*const c_void, *mut u64) -> i32;
type Copy = unsafe extern "C" fn(*const c_void, *mut f64, u64) -> i32;
type Release = unsafe extern "C" fn(*mut c_void) -> i32;
type LastError = unsafe extern "C" fn() -> *const c_char;
type MatrixVersion = unsafe extern "C" fn() -> u32;
type MatrixCreate = unsafe extern "C" fn(*const f64, u64, u64, *mut *mut c_void) -> i32;
type Matmul = unsafe extern "C" fn(*const c_void, *const c_void, *mut *mut c_void) -> i32;
type MatrixShape = unsafe extern "C" fn(*const c_void, *mut u64, *mut u64) -> i32;
type MatrixCopy = unsafe extern "C" fn(*const c_void, *mut f64, u64) -> i32;
type MatrixCreateF32 = unsafe extern "C" fn(*const f32, u64, u64, *mut *mut c_void) -> i32;
type MatrixInfoF32 = unsafe extern "C" fn(*const c_void, *mut u64, *mut u64, *mut u32) -> i32;
type MatrixCopyF32 = unsafe extern "C" fn(*const c_void, *mut f32, u64) -> i32;
struct InferenceFunctions {
    add: Matmul,
    relu: unsafe extern "C" fn(*const c_void, *mut *mut c_void) -> i32,
}
struct MatrixFunctions {
    create: MatrixCreate,
    matmul: Matmul,
    shape: MatrixShape,
    copy: MatrixCopy,
}
struct MatrixFunctionsF32 {
    create: MatrixCreateF32,
    matmul: Matmul,
    info: MatrixInfoF32,
    copy: MatrixCopyF32,
}
/// Maximum bytes read from a foreign error string before truncation.
pub const MAX_ERROR_BYTES: usize = 4096;

/// Maximum host copy: 1,048,576 values (8 MiB for float64, 4 MiB for float32).
pub const MAX_COPY_ELEMENTS: usize = 1_048_576;
pub const F32_DTYPE_CODE: u32 = 1;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Native(NativeError),
    InvalidLength,
    InvalidMatrixShape,
    NonFinite,
    MatrixDtypeMismatch {
        reported: u32,
    },
    MatrixDimensionMismatch {
        left_cols: usize,
        right_rows: usize,
    },
    MatrixShapeMismatch {
        expected: [usize; 2],
        reported: [usize; 2],
    },
    CapacityMismatch {
        expected: usize,
        provided: usize,
    },
    MatrixApiUnavailable,
    MatrixApiVersion {
        reported: u32,
    },
    LengthLimitExceeded {
        reported: usize,
        maximum: usize,
    },
    ShapeMismatch {
        expected: usize,
        reported: usize,
    },
    UnverifiedLoader(ClosureError),
    NullHandle,
    DifferentRuntime,
    AlreadyReleased,
    Foreign {
        status: i32,
        message: String,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum PinnedError {
    Preflight(Report),
    Closure(ClosureError),
    Adapter(Error),
}

impl From<NativeError> for Error {
    fn from(value: NativeError) -> Self {
        Self::Native(value)
    }
}

struct Inner {
    _library: Library,
    pinned: Option<VerifiedClosure>,
    create: Create,
    matrix: Option<MatrixFunctions>,
    matrix_f32: Option<MatrixFunctionsF32>,
    inference: Option<InferenceFunctions>,
    add: Add,
    length: Length,
    copy: Copy,
    release: Release,
    last_error: LastError,
    live_handles: RefCell<HashSet<usize>>,
}

impl Inner {
    fn check_pinned(&self) -> Result<(), Error> {
        if let Some(closure) = &self.pinned {
            closure.verify_loaded().map_err(Error::UnverifiedLoader)?;
        }
        Ok(())
    }

    fn check(&self, status: i32) -> Result<(), Error> {
        // A facade call may have changed the loader state. Do not call even its
        // error accessor until the closure has been checked again.
        self.check_pinned()?;
        if status == 0 {
            return Ok(());
        }
        let ptr = unsafe { (self.last_error)() };
        let message = if ptr.is_null() {
            "native error without message".into()
        } else {
            // The facade promises a readable NUL-terminated string. Bound
            // the scan and allocation even if that promise is broken.
            let mut length = 0;
            while length < MAX_ERROR_BYTES && unsafe { *ptr.add(length) } != 0 {
                length += 1;
            }
            let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), length) };
            let mut message = String::from_utf8_lossy(bytes).into_owned();
            if length == MAX_ERROR_BYTES {
                message.push_str(" [truncated]");
            }
            message
        };
        self.check_pinned()?;
        Err(Error::Foreign { status, message })
    }

    fn tensor(
        self: &Rc<Self>,
        status: i32,
        raw: *mut c_void,
        expected_length: usize,
    ) -> Result<Tensor, Error> {
        if let Err(error) = self.check(status) {
            // A failed foreign call may still return an owned output. Never
            // release a pointer already owned by another Tensor, or call back
            // into a closure that became unverifiable during the call.
            if !raw.is_null()
                && !matches!(error, Error::UnverifiedLoader(_))
                && !self.live_handles.borrow().contains(&(raw as usize))
                && self.check_pinned().is_ok()
            {
                unsafe {
                    (self.release)(raw);
                }
            }
            return Err(error);
        }
        let raw = NonNull::new(raw).ok_or(Error::NullHandle)?;
        if !self.live_handles.borrow_mut().insert(raw.as_ptr() as usize) {
            return Err(Error::Foreign {
                status: -1,
                message: "facade returned an already owned handle".into(),
            });
        }
        Ok(Tensor {
            inner: self.clone(),
            raw: Some(raw),
            expected_length,
        })
    }

    fn matrix(
        self: &Rc<Self>,
        status: i32,
        raw: *mut c_void,
        shape: [usize; 2],
    ) -> Result<Matrix, Error> {
        if let Err(error) = self.check(status) {
            if !raw.is_null()
                && !matches!(error, Error::UnverifiedLoader(_))
                && !self.live_handles.borrow().contains(&(raw as usize))
                && self.check_pinned().is_ok()
            {
                unsafe {
                    (self.release)(raw);
                }
            }
            return Err(error);
        }
        let raw = NonNull::new(raw).ok_or(Error::NullHandle)?;
        if !self.live_handles.borrow_mut().insert(raw.as_ptr() as usize) {
            return Err(Error::Foreign {
                status: -1,
                message: "facade returned an already owned handle".into(),
            });
        }
        Ok(Matrix {
            inner: self.clone(),
            raw: Some(raw),
            expected_shape: shape,
        })
    }

    fn matrix_f32(
        self: &Rc<Self>,
        status: i32,
        raw: *mut c_void,
        shape: [usize; 2],
    ) -> Result<MatrixF32, Error> {
        if let Err(error) = self.check(status) {
            if !raw.is_null()
                && !matches!(error, Error::UnverifiedLoader(_))
                && !self.live_handles.borrow().contains(&(raw as usize))
                && self.check_pinned().is_ok()
            {
                unsafe { (self.release)(raw) };
            }
            return Err(error);
        }
        let raw = NonNull::new(raw).ok_or(Error::NullHandle)?;
        if !self.live_handles.borrow_mut().insert(raw.as_ptr() as usize) {
            return Err(Error::Foreign {
                status: -1,
                message: "facade returned an already owned handle".into(),
            });
        }
        Ok(MatrixF32 {
            inner: self.clone(),
            raw: Some(raw),
            expected_shape: shape,
        })
    }
}

#[derive(Clone)]
pub struct Torch {
    inner: Rc<Inner>,
}

impl Torch {
    /// Open only after schema-2/3 package preflight and complete ELF closure checks.
    /// The loader's mapped paths and inodes are checked before any tensor operation.
    ///
    /// # Safety
    /// The lock identifies reviewed binaries but cannot prove the facade's C ABI or
    /// foreign handle lifetimes. The caller must validate those contracts separately.
    pub unsafe fn open_pinned(root: &Path, binding_id: &str) -> Result<Self, PinnedError> {
        let lock = checked_lock(root).map_err(PinnedError::Preflight)?;
        if !matches!(lock.schema_version, 2 | 3) {
            return Err(PinnedError::Closure(ClosureError::Invalid(
                "schema-2/3 closure lock required".into(),
            )));
        }
        let binding = lock
            .bindings
            .iter()
            .find(|binding| binding.id == binding_id && binding.route == "libtorch")
            .ok_or_else(|| {
                PinnedError::Closure(ClosureError::Invalid(
                    "LibTorch binding id not found".into(),
                ))
            })?;
        let verified = verify_libtorch_closure(root, binding).map_err(PinnedError::Closure)?;
        verified.verify_preloaded().map_err(PinnedError::Closure)?;
        let mut torch = unsafe { Self::open(verified.facade_path(), &binding.version) }
            .map_err(PinnedError::Adapter)?;
        verified.verify_loaded().map_err(PinnedError::Closure)?;
        Rc::get_mut(&mut torch.inner)
            .expect("new adapter has one owner")
            .pinned = Some(verified);
        if binding.layout == "tensor-f64-cpu-matrix-v2"
            || binding.layout == "tensor-f32-cpu-matrix-v3"
            || binding.layout == "tensor-f32-cpu-inference-v4"
        {
            torch.enable_matrix().map_err(PinnedError::Adapter)?;
        }
        if matches!(
            binding.layout.as_str(),
            "tensor-f32-cpu-matrix-v3" | "tensor-f32-cpu-inference-v4"
        ) {
            torch.enable_matrix_f32().map_err(PinnedError::Adapter)?;
        }
        if binding.layout == "tensor-f32-cpu-inference-v4" {
            torch.enable_inference().map_err(PinnedError::Adapter)?;
        }
        Ok(torch)
    }
    /// `exact_version` must match the LibTorch version used to compile the facade.
    ///
    /// # Safety
    ///
    /// `path` must be the reviewed facade built from this crate's `facade.h`.
    /// Version metadata alone cannot establish the native symbol signatures.
    pub unsafe fn open(path: &Path, exact_version: &str) -> Result<Self, Error> {
        let library = unsafe {
            Library::open(
                path,
                &LibraryDeclaration::new("cgx2.libtorch.cpu", exact_version),
            )?
        };
        // SAFETY: facade.h specifies these exact C signatures. This adapter only loads that
        // reviewed facade, after identity and exact version metadata validation.
        let inner = unsafe {
            Inner {
                pinned: None,
                create: library.symbol("cgx2_torch_f64_cpu")?,
                matrix: None,
                matrix_f32: None,
                inference: None,
                add: library.symbol("cgx2_torch_add")?,
                length: library.symbol("cgx2_torch_length")?,
                copy: library.symbol("cgx2_torch_copy_f64")?,
                release: library.symbol("cgx2_torch_release")?,
                last_error: library.symbol("cgx2_torch_last_error")?,
                live_handles: RefCell::new(HashSet::new()),
                _library: library,
            }
        };
        Ok(Self {
            inner: Rc::new(inner),
        })
    }

    /// Creates a one-dimensional CPU float64 tensor, copying `values` into LibTorch storage.
    pub fn from_f64(&self, values: &[f64]) -> Result<Tensor, Error> {
        self.inner.check_pinned()?;
        let length = u64::try_from(values.len()).map_err(|_| Error::InvalidLength)?;
        let mut raw = std::ptr::null_mut();
        let status = unsafe { (self.inner.create)(values.as_ptr(), length, &mut raw) };
        self.inner.tensor(status, raw, values.len())
    }

    /// Open the additive matrix API without package closure pinning. Intended
    /// for reviewed local facades and mock tests; use `open_pinned` for packages.
    ///
    /// # Safety
    /// The caller must verify all matrix C signatures and handle lifetimes.
    pub unsafe fn open_matrix(path: &Path, exact_version: &str) -> Result<Self, Error> {
        let mut torch = unsafe { Self::open(path, exact_version) }?;
        torch.enable_matrix()?;
        Ok(torch)
    }

    /// Open the additive float32 matrix API on a reviewed local facade.
    ///
    /// # Safety
    /// The caller must verify the v3 C signatures and handle lifetimes.
    pub unsafe fn open_matrix_f32(path: &Path, exact_version: &str) -> Result<Self, Error> {
        let mut torch = unsafe { Self::open_matrix(path, exact_version) }?;
        torch.enable_matrix_f32()?;
        Ok(torch)
    }

    /// Open reviewed additive inference API v4 without package pinning.
    ///
    /// # Safety
    /// All v1-v4 exports must obey facade.h, including exception containment.
    pub unsafe fn open_inference(path: &Path, exact_version: &str) -> Result<Self, Error> {
        let mut torch = unsafe { Self::open_matrix_f32(path, exact_version) }?;
        torch.enable_inference()?;
        Ok(torch)
    }

    fn enable_inference(&mut self) -> Result<(), Error> {
        self.inner.check_pinned()?;
        let inner = Rc::get_mut(&mut self.inner).expect("new adapter has one owner");
        let version: MatrixVersion =
            unsafe { inner._library.symbol("cgx2_torch_inference_api_version")? };
        let functions = unsafe {
            InferenceFunctions {
                add: inner._library.symbol("cgx2_torch_matrix_add_f32")?,
                relu: inner._library.symbol("cgx2_torch_matrix_relu_f32")?,
            }
        };
        let reported = unsafe { version() };
        inner.check_pinned()?;
        if reported != 4 {
            return Err(Error::MatrixApiVersion { reported });
        }
        inner.inference = Some(functions);
        Ok(())
    }

    fn enable_matrix(&mut self) -> Result<(), Error> {
        self.inner.check_pinned()?;
        let inner = Rc::get_mut(&mut self.inner).expect("new adapter has one owner");
        let library = &inner._library;
        let version: MatrixVersion = unsafe { library.symbol("cgx2_torch_matrix_api_version")? };
        let matrix = unsafe {
            MatrixFunctions {
                create: library.symbol("cgx2_torch_matrix_f64_cpu")?,
                matmul: library.symbol("cgx2_torch_matmul")?,
                shape: library.symbol("cgx2_torch_matrix_shape")?,
                copy: library.symbol("cgx2_torch_matrix_copy_f64")?,
            }
        };
        inner.check_pinned()?;
        let reported = unsafe { version() };
        inner.check_pinned()?;
        if reported != 2 {
            return Err(Error::MatrixApiVersion { reported });
        }
        inner.matrix = Some(matrix);
        Ok(())
    }

    fn enable_matrix_f32(&mut self) -> Result<(), Error> {
        self.inner.check_pinned()?;
        let inner = Rc::get_mut(&mut self.inner).expect("new adapter has one owner");
        let library = &inner._library;
        let version: MatrixVersion =
            unsafe { library.symbol("cgx2_torch_matrix_f32_api_version")? };
        let functions = unsafe {
            MatrixFunctionsF32 {
                create: library.symbol("cgx2_torch_matrix_f32_cpu")?,
                matmul: library.symbol("cgx2_torch_matmul_f32")?,
                info: library.symbol("cgx2_torch_matrix_f32_info")?,
                copy: library.symbol("cgx2_torch_matrix_copy_f32")?,
            }
        };
        inner.check_pinned()?;
        let reported = unsafe { version() };
        inner.check_pinned()?;
        if reported != 3 {
            return Err(Error::MatrixApiVersion { reported });
        }
        inner.matrix_f32 = Some(functions);
        Ok(())
    }

    /// Copy a nonempty contiguous row-major matrix into LibTorch-owned CPU storage.
    pub fn from_f64_matrix(
        &self,
        rows: usize,
        cols: usize,
        values: &[f64],
    ) -> Result<Matrix, Error> {
        let count = matrix_count(rows, cols)?;
        if count != values.len() {
            return Err(Error::CapacityMismatch {
                expected: count,
                provided: values.len(),
            });
        }
        self.inner.check_pinned()?;
        let functions = self
            .inner
            .matrix
            .as_ref()
            .ok_or(Error::MatrixApiUnavailable)?;
        let mut raw = std::ptr::null_mut();
        let status =
            unsafe { (functions.create)(values.as_ptr(), rows as u64, cols as u64, &mut raw) };
        self.inner.matrix(status, raw, [rows, cols])
    }

    /// Copy finite row-major float32 values into a distinct owned CPU matrix.
    pub fn from_f32_matrix(
        &self,
        rows: usize,
        cols: usize,
        values: &[f32],
    ) -> Result<MatrixF32, Error> {
        let count = matrix_count_f32(rows, cols)?;
        if count != values.len() {
            return Err(Error::CapacityMismatch {
                expected: count,
                provided: values.len(),
            });
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(Error::NonFinite);
        }
        self.inner.check_pinned()?;
        let functions = self
            .inner
            .matrix_f32
            .as_ref()
            .ok_or(Error::MatrixApiUnavailable)?;
        let mut raw = std::ptr::null_mut();
        let status =
            unsafe { (functions.create)(values.as_ptr(), rows as u64, cols as u64, &mut raw) };
        self.inner.matrix_f32(status, raw, [rows, cols])
    }
}

fn matrix_count(rows: usize, cols: usize) -> Result<usize, Error> {
    if rows == 0 || cols == 0 {
        return Err(Error::InvalidMatrixShape);
    }
    let count = rows.checked_mul(cols).ok_or(Error::InvalidMatrixShape)?;
    if count > MAX_COPY_ELEMENTS
        || count.checked_mul(size_of::<f64>()).is_none()
        || u64::try_from(rows).is_err()
        || u64::try_from(cols).is_err()
    {
        return Err(Error::InvalidMatrixShape);
    }
    Ok(count)
}

fn matrix_count_f32(rows: usize, cols: usize) -> Result<usize, Error> {
    let count = matrix_count(rows, cols)?;
    if count.checked_mul(size_of::<f32>()).is_none() {
        return Err(Error::InvalidMatrixShape);
    }
    Ok(count)
}

pub struct Tensor {
    inner: Rc<Inner>,
    raw: Option<NonNull<c_void>>,
    expected_length: usize,
}

impl Tensor {
    fn ptr(&self) -> Result<*mut c_void, Error> {
        self.raw.map(NonNull::as_ptr).ok_or(Error::AlreadyReleased)
    }

    pub fn add(&self, other: &Self) -> Result<Self, Error> {
        if !Rc::ptr_eq(&self.inner, &other.inner) {
            return Err(Error::DifferentRuntime);
        }
        self.inner.check_pinned()?;
        let mut raw = std::ptr::null_mut();
        let status = unsafe { (self.inner.add)(self.ptr()?, other.ptr()?, &mut raw) };
        self.inner.tensor(status, raw, self.expected_length)
    }

    pub fn shape(&self) -> Result<[usize; 1], Error> {
        self.inner.check_pinned()?;
        let mut length = 0;
        let status = unsafe { (self.inner.length)(self.ptr()?, &mut length) };
        self.inner.check(status)?;
        Ok([usize::try_from(length).map_err(|_| Error::InvalidLength)?])
    }

    /// Copy at most `maximum` elements, also subject to [`MAX_COPY_ELEMENTS`].
    /// The facade length must equal the length known when this handle was created.
    pub fn to_vec_bounded(&self, maximum: usize) -> Result<Vec<f64>, Error> {
        let [length] = self.shape()?;
        let maximum = maximum.min(MAX_COPY_ELEMENTS);
        if length > maximum {
            return Err(Error::LengthLimitExceeded {
                reported: length,
                maximum,
            });
        }
        if length != self.expected_length {
            return Err(Error::ShapeMismatch {
                expected: self.expected_length,
                reported: length,
            });
        }
        let mut output = vec![0.0; length];
        self.inner.check_pinned()?;
        let status = unsafe {
            (self.inner.copy)(
                self.ptr()?,
                output.as_mut_ptr(),
                u64::try_from(length).map_err(|_| Error::InvalidLength)?,
            )
        };
        self.inner.check(status)?;
        Ok(output)
    }

    /// Compatibility copy with a fixed 8 MiB upper bound. Use `to_vec_bounded`
    /// to supply a tighter application limit.
    pub fn to_vec(&self) -> Result<Vec<f64>, Error> {
        self.to_vec_bounded(MAX_COPY_ELEMENTS)
    }

    pub fn dtype(&self) -> &'static str {
        "float64"
    }
    pub fn device(&self) -> &'static str {
        "cpu"
    }

    pub fn release(&mut self) -> Result<(), Error> {
        let raw = self.raw.ok_or(Error::AlreadyReleased)?;
        self.inner.check_pinned()?;
        self.raw = None;
        self.inner
            .live_handles
            .borrow_mut()
            .remove(&(raw.as_ptr() as usize));
        self.inner
            .check(unsafe { (self.inner.release)(raw.as_ptr()) })
    }
}

impl Drop for Tensor {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

/// One owned two-dimensional CPU float64 handle. Its LibTorch allocation is foreign;
/// CGx2 owns only this wrapper and host copies returned by `to_vec_bounded`.
pub struct Matrix {
    inner: Rc<Inner>,
    raw: Option<NonNull<c_void>>,
    expected_shape: [usize; 2],
}

impl Matrix {
    fn ptr(&self) -> Result<*mut c_void, Error> {
        self.raw.map(NonNull::as_ptr).ok_or(Error::AlreadyReleased)
    }

    /// Multiply `[m, k]` by `[k, n]`, producing an owned `[m, n]` matrix.
    pub fn matmul(&self, other: &Self) -> Result<Self, Error> {
        if !Rc::ptr_eq(&self.inner, &other.inner) {
            return Err(Error::DifferentRuntime);
        }
        let left = self.ptr()?;
        let right = other.ptr()?;
        if self.expected_shape[1] != other.expected_shape[0] {
            return Err(Error::MatrixDimensionMismatch {
                left_cols: self.expected_shape[1],
                right_rows: other.expected_shape[0],
            });
        }
        matrix_count(self.expected_shape[0], other.expected_shape[1])?;
        self.inner.check_pinned()?;
        let functions = self
            .inner
            .matrix
            .as_ref()
            .ok_or(Error::MatrixApiUnavailable)?;
        let mut raw = std::ptr::null_mut();
        let status = unsafe { (functions.matmul)(left, right, &mut raw) };
        self.inner.matrix(
            status,
            raw,
            [self.expected_shape[0], other.expected_shape[1]],
        )
    }

    pub fn shape(&self) -> Result<[usize; 2], Error> {
        let raw = self.ptr()?;
        self.inner.check_pinned()?;
        let functions = self
            .inner
            .matrix
            .as_ref()
            .ok_or(Error::MatrixApiUnavailable)?;
        let (mut rows, mut cols) = (0, 0);
        let status = unsafe { (functions.shape)(raw, &mut rows, &mut cols) };
        self.inner.check(status)?;
        let reported = [
            usize::try_from(rows).map_err(|_| Error::InvalidMatrixShape)?,
            usize::try_from(cols).map_err(|_| Error::InvalidMatrixShape)?,
        ];
        matrix_count(reported[0], reported[1])?;
        if reported != self.expected_shape {
            return Err(Error::MatrixShapeMismatch {
                expected: self.expected_shape,
                reported,
            });
        }
        Ok(reported)
    }

    /// Copy into a caller-owned buffer whose capacity exactly matches the shape.
    pub fn copy_into(&self, output: &mut [f64]) -> Result<(), Error> {
        let [rows, cols] = self.shape()?;
        let count = matrix_count(rows, cols)?;
        if output.len() != count {
            return Err(Error::CapacityMismatch {
                expected: count,
                provided: output.len(),
            });
        }
        self.inner.check_pinned()?;
        let functions = self
            .inner
            .matrix
            .as_ref()
            .ok_or(Error::MatrixApiUnavailable)?;
        let status = unsafe { (functions.copy)(self.ptr()?, output.as_mut_ptr(), count as u64) };
        self.inner.check(status)
    }

    /// Allocate a CGx2 host copy bounded by `maximum` elements and 8 MiB.
    pub fn to_vec_bounded(&self, maximum: usize) -> Result<Vec<f64>, Error> {
        let [rows, cols] = self.shape()?;
        let count = matrix_count(rows, cols)?;
        let maximum = maximum.min(MAX_COPY_ELEMENTS);
        if count > maximum {
            return Err(Error::LengthLimitExceeded {
                reported: count,
                maximum,
            });
        }
        let mut output = vec![0.0; count];
        self.copy_into(&mut output)?;
        Ok(output)
    }

    pub fn release(&mut self) -> Result<(), Error> {
        let raw = self.raw.ok_or(Error::AlreadyReleased)?;
        self.inner.check_pinned()?;
        self.raw = None;
        self.inner
            .live_handles
            .borrow_mut()
            .remove(&(raw.as_ptr() as usize));
        self.inner
            .check(unsafe { (self.inner.release)(raw.as_ptr()) })
    }
}

impl Drop for Matrix {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

/// Metadata returned by the facade, independently of the requested shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatrixInfoF32Value {
    pub shape: [usize; 2],
    pub dtype: u32,
}

/// One owned two-dimensional CPU float32 handle, distinct from `Matrix`.
pub struct MatrixF32 {
    inner: Rc<Inner>,
    raw: Option<NonNull<c_void>>,
    expected_shape: [usize; 2],
}

impl MatrixF32 {
    fn ptr(&self) -> Result<*mut c_void, Error> {
        self.raw.map(NonNull::as_ptr).ok_or(Error::AlreadyReleased)
    }

    /// Return facade-reported dimensions and dtype without replacing them with caller expectations.
    pub fn info(&self) -> Result<MatrixInfoF32Value, Error> {
        let raw = self.ptr()?;
        self.inner.check_pinned()?;
        let functions = self
            .inner
            .matrix_f32
            .as_ref()
            .ok_or(Error::MatrixApiUnavailable)?;
        let (mut rows, mut cols, mut dtype) = (0, 0, 0);
        let status = unsafe { (functions.info)(raw, &mut rows, &mut cols, &mut dtype) };
        self.inner.check(status)?;
        let shape = [
            usize::try_from(rows).map_err(|_| Error::InvalidMatrixShape)?,
            usize::try_from(cols).map_err(|_| Error::InvalidMatrixShape)?,
        ];
        matrix_count_f32(shape[0], shape[1])?;
        Ok(MatrixInfoF32Value { shape, dtype })
    }

    fn checked_shape(&self) -> Result<[usize; 2], Error> {
        let info = self.info()?;
        if info.dtype != F32_DTYPE_CODE {
            return Err(Error::MatrixDtypeMismatch {
                reported: info.dtype,
            });
        }
        if info.shape != self.expected_shape {
            return Err(Error::MatrixShapeMismatch {
                expected: self.expected_shape,
                reported: info.shape,
            });
        }
        Ok(info.shape)
    }

    pub fn shape(&self) -> Result<[usize; 2], Error> {
        self.checked_shape()
    }

    /// Multiply compatible float32 matrices from the same facade instance.
    pub fn matmul(&self, other: &Self) -> Result<Self, Error> {
        if !Rc::ptr_eq(&self.inner, &other.inner) {
            return Err(Error::DifferentRuntime);
        }
        let left = self.checked_shape()?;
        let right = other.checked_shape()?;
        if left[1] != right[0] {
            return Err(Error::MatrixDimensionMismatch {
                left_cols: left[1],
                right_rows: right[0],
            });
        }
        matrix_count_f32(left[0], right[1])?;
        self.inner.check_pinned()?;
        let functions = self
            .inner
            .matrix_f32
            .as_ref()
            .ok_or(Error::MatrixApiUnavailable)?;
        let mut raw = std::ptr::null_mut();
        let status = unsafe { (functions.matmul)(self.ptr()?, other.ptr()?, &mut raw) };
        self.inner.matrix_f32(status, raw, [left[0], right[1]])
    }

    /// Exact-shape addition, without broadcasting. Distinct owned output.
    pub fn add(&self, other: &Self) -> Result<Self, Error> {
        if !Rc::ptr_eq(&self.inner, &other.inner) {
            return Err(Error::DifferentRuntime);
        }
        let shape = self.checked_shape()?;
        let other_shape = other.checked_shape()?;
        if shape != other_shape {
            return Err(Error::MatrixShapeMismatch {
                expected: shape,
                reported: other_shape,
            });
        }
        self.inner.check_pinned()?;
        let functions = self
            .inner
            .inference
            .as_ref()
            .ok_or(Error::MatrixApiUnavailable)?;
        let mut raw = std::ptr::null_mut();
        let status = unsafe { (functions.add)(self.ptr()?, other.ptr()?, &mut raw) };
        self.inner.matrix_f32(status, raw, shape)
    }

    /// CPU float32 ReLU under inference mode, with finite output and no autograd.
    pub fn relu(&self) -> Result<Self, Error> {
        let shape = self.checked_shape()?;
        self.inner.check_pinned()?;
        let functions = self
            .inner
            .inference
            .as_ref()
            .ok_or(Error::MatrixApiUnavailable)?;
        let mut raw = std::ptr::null_mut();
        let status = unsafe { (functions.relu)(self.ptr()?, &mut raw) };
        self.inner.matrix_f32(status, raw, shape)
    }

    /// The reviewed f32 CPU profile checks device during every info call.
    pub fn device(&self) -> Result<&'static str, Error> {
        self.checked_shape()?;
        Ok("cpu")
    }

    /// Copy into an exactly sized caller buffer after checking all values are finite.
    pub fn copy_into(&self, output: &mut [f32]) -> Result<(), Error> {
        let [rows, cols] = self.checked_shape()?;
        let count = matrix_count_f32(rows, cols)?;
        if output.len() != count {
            return Err(Error::CapacityMismatch {
                expected: count,
                provided: output.len(),
            });
        }
        self.inner.check_pinned()?;
        let functions = self
            .inner
            .matrix_f32
            .as_ref()
            .ok_or(Error::MatrixApiUnavailable)?;
        let mut staged = vec![0.0; count];
        let status = unsafe { (functions.copy)(self.ptr()?, staged.as_mut_ptr(), count as u64) };
        self.inner.check(status)?;
        if staged.iter().any(|value| !value.is_finite()) {
            return Err(Error::NonFinite);
        }
        output.copy_from_slice(&staged);
        Ok(())
    }

    pub fn to_vec_bounded(&self, maximum: usize) -> Result<Vec<f32>, Error> {
        let [rows, cols] = self.checked_shape()?;
        let count = matrix_count_f32(rows, cols)?;
        let maximum = maximum.min(MAX_COPY_ELEMENTS);
        if count > maximum {
            return Err(Error::LengthLimitExceeded {
                reported: count,
                maximum,
            });
        }
        let mut output = vec![0.0; count];
        self.copy_into(&mut output)?;
        Ok(output)
    }

    pub fn release(&mut self) -> Result<(), Error> {
        let raw = self.raw.ok_or(Error::AlreadyReleased)?;
        self.inner.check_pinned()?;
        self.raw = None;
        self.inner
            .live_handles
            .borrow_mut()
            .remove(&(raw.as_ptr() as usize));
        self.inner
            .check(unsafe { (self.inner.release)(raw.as_ptr()) })
    }
}

impl Drop for MatrixF32 {
    fn drop(&mut self) {
        let _ = self.release();
    }
}
