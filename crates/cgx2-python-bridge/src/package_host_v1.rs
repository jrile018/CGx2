//! Low-level trusted-host machinery. Use cgx2-foreign's package lock API for preflight.
use crate::host::{ValueBudget, bridge_to_python, conversion_error, python_exception};
use crate::{BridgeError, BridgeValue, PyHandle, PythonBridge};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyModule, PyTuple};
use std::sync::Mutex;
use std::{ffi::CString, path::PathBuf};

static PACKAGE_INVOCATION: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedIdentityV1 {
    pub executable: PathBuf,
    /// Mapping containing the actual linked Py_GetVersion symbol, not sysconfig's guess.
    pub library: PathBuf,
    pub version: String,
    pub prefix: PathBuf,
    pub base_prefix: PathBuf,
}

/// A verified, copied source snapshot. Never reopened by the Python loader.
pub struct PythonSourceV1 {
    pub module: String,
    pub filename: String,
    pub is_package: bool,
    pub source: Vec<u8>,
}

pub struct PinnedInvocationV1<'a> {
    pub sources: &'a [PythonSourceV1],
    pub stdlib_paths: &'a [String],
    pub module: &'a str,
    pub attribute: &'a str,
    pub positional: &'a [BridgeValue],
    pub keyword: &'a [(&'a str, BridgeValue)],
    pub array_result: bool,
    pub input_bytes: usize,
}

