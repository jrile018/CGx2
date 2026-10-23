//! Checked, deliberately narrow native C ABI v1 boundary for Linux/x86-64.
//! Declarations are reviewed inputs: C cannot reveal a symbol's physical signature.

use std::ffi::{CStr, CString, c_char, c_void};
use std::path::Path;
use std::ptr::NonNull;
use std::rc::Rc;
pub mod sdk_v2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    UnsupportedPlatform,
    UnsupportedDeclaration,
    OpenFailed,
    MissingSymbol(String),
    IdentityMismatch,
    VersionMismatch,
    AbiMismatch,
    InvalidMetadata,
    ForeignStatus(i32),
    NullHandle,
    AlreadyReleased,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbiType {
    I32,
    I64,
    Bytes,
    OpaqueHandle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    None,
    BorrowedForCall,
    ForeignOwned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    ScalarI64,
    CopyBytes,
}

#[derive(Debug, Clone)]
pub struct LibraryDeclaration<'a> {
    pub schema_version: u32,
    pub package_id: &'a str,
    pub library_id: &'a str,
    pub library_version: &'a str,
    pub abi_version: u32,
    pub target_os: &'a str,
    pub target_arch: &'a str,
}

impl<'a> LibraryDeclaration<'a> {
    pub fn new(library_id: &'a str, library_version: &'a str) -> Self {
        Self {
            schema_version: 1,
            package_id: library_id,
            library_id,
            library_version,
            abi_version: 1,
            target_os: "linux",
            target_arch: "x86_64",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Declaration<'a> {
    pub symbol: &'a str,
    pub operation: Operation,
    pub parameters: Vec<AbiType>,
    pub result: AbiType,
    pub calling_convention: &'a str,
    pub error_channel: &'a str,
    pub ownership: Ownership,
    pub encoding: &'a str,
    pub thread_rule: &'a str,
    pub callbacks: bool,
    pub effects: &'a [&'a str],
    pub capabilities: &'a [&'a str],
}

impl<'a> Declaration<'a> {
    pub fn scalar(symbol: &'a str) -> Self {
        Self {
            symbol,
            operation: Operation::ScalarI64,
            parameters: vec![AbiType::I64],
            result: AbiType::I64,
            calling_convention: "C",
            error_channel: "i32_status_zero_ok",
            ownership: Ownership::None,
            encoding: "none",
            thread_rule: "calling_thread",
            callbacks: false,
            effects: &["foreign"],
            capabilities: &["native"],
        }
    }

    pub fn bytes(symbol: &'a str) -> Self {
        Self {
            symbol,
            operation: Operation::CopyBytes,
            parameters: vec![AbiType::Bytes],
            result: AbiType::Bytes,
            calling_convention: "C",
            error_channel: "i32_status_zero_ok",
            ownership: Ownership::BorrowedForCall,
            encoding: "bytes",
            thread_rule: "calling_thread",
            callbacks: false,
            effects: &["foreign", "allocate"],
            capabilities: &[],
        }
    }

