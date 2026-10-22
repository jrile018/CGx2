//! Direct, version-pinned LibTorch CPU facade. No Python runtime is used.

use cgx2_ffi_native::{Error as NativeError, Library, LibraryDeclaration};
use std::ffi::{CStr, c_char, c_void};
use std::path::Path;
use std::ptr::NonNull;
use std::rc::Rc;

type Create = unsafe extern "C" fn(*const f64, u64, *mut *mut c_void) -> i32;
type Add = unsafe extern "C" fn(*const c_void, *const c_void, *mut *mut c_void) -> i32;
type Length = unsafe extern "C" fn(*const c_void, *mut u64) -> i32;
type Copy = unsafe extern "C" fn(*const c_void, *mut f64, u64) -> i32;
type Release = unsafe extern "C" fn(*mut c_void) -> i32;
type LastError = unsafe extern "C" fn() -> *const c_char;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Native(NativeError),
    InvalidLength,
    NullHandle,
    DifferentRuntime,
    AlreadyReleased,
    Foreign { status: i32, message: String },
}

impl From<NativeError> for Error {
    fn from(value: NativeError) -> Self {
        Self::Native(value)
    }
}

struct Inner {
    _library: Library,
    create: Create,
    add: Add,
    length: Length,
    copy: Copy,
    release: Release,
    last_error: LastError,
}

impl Inner {
    fn check(&self, status: i32) -> Result<(), Error> {
        if status == 0 {
            return Ok(());
        }
        let ptr = unsafe { (self.last_error)() };
        let message = if ptr.is_null() {
            "native error without message".into()
        } else {
            unsafe { CStr::from_ptr(ptr) }
                .to_string_lossy()
                .into_owned()
        };
        Err(Error::Foreign { status, message })
    }

    fn tensor(self: &Rc<Self>, status: i32, raw: *mut c_void) -> Result<Tensor, Error> {
        if let Err(error) = self.check(status) {
            if !raw.is_null() {
                unsafe {
                    (self.release)(raw);
                }
            }
            return Err(error);
        }
        Ok(Tensor {
            inner: self.clone(),
            raw: Some(NonNull::new(raw).ok_or(Error::NullHandle)?),
        })
    }
}

#[derive(Clone)]
pub struct Torch {
    inner: Rc<Inner>,
}

impl Torch {
    /// `exact_version` must match the LibTorch version used to compile the facade.
    pub fn open(path: &Path, exact_version: &str) -> Result<Self, Error> {
        let library = Library::open(
            path,
            &LibraryDeclaration::new("cgx2.libtorch.cpu", exact_version),
        )?;
        // SAFETY: facade.h specifies these exact C signatures. This adapter only loads that
        // reviewed facade, after identity and exact version metadata validation.
        let inner = unsafe {
            Inner {
                create: library.symbol("cgx2_torch_f64_cpu")?,
                add: library.symbol("cgx2_torch_add")?,
                length: library.symbol("cgx2_torch_length")?,
                copy: library.symbol("cgx2_torch_copy_f64")?,
                release: library.symbol("cgx2_torch_release")?,
                last_error: library.symbol("cgx2_torch_last_error")?,
                _library: library,
            }
        };
        Ok(Self {
            inner: Rc::new(inner),
        })
    }

    /// Creates a one-dimensional CPU float64 tensor, copying `values` into LibTorch storage.
    pub fn from_f64(&self, values: &[f64]) -> Result<Tensor, Error> {
        let length = u64::try_from(values.len()).map_err(|_| Error::InvalidLength)?;
        let mut raw = std::ptr::null_mut();
        let status = unsafe { (self.inner.create)(values.as_ptr(), length, &mut raw) };
        self.inner.tensor(status, raw)
    }
}

pub struct Tensor {
    inner: Rc<Inner>,
    raw: Option<NonNull<c_void>>,
}

impl Tensor {
    fn ptr(&self) -> Result<*mut c_void, Error> {
        self.raw.map(NonNull::as_ptr).ok_or(Error::AlreadyReleased)
    }

    pub fn add(&self, other: &Self) -> Result<Self, Error> {
        if !Rc::ptr_eq(&self.inner, &other.inner) {
            return Err(Error::DifferentRuntime);
        }
        let mut raw = std::ptr::null_mut();
        let status = unsafe { (self.inner.add)(self.ptr()?, other.ptr()?, &mut raw) };
        self.inner.tensor(status, raw)
    }

    pub fn shape(&self) -> Result<[usize; 1], Error> {
        let mut length = 0;
        let status = unsafe { (self.inner.length)(self.ptr()?, &mut length) };
        self.inner.check(status)?;
        Ok([usize::try_from(length).map_err(|_| Error::InvalidLength)?])
    }

    pub fn to_vec(&self) -> Result<Vec<f64>, Error> {
        let [length] = self.shape()?;
        let mut output = vec![0.0; length];
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

    pub fn dtype(&self) -> &'static str {
        "float64"
    }
    pub fn device(&self) -> &'static str {
        "cpu"
    }

    pub fn release(&mut self) -> Result<(), Error> {
        let raw = self.raw.take().ok_or(Error::AlreadyReleased)?;
        self.inner
            .check(unsafe { (self.inner.release)(raw.as_ptr()) })
    }
}

impl Drop for Tensor {
    fn drop(&mut self) {
        let _ = self.release();
    }
}
