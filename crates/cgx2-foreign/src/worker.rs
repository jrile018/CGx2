//! One-shot copied-scalar CPython worker. This module never references PyO3 values.
use crate::declaration::MAX_DECLARATION_BYTES;
use crate::{ForeignError, Registry, ValueShape};
use cgx2_python_bridge::{
    MAX_PYTHON_FAILURE_BYTES, MAX_VALUE_BYTES, MAX_VALUE_ELEMENTS, PythonFailure,
};
use std::{ffi::OsString, path::PathBuf, time::Duration};

const MAGIC: &[u8] = b"CGX2PY01";
pub const MAX_FRAME_BYTES: usize = MAX_VALUE_BYTES + 131_072;
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const SCRIPT: &str = include_str!("worker.py");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScalarValue {
    I64(i64),
    Bool(bool),
    String(String),
}
impl ScalarValue {
    fn shape(&self) -> ValueShape {
        match self {
            Self::I64(_) => ValueShape::I64,
            Self::Bool(_) => ValueShape::Bool,
            Self::String(_) => ValueShape::String,
        }
    }
}

/// Local, unpinned identity reported by the selected CPython worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonProvenance {
    pub executable: String,
    pub version: String,
    pub prefix: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerOutput {
    pub value: ScalarValue,
    pub provenance: PythonProvenance,
}

/// Default interpreter: `CGX2_PYTHON`, otherwise `python3` on PATH. No retries/fallback.
/// The audited script is embedded; `CGX2_PYTHON_WORKER` explicitly selects a file instead.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub interpreter: OsString,
    pub worker_path: Option<PathBuf>,
    pub timeout: Duration,
}
impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            interpreter: std::env::var_os("CGX2_PYTHON").unwrap_or_else(|| "python3".into()),
            worker_path: std::env::var_os("CGX2_PYTHON_WORKER").map(PathBuf::from),
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

impl Registry {
    /// A deliberately narrower route than `call`: only the existing source scalar subset.
    /// Declaration, argument shapes and budgets are checked before starting any process.
    pub fn call_worker(
        &self,
        config: &WorkerConfig,
        key: &str,
        args: &[ScalarValue],
    ) -> Result<WorkerOutput, ForeignError> {
        let call = self
            .get(key)
            .ok_or_else(|| ForeignError::BlockedCall { key: key.into() })?;
        let scalar = |shape| {
            matches!(
                shape,
                ValueShape::I64 | ValueShape::Bool | ValueShape::String
            )
        };
        let allowed = (call.module() == "math"
            && call.attribute().len() == 1
            && !call.attribute()[0].starts_with('_'))
            || (call.module() == "builtins"
                && call.attribute().len() == 1
                && matches!(call.attribute()[0].as_str(), "str" | "bool"));
        if !allowed
            || call.ownership() != "copied"
            || !call.keywords().is_empty()
            || !scalar(call.returns())
            || !call.positional().iter().copied().all(scalar)
        {
            return Err(ForeignError::Declaration(
                "worker requires the copied scalar source allowlist".into(),
            ));
        }
        if args.len() != call.positional().len() || args.len() > MAX_VALUE_ELEMENTS {
            return Err(ForeignError::Argument(
                "worker positional argument count mismatch or limit".into(),
            ));
        }
        let mut bytes = 0usize;
        for (arg, expected) in args.iter().zip(call.positional()) {
            if arg.shape() != *expected {
                return Err(ForeignError::Argument(format!(
                    "expected {}; got {}",
                    expected.as_str(),
                    arg.shape().as_str()
                )));
            }
            if let ScalarValue::String(text) = arg {
                bytes = bytes
                    .checked_add(text.len())
                    .ok_or_else(|| protocol("argument size overflow"))?;
                if bytes > MAX_VALUE_BYTES {
                    return Err(ForeignError::Argument(
                        "copied payload exceeds 1048576 bytes".into(),
                    ));
                }
            }
        }
        let mut request = MAGIC.to_vec();
        request.push(b'Q');
        put_string(&mut request, call.module());
        put_string(&mut request, &call.attribute()[0]);
        request.push(shape_tag(call.returns())?);
        request.extend_from_slice(&(args.len() as u32).to_be_bytes());
        for value in args {
            encode_value(&mut request, value);
        }
        if request.len() > MAX_FRAME_BYTES {
            return Err(protocol("request exceeds frame limit"));
        }
        // No interpreter discovery, execution or import occurs until all validation above succeeds.
        if config.timeout.is_zero() || config.timeout > DEFAULT_TIMEOUT {
            return Err(protocol("timeout must be positive and at most 5 seconds"));
        }
        let interpreter = resolve_interpreter(&config.interpreter)?;
        let response = exchange(config, &interpreter, &request)?;
        decode_response(&response, &interpreter, call.returns())
    }
}