    fn validate(&self) -> Result<(), Error> {
        let shape = match self.operation {
            Operation::ScalarI64 => {
                self.parameters == [AbiType::I64]
                    && self.result == AbiType::I64
                    && self.ownership == Ownership::None
                    && self.encoding == "none"
            }
            Operation::CopyBytes => {
                self.parameters == [AbiType::Bytes]
                    && self.result == AbiType::Bytes
                    && self.ownership == Ownership::BorrowedForCall
                    && self.encoding == "bytes"
            }
        };
        if !shape
            || self.calling_convention != "C"
            || self.error_channel != "i32_status_zero_ok"
            || self.thread_rule != "calling_thread"
            || self.callbacks
            || !self.effects.contains(&"foreign")
            || (self.operation == Operation::ScalarI64 && !self.capabilities.contains(&"native"))
        {
            return Err(Error::UnsupportedDeclaration);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct HandleDeclaration<'a> {
    pub create_symbol: &'a str,
    pub read_symbol: &'a str,
    pub release_symbol: &'a str,
    pub parameter: AbiType,
    pub result: AbiType,
    pub ownership: Ownership,
    pub calling_convention: &'a str,
    pub error_channel: &'a str,
    pub thread_rule: &'a str,
    pub callbacks: bool,
    pub effects: &'a [&'a str],
}

impl<'a> HandleDeclaration<'a> {
    pub fn new(create_symbol: &'a str, read_symbol: &'a str, release_symbol: &'a str) -> Self {
        Self {
            create_symbol,
            read_symbol,
            release_symbol,
            parameter: AbiType::I64,
            result: AbiType::OpaqueHandle,
            ownership: Ownership::ForeignOwned,
            calling_convention: "C",
            error_channel: "i32_status_zero_ok",
            thread_rule: "calling_thread",
            callbacks: false,
            effects: &["foreign", "allocate"],
        }
    }
    fn validate(&self) -> Result<(), Error> {
        if self.parameter != AbiType::I64
            || self.result != AbiType::OpaqueHandle
            || self.ownership != Ownership::ForeignOwned
            || self.release_symbol.is_empty()
            || self.calling_convention != "C"
            || self.error_channel != "i32_status_zero_ok"
            || self.thread_rule != "calling_thread"
            || self.callbacks
            || !self.effects.contains(&"foreign")
        {
            return Err(Error::UnsupportedDeclaration);
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
#[link(name = "dl")]
unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> i32;
}

#[derive(Debug)]
struct LibraryInner {
    raw: NonNull<c_void>,
}

impl Drop for LibraryInner {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        unsafe {
            dlclose(self.raw.as_ptr());
        }
    }
}

#[derive(Debug, Clone)]
pub struct Library {
    inner: Rc<LibraryInner>,
}

impl Library {
    /// Loads metadata from a reviewed native library.
    ///
    /// # Safety
    ///
    /// `path` must identify a trusted library whose three `cgx2_*` metadata
    /// exports have the exact C signatures used below. Identity and version
    /// strings cannot prove the physical signatures of those exports.
    pub unsafe fn open(path: &Path, declaration: &LibraryDeclaration<'_>) -> Result<Self, Error> {
        if declaration.schema_version != 1
            || declaration.package_id.is_empty()
            || declaration.library_id.is_empty()
            || declaration.library_version.is_empty()
            || declaration.abi_version != 1
        {
            return Err(Error::UnsupportedDeclaration);
        }
        if !cfg!(all(target_os = "linux", target_arch = "x86_64"))
            || declaration.target_os != "linux"
            || declaration.target_arch != "x86_64"
        {
            return Err(Error::UnsupportedPlatform);
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::ffi::OsStrExt;
            let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| Error::OpenFailed)?;
            let raw = unsafe { dlopen(path.as_ptr(), 2) }; // RTLD_NOW, local visibility by default.
            let raw = NonNull::new(raw).ok_or(Error::OpenFailed)?;
            let library = Self {
                inner: Rc::new(LibraryInner { raw }),
            };
            type Text = unsafe extern "C" fn() -> *const c_char;
            type Version = unsafe extern "C" fn() -> u32;
            let identity: Text = unsafe { library.symbol("cgx2_library_identity")? };
            let version: Text = unsafe { library.symbol("cgx2_library_version")? };
            let abi: Version = unsafe { library.symbol("cgx2_abi_version")? };
            let identity = unsafe { checked_text(identity())? };
            let version = unsafe { checked_text(version())? };
            if identity != declaration.library_id {
                return Err(Error::IdentityMismatch);
            }
            if version != declaration.library_version {
                return Err(Error::VersionMismatch);
            }
            if unsafe { abi() } != declaration.abi_version {
                return Err(Error::AbiMismatch);
            }
            Ok(library)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = path;
            Err(Error::UnsupportedPlatform)
        }
    }

    /// Resolves a function from a reviewed native library.
    ///
    /// # Safety
    ///
    /// The caller must prove `T` exactly matches the exported C function signature,
    /// including calling convention and parameter/result layout. C symbol tables do
    /// not encode this information. Metadata symbols and reviewed facades are the
    /// only intended callers.
    pub unsafe fn symbol<T: Copy>(&self, name: &str) -> Result<T, Error> {
        #[cfg(target_os = "linux")]
        {
            if std::mem::size_of::<T>() != std::mem::size_of::<*mut c_void>() {
                return Err(Error::UnsupportedDeclaration);
            }
            let name_c = CString::new(name).map_err(|_| Error::UnsupportedDeclaration)?;
            let raw = unsafe { dlsym(self.inner.raw.as_ptr(), name_c.as_ptr()) };
            if raw.is_null() {
                return Err(Error::MissingSymbol(name.into()));
            }
            Ok(unsafe { std::mem::transmute_copy(&raw) })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = name;
            Err(Error::UnsupportedPlatform)
        }
    }

    /// # Safety
    ///
    /// The selected export must have the exact physical C signature described
    /// by `declaration`. Dynamic symbol lookup cannot verify that signature.
    pub unsafe fn bind(&self, declaration: &Declaration<'_>) -> Result<Function, Error> {
        declaration.validate()?;
        match declaration.operation {
            Operation::ScalarI64 => {
                let call = unsafe { self.symbol(declaration.symbol)? };
                Ok(Function {
                    kind: FunctionKind::Scalar {
                        _library: self.inner.clone(),
                        call,
                    },
                })
            }
            Operation::CopyBytes => {
                let call = unsafe { self.symbol(declaration.symbol)? };
                Ok(Function {
                    kind: FunctionKind::Bytes {
                        _library: self.inner.clone(),
                        call,
                    },
                })
            }
        }
    }

