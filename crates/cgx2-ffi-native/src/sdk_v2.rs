//! Reviewed, pinned native SDK v2. Never retain caller pointers or unwind through C.
use crate::{Library, LibraryDeclaration};
use cgx2_bind::{Binding, Report, checked_artifact, checked_lock};
use cgx2_ffi_core::{Error as ValueError, ForeignValue, Limits};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::rc::{Rc, Weak};

type LiveSet = RefCell<HashSet<usize>>;
thread_local! {
    // Independent adapters to the same mapped inode must not own an alias twice.
    static LIVE: RefCell<HashMap<(u64, u64), Weak<LiveSet>>> = RefCell::new(HashMap::new());
}
fn live_registry(file: &File) -> Result<Rc<LiveSet>, Error> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata().map_err(|e| Error::Pin(e.to_string()))?;
        let key = (metadata.dev(), metadata.ino());
        Ok(LIVE.with(|registry| {
            let mut registry = registry.borrow_mut();
            registry.retain(|_, weak| weak.strong_count() > 0);
            if let Some(live) = registry.get(&key).and_then(Weak::upgrade) {
                return live;
            }
            let live = Rc::new(RefCell::new(HashSet::new()));
            registry.insert(key, Rc::downgrade(&live));
            live
        }))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = file;
        Err(crate::Error::UnsupportedPlatform.into())
    }
}

type Copy = unsafe extern "C" fn(*const u8, u64, *mut u8, u64, *mut u64) -> i32;
type Create = unsafe extern "C" fn(i64, *mut *mut c_void) -> i32;
type Read = unsafe extern "C" fn(*const c_void, *mut i64) -> i32;
type Release = unsafe extern "C" fn(*mut c_void) -> i32;

#[derive(Debug)]
pub enum Error {
    Preflight(Report),
    Pin(String),
    Native(crate::Error),
    Value(ValueError),
    ApiVersion(u32),
    NullHandle,
    Alias,
    AlreadyReleased,
}
impl From<crate::Error> for Error {
    fn from(value: crate::Error) -> Self {
        Self::Native(value)
    }
}
impl From<ValueError> for Error {
    fn from(value: ValueError) -> Self {
        Self::Value(value)
    }
}

struct Inner {
    _library: Library,
    artifact: File,
    root: PathBuf,
    binding: Binding,
    copy_bytes: Copy,
    copy_utf8: Copy,
    create: Create,
    read: Read,
    release: Release,
    live: Rc<LiveSet>,
}
impl Inner {
    fn verify(&self) -> Result<(), Error> {
        let file = checked_artifact(&self.root, &self.binding).map_err(Error::Pin)?;
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::MetadataExt;
            let old = self
                .artifact
                .metadata()
                .map_err(|e| Error::Pin(e.to_string()))?;
            let now = file.metadata().map_err(|e| Error::Pin(e.to_string()))?;
            if (old.dev(), old.ino()) != (now.dev(), now.ino()) {
                return Err(Error::Pin("artifact inode changed".into()));
            }
        }
        Ok(())
    }
    fn status(&self, status: i32) -> Result<(), Error> {
        self.verify()?;
        if status != 0 {
            return Err(ValueError::ForeignFailure(status).into());
        }
        Ok(())
    }
}

/// Calling-thread-only adapter. Functions, handles and shared aliases retain the DSO.
pub struct Sdk {
    inner: Rc<Inner>,
}
impl Sdk {
    /// Validate lock schema 3, target, hash, exports, layout and metadata before use.
    ///
    /// # Safety
    /// The artifact and its dependencies must be reviewed against sdk_v2.h. Hashes
    /// attest identity, not native memory safety. Constructors run during dlopen.
    pub unsafe fn open_pinned(root: &Path, binding_id: &str) -> Result<Self, Error> {
        let lock = checked_lock(root).map_err(Error::Preflight)?;
        let binding = lock
            .bindings
            .iter()
            .find(|b| b.id == binding_id && b.layout == "native-sdk-v2" && b.route == "native-c")
            .ok_or_else(|| Error::Pin("native SDK binding absent".into()))?
            .clone();
        for utf8 in [false, true] {
            cgx2_bind::sdk_v2::copied_declaration(&binding, &lock.package, utf8).validate()?;
        }
        let artifact = checked_artifact(root, &binding).map_err(Error::Pin)?;
        #[cfg(target_os = "linux")]
        let path = {
            use std::os::fd::AsRawFd;
            PathBuf::from(format!("/proc/self/fd/{}", artifact.as_raw_fd()))
        };
        #[cfg(not(target_os = "linux"))]
        let path = root.join(&binding.artifact);
        let library = unsafe {
            Library::open(
                &path,
                &LibraryDeclaration::new(&binding.library, &binding.version),
            )?
        };
        let version: unsafe extern "C" fn() -> u32 =
            unsafe { library.symbol("cgx2_sdk_api_version")? };
        let reported = unsafe { version() };
        if reported != 2 {
            return Err(Error::ApiVersion(reported));
        }
        let live = live_registry(&artifact)?;
        let inner = unsafe {
            Inner {
                copy_bytes: library.symbol("cgx2_sdk_copy_bytes")?,
                copy_utf8: library.symbol("cgx2_sdk_copy_utf8")?,
                create: library.symbol("cgx2_sdk_create")?,
                read: library.symbol("cgx2_sdk_read")?,
                release: library.symbol("cgx2_sdk_release")?,
                _library: library,
                artifact,
                root: std::fs::canonicalize(root).map_err(|e| Error::Pin(e.to_string()))?,
                binding,
                live,
            }
        };
        inner.verify()?;
        Ok(Self {
            inner: Rc::new(inner),
        })
    }