impl PythonBridge {
    /// Initialize CPython with isolated configuration and site/.pth startup disabled.
    /// The caller must check package/runtime artifacts before calling this method.
    /// Unlike `new`, this rejects an already initialized interpreter.
    ///
    /// # Safety
    /// Call once at application startup, before ANY other thread or library uses
    /// CPython/PyO3. No concurrent interpreter initialization/finalization is allowed.
    #[cfg(target_os = "linux")]
    pub unsafe fn initialize_isolated_v1(
        interpreter: &std::path::Path,
        library: &std::path::Path,
        version: &str,
        stdlib_paths: &[String],
    ) -> Result<Self, BridgeError> {
        use std::ffi::CStr;
        use std::os::unix::ffi::OsStrExt;
        let _exclusive = PACKAGE_INVOCATION
            .try_lock()
            .map_err(|_| conversion_error("exclusive interpreter startup", "busy".into()))?;
        let program = CString::new(interpreter.as_os_str().as_bytes())
            .map_err(|_| conversion_error("interpreter path without NUL", "NUL".into()))?;
        if stdlib_paths.is_empty() {
            return Err(conversion_error(
                "pinned stdlib paths for isolated startup",
                "empty".into(),
            ));
        }
        let search_paths = stdlib_paths
            .iter()
            .map(|p| {
                CString::new(p.as_bytes())
                    .map_err(|_| conversion_error("stdlib path without NUL", "NUL".into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        // SAFETY: the caller guarantees no concurrent CPython use; Py_GetVersion
        // and Py_IsInitialized are valid before initialization. The version string
        // is CPython-owned static storage. PyConfig is cleared on every path.
        unsafe {
            if pyo3::ffi::Py_IsInitialized() != 0 {
                return Err(conversion_error(
                    "uninitialized CPython",
                    "already initialized".into(),
                ));
            }
            if linked_python_image()? != library
                || CStr::from_ptr(pyo3::ffi::Py_GetVersion()).to_string_lossy() != version
            {
                return Err(conversion_error(
                    "pinned linked CPython image/version",
                    "mismatch".into(),
                ));
            }
            let mut config = std::mem::MaybeUninit::<pyo3::ffi::PyConfig>::uninit();
            pyo3::ffi::PyConfig_InitIsolatedConfig(config.as_mut_ptr());
            let mut config = config.assume_init();
            config.site_import = 0;
            config.user_site_directory = 0;
            config.write_bytecode = 0;
            config.parse_argv = 0;
            config.module_search_paths_set = 1;
            let mut status = pyo3::ffi::PyConfig_SetBytesString(
                &raw mut config,
                &raw mut config.program_name,
                program.as_ptr(),
            );
            for path in &search_paths {
                if pyo3::ffi::PyStatus_Exception(status) != 0 {
                    break;
                }
                let wide = pyo3::ffi::Py_DecodeLocale(path.as_ptr(), std::ptr::null_mut());
                if wide.is_null() {
                    pyo3::ffi::PyConfig_Clear(&raw mut config);
                    return Err(conversion_error(
                        "decoded stdlib path",
                        "allocation/encoding failure".into(),
                    ));
                }
                status =
                    pyo3::ffi::PyWideStringList_Append(&raw mut config.module_search_paths, wide);
                pyo3::ffi::PyMem_RawFree(wide.cast());
            }
            let status = if pyo3::ffi::PyStatus_Exception(status) == 0 {
                pyo3::ffi::Py_InitializeFromConfig(&config)
            } else {
                status
            };
            let failure = if pyo3::ffi::PyStatus_Exception(status) != 0 {
                Some(if status.err_msg.is_null() {
                    "CPython initialization failed".into()
                } else {
                    CStr::from_ptr(status.err_msg)
                        .to_string_lossy()
                        .into_owned()
                })
            } else {
                None
            };
            pyo3::ffi::PyConfig_Clear(&raw mut config);
            if let Some(failure) = failure {
                return Err(conversion_error("isolated CPython startup", failure));
            }
            // PyO3 attaches through its GIL API; release the initialization thread's
            // GIL first. Interpreter lifetime is process lifetime; never finalize here.
            pyo3::ffi::PyEval_SaveThread();
        }
        Self::new()
    }

    /// Current identity of the already initialized trusted interpreter. Linux only in v1.
    /// Initialization/site startup belongs to the embedding application's trusted host.
    pub fn embedded_identity_v1(&self) -> Result<EmbeddedIdentityV1, BridgeError> {
        Python::attach(|py| {
            let sys = py.import("sys").map_err(|e| python_exception(py, e))?;
            let path = |name| -> Result<PathBuf, BridgeError> {
                let value: String = sys
                    .getattr(name)
                    .and_then(|v| v.extract())
                    .map_err(|e| python_exception(py, e))?;
                std::fs::canonicalize(value)
                    .map_err(|e| conversion_error("canonical runtime path", e.to_string()))
            };
            Ok(EmbeddedIdentityV1 {
                executable: path("executable")?,
                library: linked_python_image()?,
                version: sys
                    .getattr("version")
                    .and_then(|v| v.extract())
                    .map_err(|e| python_exception(py, e))?,
                prefix: path("prefix")?,
                base_prefix: path("base_prefix")?,
            })
        })
    }

    /// Executes only supplied package snapshots with a scoped importer. This low-level API
    /// does not authenticate sources; cgx2-foreign performs identity/hash/signature checks.
    /// All temporary Python references are dropped while attached, including failure paths.
    pub fn call_pinned_copied_v1(
        &self,
        call: &PinnedInvocationV1<'_>,
    ) -> Result<BridgeValue, BridgeError> {
        // Never wait while a caller might hold the GIL. No reentrant callbacks or
        // concurrent package invocations are supported by this scoped host profile.
        let _exclusive = PACKAGE_INVOCATION
            .try_lock()
            .map_err(|_| conversion_error("one non-reentrant package invocation", "busy".into()))?;
        let mut budget = ValueBudget::default();
        for value in call
            .positional
            .iter()
            .chain(call.keyword.iter().map(|(_, v)| v))
        {
            budget.check_rust(value, 0)?;
            if contains_handle(value) {
                return Err(conversion_error("copied values", "Python handle".into()));
            }
        }
        Python::attach(|py| {
            let bootstrap =
                CString::new(include_str!("package_loader_v1.py")).expect("static code");
            let loader = PyModule::from_code(
                py,
                &bootstrap,
                c"cgx2_package_loader_v1.py",
                c"_cgx2_package_loader_v1",
            )
            .map_err(|e| python_exception(py, e))?;
            let sources = PyDict::new(py);
            for source in call.sources {
                sources
                    .set_item(
                        &source.module,
                        (
                            PyBytes::new(py, &source.source),
                            &source.filename,
                            source.is_package,
                        ),
                    )
                    .map_err(|e| python_exception(py, e))?;
            }
            let positional = call
                .positional
                .iter()
                .map(|v| bridge_to_python(py, v))
                .collect::<Result<Vec<_>, _>>()?;
            let args = PyTuple::new(py, positional).map_err(|e| python_exception(py, e))?;
            let kwargs = PyDict::new(py);
            for (name, value) in call.keyword {
                kwargs
                    .set_item(*name, bridge_to_python(py, value)?)
                    .map_err(|e| python_exception(py, e))?;
            }
            let result = loader
                .getattr("invoke")
                .and_then(|f| {
                    f.call1((
                        sources,
                        call.stdlib_paths,
                        call.module,
                        call.attribute,
                        args,
                        kwargs,
                    ))
                })
                .map_err(|e| python_exception(py, e))?;
            let handle = PyHandle(result.unbind());
            if call.array_result {
                return self
                    .to_f64_array_1d_copy_v1(&handle, call.input_bytes)
                    .map(BridgeValue::F64Array1DCopyV1);
            }
            let copied = self.to_value_with_live_bytes_v1(&handle, call.input_bytes)?;
            if contains_handle(&copied) {
                return Err(conversion_error(
                    "acyclic built-in copied result",
                    "unsupported object, subclass, key or cycle".into(),
                ));
            }
            Ok(copied)
        })
    }
}

fn contains_handle(value: &BridgeValue) -> bool {
    match value {
        BridgeValue::Handle(_) => true,
        BridgeValue::List(v) | BridgeValue::Tuple(v) => v.iter().any(contains_handle),
        BridgeValue::Dict(v) => v.values().any(contains_handle),
        _ => false,
    }
}

#[cfg(target_os = "linux")]
fn linked_python_image() -> Result<PathBuf, BridgeError> {
    let address = pyo3::ffi::Py_GetVersion as *const () as usize;
    let maps = std::fs::read_to_string("/proc/self/maps")
        .map_err(|e| conversion_error("readable Linux process mappings", e.to_string()))?;
    for line in maps.lines() {
        let mut columns = line.split_whitespace();
        let Some((start, end)) = columns.next().and_then(|s| s.split_once('-')) else {
            continue;
        };
        let (Ok(start), Ok(end)) = (
            usize::from_str_radix(start, 16),
            usize::from_str_radix(end, 16),
        ) else {
            continue;
        };
        if (start..end).contains(&address) {
            // permissions, offset, device, inode, then pathname (which can contain spaces).
            let path = columns.skip(4).collect::<Vec<_>>().join(" ");
            return std::fs::canonicalize(path)
                .map_err(|e| conversion_error("mapped CPython image", e.to_string()));
        }
    }
    Err(conversion_error("mapped CPython image", "not found".into()))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn snapshot_loader_rejects_caches_and_restores_host_state() {
        let bridge = PythonBridge::new().unwrap();
        let sources = [PythonSourceV1 {
            module: "cgx2_bridge_fixture_v1".into(), filename: "fixture.py".into(),
            is_package: false,
            source: b"def echo(value):\n    return value\ndef fail(value):\n    raise ValueError('expected')\ndef cached(value):\n    import cgx2_ambient_not_locked\n    return 1\ndef dynamic(value):\n    from importlib import import_module\n    return import_module('cgx2_ambient_not_locked').value\n".to_vec(),
        }];
        let args = [BridgeValue::I64(42)];
        let mut call = PinnedInvocationV1 {
            sources: &sources,
            stdlib_paths: &[],
            module: "cgx2_bridge_fixture_v1",
            attribute: "echo",
            positional: &args,
            keyword: &[],
            array_result: false,
            input_bytes: 0,
        };
        Python::attach(|py| {
            let sys = py.import("sys").unwrap();
            let modules = sys
                .getattr("modules")
                .unwrap()
                .cast_into::<PyDict>()
                .unwrap();
            let path = sys.getattr("path").unwrap();
            let meta = sys.getattr("meta_path").unwrap();
            let bytecode = sys.getattr("dont_write_bytecode").unwrap();
            let importlib = py.import("importlib").unwrap();
            let original_import_module = importlib.getattr("import_module").unwrap();
            modules
                .set_item(call.module, PyModule::new(py, call.module).unwrap())
                .unwrap();
            assert!(
                matches!(bridge.call_pinned_copied_v1(&call), Err(BridgeError::PythonException(e)) if e.exception_type == "ImportError")
            );
            modules.del_item(call.module).unwrap();
            let ambient = PyModule::new(py, "cgx2_ambient_not_locked").unwrap();
            ambient.add("value", 99).unwrap();
            modules
                .set_item("cgx2_ambient_not_locked", ambient)
                .unwrap();
            for attribute in ["echo", "fail", "cached", "dynamic"] {
                call.attribute = attribute;
                let result = bridge.call_pinned_copied_v1(&call);
                if attribute == "echo" {
                    assert!(matches!(result.unwrap(), BridgeValue::I64(42)));
                } else {
                    assert!(matches!(result, Err(BridgeError::PythonException(_))));
                }
                assert!(!modules.contains(call.module).unwrap());
                assert!(sys.getattr("path").unwrap().is(&path));
                assert!(sys.getattr("meta_path").unwrap().is(&meta));
                assert!(sys.getattr("dont_write_bytecode").unwrap().is(&bytecode));
                assert!(
                    importlib
                        .getattr("import_module")
                        .unwrap()
                        .is(&original_import_module)
                );
            }
            modules.del_item("cgx2_ambient_not_locked").unwrap();
        });
    }
}

#[cfg(not(target_os = "linux"))]
fn linked_python_image() -> Result<PathBuf, BridgeError> {
    Err(conversion_error(
        "Linux embedded identity v1",
        "unsupported host".into(),
    ))
}