    /// # Safety
    ///
    /// All create/read/release exports must have the declared C signatures and
    /// obey the documented ownership contract, including on error paths. In
    /// particular, a nonzero release status must leave the handle valid and
    /// owned by the caller so it can be retried or released during `Drop`.
    pub unsafe fn bind_handle(
        &self,
        declaration: &HandleDeclaration<'_>,
    ) -> Result<HandleFactory, Error> {
        declaration.validate()?;
        let create = unsafe { self.symbol(declaration.create_symbol)? };
        let read = unsafe { self.symbol(declaration.read_symbol)? };
        let release = unsafe { self.symbol(declaration.release_symbol)? };
        Ok(HandleFactory {
            library: self.inner.clone(),
            create,
            read,
            release,
        })
    }
}

unsafe fn checked_text(ptr: *const c_char) -> Result<String, Error> {
    if ptr.is_null() {
        return Err(Error::InvalidMetadata);
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map(str::to_owned)
        .map_err(|_| Error::InvalidMetadata)
}

type ScalarFn = unsafe extern "C" fn(i64, *mut i64) -> i32;
type BytesFn = unsafe extern "C" fn(*const u8, u64, *mut u8, u64) -> i32;
type CreateFn = unsafe extern "C" fn(i64, *mut *mut c_void) -> i32;
type ReadFn = unsafe extern "C" fn(*mut c_void, *mut i64) -> i32;
type ReleaseFn = unsafe extern "C" fn(*mut c_void) -> i32;

#[derive(Debug)]
enum FunctionKind {
    Scalar {
        _library: Rc<LibraryInner>,
        call: ScalarFn,
    },
    Bytes {
        _library: Rc<LibraryInner>,
        call: BytesFn,
    },
}

#[derive(Debug)]
pub struct Function {
    kind: FunctionKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyStats {
    pub bytes: usize,
    pub host_allocations: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CopyResult {
    pub value: Vec<u8>,
    pub copy: CopyStats,
}

impl Function {
    pub fn call_i64(&self, value: i64) -> Result<i64, Error> {
        let FunctionKind::Scalar { call, .. } = &self.kind else {
            return Err(Error::UnsupportedDeclaration);
        };
        let mut output = 0;
        let status = unsafe { call(value, &mut output) };
        if status != 0 {
            return Err(Error::ForeignStatus(status));
        }
        Ok(output)
    }

    pub fn copy_bytes(&self, input: &[u8]) -> Result<CopyResult, Error> {
        let FunctionKind::Bytes { call, .. } = &self.kind else {
            return Err(Error::UnsupportedDeclaration);
        };
        let length = u64::try_from(input.len()).map_err(|_| Error::UnsupportedDeclaration)?;
        let mut output = vec![0; input.len()];
        let status = unsafe { call(input.as_ptr(), length, output.as_mut_ptr(), length) };
        if status != 0 {
            return Err(Error::ForeignStatus(status));
        }
        Ok(CopyResult {
            value: output,
            copy: CopyStats {
                bytes: input.len(),
                host_allocations: usize::from(!input.is_empty()),
            },
        })
    }
}

#[derive(Debug)]
pub struct HandleFactory {
    library: Rc<LibraryInner>,
    create: CreateFn,
    read: ReadFn,
    release: ReleaseFn,
}

impl HandleFactory {
    pub fn create(&self, value: i64) -> Result<OwnedHandle, Error> {
        let mut raw = std::ptr::null_mut();
        let status = unsafe { (self.create)(value, &mut raw) };
        if status != 0 {
            if !raw.is_null() {
                unsafe {
                    (self.release)(raw);
                }
            }
            return Err(Error::ForeignStatus(status));
        }
        let raw = NonNull::new(raw).ok_or(Error::NullHandle)?;
        Ok(OwnedHandle {
            _library: self.library.clone(),
            raw: Some(raw),
            read: self.read,
            release: self.release,
        })
    }
}

#[derive(Debug)]
pub struct OwnedHandle {
    _library: Rc<LibraryInner>,
    raw: Option<NonNull<c_void>>,
    read: ReadFn,
    release: ReleaseFn,
}

impl OwnedHandle {
    pub fn read(&self) -> Result<i64, Error> {
        let raw = self.raw.ok_or(Error::AlreadyReleased)?;
        let mut output = 0;
        let status = unsafe { (self.read)(raw.as_ptr(), &mut output) };
        if status != 0 {
            return Err(Error::ForeignStatus(status));
        }
        Ok(output)
    }

    pub fn release(&mut self) -> Result<(), Error> {
        let raw = self.raw.ok_or(Error::AlreadyReleased)?;
        let status = unsafe { (self.release)(raw.as_ptr()) };
        if status != 0 {
            return Err(Error::ForeignStatus(status));
        }
        self.raw = None;
        Ok(())
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        let _ = self.release();
    }
}