    /// One invocation, no size probe or automatic retry. Output capacity is fixed
    /// before dispatch and charged to `limits.max_bytes` (maximum 1 MiB).
    pub fn copy(&self, input: &[u8], utf8: bool, limits: Limits) -> Result<ForeignValue, Error> {
        limits.validate()?;
        if input.len() > limits.max_bytes {
            return Err(ValueError::Limit.into());
        }
        self.inner.verify()?;
        let mut output = vec![0; limits.max_bytes];
        let mut written = 0;
        let call = if utf8 {
            self.inner.copy_utf8
        } else {
            self.inner.copy_bytes
        };
        let status = unsafe {
            call(
                input.as_ptr(),
                input.len() as u64,
                output.as_mut_ptr(),
                output.len() as u64,
                &mut written,
            )
        };
        self.inner.status(status)?;
        let written = usize::try_from(written).map_err(|_| ValueError::Limit)?;
        if written > output.len() {
            return Err(ValueError::Limit.into());
        }
        output.truncate(written);
        Ok(if utf8 {
            ForeignValue::Utf8(String::from_utf8(output).map_err(|_| ValueError::InvalidUtf8)?)
        } else {
            ForeignValue::Bytes(output)
        })
    }

    pub fn create(&self, value: i64) -> Result<OwnedHandle, Error> {
        self.inner.verify()?;
        let mut raw = std::ptr::null_mut();
        let status = unsafe { (self.inner.create)(value, &mut raw) };
        if let Err(error) = self.inner.status(status) {
            if !raw.is_null()
                && !self.inner.live.borrow().contains(&(raw as usize))
                && self.inner.verify().is_ok()
            {
                unsafe {
                    (self.inner.release)(raw);
                }
            }
            return Err(error);
        }
        let raw = NonNull::new(raw).ok_or(Error::NullHandle)?;
        if !self.inner.live.borrow_mut().insert(raw.as_ptr() as usize) {
            return Err(Error::Alias);
        }
        Ok(OwnedHandle {
            state: State {
                inner: self.inner.clone(),
                raw: Some(raw),
            },
        })
    }
}
struct State {
    inner: Rc<Inner>,
    raw: Option<NonNull<c_void>>,
}
impl State {
    fn read(&self) -> Result<i64, Error> {
        let raw = self.raw.ok_or(Error::AlreadyReleased)?;
        self.inner.verify()?;
        let mut value = 0;
        let status = unsafe { (self.inner.read)(raw.as_ptr(), &mut value) };
        self.inner.status(status)?;
        Ok(value)
    }
    fn release(&mut self) -> Result<(), Error> {
        let raw = self.raw.ok_or(Error::AlreadyReleased)?;
        self.inner.verify()?;
        // SDK v2 release is terminal even on foreign failure. Never retry a free.
        self.raw = None;
        self.inner
            .live
            .borrow_mut()
            .remove(&(raw.as_ptr() as usize));
        self.inner
            .status(unsafe { (self.inner.release)(raw.as_ptr()) })
    }
}
impl Drop for State {
    fn drop(&mut self) {
        let _ = self.release();
    }
}
pub struct OwnedHandle {
    state: State,
}
impl OwnedHandle {
    pub fn read(&self) -> Result<i64, Error> {
        self.state.read()
    }
    pub fn release(&mut self) -> Result<(), Error> {
        self.state.release()
    }
    pub fn into_shared(self) -> SharedHandle {
        SharedHandle {
            state: Rc::new(self.state),
        }
    }
}
/// Immutable Rust aliases to a single foreign owner, not native retain/release.
/// Drop of the final alias releases once; aliases cannot explicitly free peers.
#[derive(Clone)]
pub struct SharedHandle {
    state: Rc<State>,
}
impl SharedHandle {
    pub fn read(&self) -> Result<i64, Error> {
        self.state.read()
    }
}