fn protocol(message: &str) -> ForeignError {
    ForeignError::Declaration(format!("Python worker: {message}"))
}
fn shape_tag(shape: ValueShape) -> Result<u8, ForeignError> {
    match shape {
        ValueShape::I64 => Ok(b'i'),
        ValueShape::Bool => Ok(b'b'),
        ValueShape::String => Ok(b's'),
        _ => Err(protocol("unsupported scalar shape")),
    }
}
fn put_string(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(&(text.len() as u32).to_be_bytes());
    out.extend_from_slice(text.as_bytes());
}
fn encode_value(out: &mut Vec<u8>, value: &ScalarValue) {
    match value {
        ScalarValue::I64(n) => {
            out.push(b'i');
            out.extend_from_slice(&n.to_be_bytes());
        }
        ScalarValue::Bool(b) => {
            out.push(b'b');
            out.push(u8::from(*b));
        }
        ScalarValue::String(s) => {
            out.push(b's');
            put_string(out, s);
        }
    }
}
fn resolve_interpreter(name: &OsString) -> Result<PathBuf, ForeignError> {
    let path = PathBuf::from(name);
    if path.is_absolute() || path.components().count() > 1 {
        return std::path::absolute(path).map_err(|e| protocol(&format!("interpreter path: {e}")));
    }
    for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let candidate = directory.join(&path);
        if candidate.is_file() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if candidate
                    .metadata()
                    .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
                {
                    return std::path::absolute(candidate).map_err(|e| protocol(&e.to_string()));
                }
            }
            #[cfg(not(unix))]
            return Ok(candidate);
        }
    }
    Err(protocol(&format!(
        "interpreter {:?} unavailable on PATH",
        name
    )))
}

// Linux x86_64/aarch64 fcntl ABI. Kept local to avoid changing the shared lockfile.
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn nonblocking(pipe: &impl std::os::fd::AsRawFd) -> std::io::Result<()> {
    unsafe extern "C" {
        fn fcntl(fd: std::ffi::c_int, cmd: std::ffi::c_int, ...) -> std::ffi::c_int;
    }
    // SAFETY: the borrowed pipe keeps fd live; F_GETFL takes no argument and
    // F_SETFL takes an int. Only O_NONBLOCK is added to the existing flags.
    unsafe {
        let flags = fcntl(pipe.as_raw_fd(), 3);
        if flags < 0 || fcntl(pipe.as_raw_fd(), 4, flags | 0x800) < 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn exchange(
    config: &WorkerConfig,
    interpreter: &std::path::Path,
    request: &[u8],
) -> Result<Vec<u8>, ForeignError> {
    use std::{
        io::{ErrorKind, Read, Write},
        process::{Command, Stdio},
        time::Instant,
    };
    let io_error = |e| protocol(&format!("transport: {e}"));
    let (child_input, input) = std::io::pipe().map_err(io_error)?;
    let (mut output, child_output) = std::io::pipe().map_err(io_error)?;
    nonblocking(&input).map_err(io_error)?;
    nonblocking(&output).map_err(io_error)?;
    let mut input = Some(input);
    let mut command = Command::new(interpreter);
    command.args(["-I", "-S"]);
    if let Some(path) = &config.worker_path {
        if !path.is_absolute() || !path.is_file() {
            return Err(protocol(
                "worker override must be an existing absolute file",
            ));
        }
        command.arg(path);
    } else {
        command.args(["-c", SCRIPT]);
    }
    command
        .stdin(Stdio::from(child_input))
        .stdout(Stdio::from(child_output))
        .stderr(Stdio::null());
    let deadline = Instant::now()
        .checked_add(config.timeout)
        .ok_or_else(|| protocol("invalid deadline"))?;
    let mut child = command
        .spawn()
        .map_err(|e| protocol(&format!("cannot launch interpreter: {e}")))?;
    drop(command);
    // Every return after spawn passes this cleanup, including I/O/protocol failures and timeout.
    let result = (|| {
        let mut sent = 0;
        let mut eof = false;
        let mut response = Vec::new();
        loop {
            if Instant::now() >= deadline {
                return Err(protocol("execution timed out"));
            }
            if let Some(pipe) = &mut input {
                match pipe.write(&request[sent..]) {
                    Ok(n) => sent += n,
                    Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                    Err(e) => return Err(io_error(e)),
                }
                if sent == request.len() {
                    input = None;
                }
            }
            // Bound each iteration so a continuously writing child cannot starve the deadline.
            for _ in 0..32 {
                let mut buffer = [0; 8192];
                match output.read(&mut buffer) {
                    Ok(0) => {
                        eof = true;
                        break;
                    }
                    Ok(n) => {
                        if response.len() + n > MAX_FRAME_BYTES {
                            return Err(protocol("response exceeds frame limit"));
                        }
                        response.extend_from_slice(&buffer[..n]);
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) => return Err(io_error(e)),
                }
            }
            if let Some(status) = child.try_wait().map_err(io_error)? {
                if !status.success() {
                    return Err(protocol(&format!("unexpected exit {status}")));
                }
                // Drain remaining buffered bytes before checking EOF on the next iteration.
                if eof {
                    return Ok(response);
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    })();
    if result.is_err() {
        let _ = child.kill();
    }
    let reaped = child.wait();
    match result {
        Err(error) => Err(error),
        Ok(response) => {
            reaped.map_err(io_error)?;
            Ok(response)
        }
    }
}
#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
fn exchange(_: &WorkerConfig, _: &std::path::Path, _: &[u8]) -> Result<Vec<u8>, ForeignError> {
    Err(protocol(
        "bounded worker transport currently requires Linux x86_64/aarch64",
    ))
}

struct Decoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Decoder<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], ForeignError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| protocol("length overflow"))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| protocol("truncated frame"))?;
        self.offset = end;
        Ok(bytes)
    }
    fn byte(&mut self) -> Result<u8, ForeignError> {
        Ok(self.take(1)?[0])
    }
    fn string(&mut self, max: usize) -> Result<String, ForeignError> {
        let len = u32::from_be_bytes(self.take(4)?.try_into().expect("length")) as usize;
        if len > max {
            return Err(protocol("string exceeds byte limit"));
        }
        String::from_utf8(self.take(len)?.to_vec()).map_err(|_| protocol("invalid UTF-8"))
    }
    fn value(&mut self) -> Result<ScalarValue, ForeignError> {
        match self.byte()? {
            b'i' => Ok(ScalarValue::I64(i64::from_be_bytes(
                self.take(8)?.try_into().expect("i64"),
            ))),
            b'b' => match self.byte()? {
                0 => Ok(ScalarValue::Bool(false)),
                1 => Ok(ScalarValue::Bool(true)),
                _ => Err(protocol("invalid bool tag")),
            },
            b's' => Ok(ScalarValue::String(self.string(MAX_VALUE_BYTES)?)),
            _ => Err(protocol("invalid value tag")),
        }
    }
    fn failure(&mut self) -> Result<PythonFailure, ForeignError> {
        let exception_type = self.string(256)?;
        let message = self.string(4096)?;
        let traceback = self.string(MAX_PYTHON_FAILURE_BYTES)?;
        if exception_type.is_empty()
            || exception_type.len() + message.len() + traceback.len() > MAX_PYTHON_FAILURE_BYTES
        {
            return Err(protocol("invalid exception metadata budget"));
        }
        Ok(PythonFailure {
            exception_type,
            message,
            traceback,
        })
    }
}
fn decode_response(
    bytes: &[u8],
    interpreter: &std::path::Path,
    expected: ValueShape,
) -> Result<WorkerOutput, ForeignError> {
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(protocol("response exceeds frame limit"));
    }
    let mut d = Decoder { bytes, offset: 0 };
    if d.take(MAGIC.len())? != MAGIC || d.byte()? != b'R' {
        return Err(protocol("wrong worker identity/version"));
    }
    if d.string(32)? != "cpython" {
        return Err(protocol("worker interpreter must be CPython"));
    }
    let provenance = PythonProvenance {
        executable: d.string(4096)?,
        version: d.string(4096)?,
        prefix: d.string(4096)?,
    };
    let executable = std::path::Path::new(&provenance.executable);
    let same_executable = executable.is_absolute()
        && std::fs::canonicalize(executable)
            .ok()
            .zip(std::fs::canonicalize(interpreter).ok())
            .is_some_and(|(a, b)| a == b);
    let version = provenance.version.split_whitespace().next().unwrap_or("");
    let mut parts = version.split('.');
    let valid_version = parts.next() == Some("3")
        && parts
            .next()
            .is_some_and(|minor| minor.parse::<u16>().is_ok())
        && parts
            .next()
            .is_some_and(|patch| patch.starts_with(|c: char| c.is_ascii_digit()));
    if !same_executable || !valid_version || !std::path::Path::new(&provenance.prefix).is_absolute()
    {
        return Err(protocol("wrong interpreter identity/provenance"));
    }
    let result = match d.byte()? {
        0 => {
            let value = d.value()?;
            if value.shape() != expected {
                return Err(protocol("response shape differs from declaration"));
            }
            Ok(WorkerOutput { value, provenance })
        }
        1 => Err(ForeignError::ModuleUnavailable {
            module: d.string(MAX_DECLARATION_BYTES)?,
            failure: d.failure()?,
        }),
        2 => Err(ForeignError::AttributeMissing {
            attribute: d.string(MAX_DECLARATION_BYTES)?,
            failure: d.failure()?,
        }),
        3 => Err(ForeignError::PythonException(d.failure()?)),
        4 => Err(ForeignError::ReturnConversion {
            expected: expected.as_str().into(),
            actual: d.string(4096)?,
            failure: None,
        }),
        _ => return Err(protocol("invalid response status")),
    };
    if d.offset != bytes.len() {
        return Err(protocol("trailing response payload"));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame() -> (Vec<u8>, PathBuf) {
        let executable = std::env::current_exe().unwrap();
        let mut out = MAGIC.to_vec();
        out.push(b'R');
        for text in ["cpython", executable.to_str().unwrap(), "3.14.0", "/usr"] {
            put_string(&mut out, text);
        }
        (out, executable)
    }
    #[test]
    fn decoder_rejects_all_truncations_trailing_bytes_tags_utf8_identity_and_budgets() {
        let (mut good, exe) = frame();
        good.push(0);
        encode_value(&mut good, &ScalarValue::String("héllo".into()));
        assert_eq!(
            decode_response(&good, &exe, ValueShape::String)
                .unwrap()
                .value,
            ScalarValue::String("héllo".into())
        );
        for cut in 0..good.len() {
            assert!(
                decode_response(&good[..cut], &exe, ValueShape::String).is_err(),
                "cut {cut}"
            );
        }
        let mut bad = good.clone();
        bad.push(0);
        assert!(
            decode_response(&bad, &exe, ValueShape::String)
                .unwrap_err()
                .to_string()
                .contains("trailing")
        );
        for index in [0, 7, 8, 13] {
            let mut bad = good.clone();
            bad[index] = b'X';
            assert!(decode_response(&bad, &exe, ValueShape::String).is_err());
        }
        let (prefix, _) = frame();
        for suffix in [
            vec![99],
            vec![0, b'z'],
            vec![0, b'b', 2],
            vec![0, b's', 0, 0, 0, 1, 255],
            vec![0, b's', 255, 255, 255, 255],
        ] {
            let mut bad = prefix.clone();
            bad.extend(suffix);
            assert!(decode_response(&bad, &exe, ValueShape::String).is_err());
        }
        assert!(
            decode_response(
                &good,
                std::path::Path::new("/missing/python"),
                ValueShape::String
            )
            .is_err()
        );
        assert!(decode_response(&good, &exe, ValueShape::Bool).is_err());
        assert!(decode_response(&vec![0; MAX_FRAME_BYTES + 1], &exe, ValueShape::String).is_err());
    }
    #[test]
    fn provenance_rejects_wrong_implementation_version_and_prefix() {
        let executable = std::env::current_exe().unwrap();
        for (implementation, version, prefix) in [
            ("pypy", "3.14.0", "/usr"),
            ("cpython", "2.7.0", "/usr"),
            ("cpython", "3.fake", "/usr"),
            ("cpython", "3.14.0", "relative"),
        ] {
            let mut bytes = MAGIC.to_vec();
            bytes.push(b'R');
            for text in [
                implementation,
                executable.to_str().unwrap(),
                version,
                prefix,
            ] {
                put_string(&mut bytes, text);
            }
            bytes.push(0);
            encode_value(&mut bytes, &ScalarValue::I64(9));
            assert!(decode_response(&bytes, &executable, ValueShape::I64).is_err());
        }
    }
    #[test]
    fn every_error_class_requires_complete_bounded_frame() {
        let (prefix, exe) = frame();
        for (status, category) in [
            (1, "module_unavailable"),
            (2, "attribute_missing"),
            (3, "python_exception"),
            (4, "return_conversion"),
        ] {
            let mut frame = prefix.clone();
            frame.push(status);
            if matches!(status, 1 | 2 | 4) {
                put_string(&mut frame, "missing");
            }
            if status != 4 {
                for s in ["ValueError", "bad input", "traceback"] {
                    put_string(&mut frame, s);
                }
            }
            assert_eq!(
                decode_response(&frame, &exe, ValueShape::I64)
                    .unwrap_err()
                    .category(),
                category
            );
            frame.push(0);
            assert!(
                decode_response(&frame, &exe, ValueShape::I64)
                    .unwrap_err()
                    .to_string()
                    .contains("trailing")
            );
        }
    }
}
