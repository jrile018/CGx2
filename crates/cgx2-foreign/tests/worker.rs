use cgx2_foreign::{ScalarValue, SourceCall, ValueShape, WorkerConfig, registry_from_source};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Script(PathBuf);
impl Script {
    fn new(code: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "cgx2-worker-{}-{}.py",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, code).unwrap();
        Self(path)
    }
    fn config(&self) -> WorkerConfig {
        WorkerConfig {
            interpreter: "python3".into(),
            worker_path: Some(self.0.clone()),
            timeout: Duration::from_millis(500),
        }
    }
}
impl Drop for Script {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn registry(
    module: &str,
    attribute: &str,
    input: ValueShape,
    output: ValueShape,
) -> cgx2_foreign::Registry {
    registry_from_source(&[SourceCall {
        key: "host",
        module,
        attribute,
        positional: &[input],
        returns: output,
    }])
    .unwrap()
}
fn default_config() -> WorkerConfig {
    WorkerConfig {
        interpreter: "python3".into(),
        worker_path: None,
        timeout: Duration::from_secs(5),
    }
}

#[test]
fn real_worker_scalars_checked_boundaries_and_provenance() {
    let config = default_config();
    let isqrt = registry("math", "isqrt", ValueShape::I64, ValueShape::I64);
    let output = isqrt
        .call_worker(&config, "host", &[ScalarValue::I64(i64::MAX)])
        .unwrap();
    assert_eq!(output.value, ScalarValue::I64(3037000499));
    assert!(PathBuf::from(&output.provenance.executable).is_absolute());
    assert!(output.provenance.version.starts_with("3."));
    assert!(PathBuf::from(&output.provenance.prefix).is_absolute());
    eprintln!("worker provenance: {:?}", output.provenance);
    let text = registry("builtins", "str", ValueShape::String, ValueShape::String);
    let value = ScalarValue::String("héllo\0\n🦀".into());
    assert_eq!(
        text.call_worker(&config, "host", std::slice::from_ref(&value))
            .unwrap()
            .value,
        value
    );
    for n in [i64::MIN, i64::MAX] {
        let text = registry("builtins", "str", ValueShape::I64, ValueShape::String);
        assert_eq!(
            text.call_worker(&config, "host", &[ScalarValue::I64(n)])
                .unwrap()
                .value,
            ScalarValue::String(n.to_string())
        );
    }
    let truth = registry("builtins", "bool", ValueShape::String, ValueShape::Bool);
    assert_eq!(
        truth
            .call_worker(&config, "host", &[ScalarValue::String(String::new())])
            .unwrap()
            .value,
        ScalarValue::Bool(false)
    );
    let factorial = registry("math", "factorial", ValueShape::I64, ValueShape::I64);
    assert_eq!(
        factorial
            .call_worker(&config, "host", &[ScalarValue::I64(21)])
            .unwrap_err()
            .category(),
        "return_conversion"
    );
}

#[test]
fn argument_and_declaration_checks_precede_discovery_and_spawn() {
    let config = WorkerConfig {
        interpreter: "/missing/cgx2-python".into(),
        ..default_config()
    };
    let isqrt = registry("math", "isqrt", ValueShape::I64, ValueShape::I64);
    assert_eq!(
        isqrt
            .call_worker(&config, "absent", &[])
            .unwrap_err()
            .category(),
        "blocked_call"
    );
    assert_eq!(
        isqrt
            .call_worker(&config, "host", &[])
            .unwrap_err()
            .category(),
        "argument"
    );
    assert_eq!(
        isqrt
            .call_worker(&config, "host", &[ScalarValue::Bool(true)])
            .unwrap_err()
            .category(),
        "argument"
    );
    let text = registry("builtins", "str", ValueShape::String, ValueShape::String);
    assert_eq!(
        text.call_worker(
            &config,
            "host",
            &[ScalarValue::String("x".repeat(1_048_577))]
        )
        .unwrap_err()
        .category(),
        "argument"
    );
    let arrays = registry(
        "math",
        "isqrt",
        ValueShape::F64Array1DCopyV1,
        ValueShape::I64,
    );
    assert!(
        arrays
            .call_worker(&config, "host", &[])
            .unwrap_err()
            .to_string()
            .contains("allowlist")
    );
    let broad = registry(
        "sys",
        "getdefaultencoding",
        ValueShape::I64,
        ValueShape::String,
    );
    assert!(
        broad
            .call_worker(&config, "host", &[])
            .unwrap_err()
            .to_string()
            .contains("allowlist")
    );
}

#[test]
fn maximum_string_round_trip_and_shape_errors() {
    let text = registry("builtins", "str", ValueShape::String, ValueShape::String);
    let input = ScalarValue::String("é".repeat(524_288));
    assert_eq!(
        text.call_worker(&default_config(), "host", std::slice::from_ref(&input))
            .unwrap()
            .value,
        input
    );
    for (attribute, n, category) in [
        ("missing_cgx2_callable", 1, "attribute_missing"),
        ("isqrt", -1, "python_exception"),
        ("sqrt", 4, "return_conversion"),
    ] {
        let r = registry("math", attribute, ValueShape::I64, ValueShape::I64);
        assert_eq!(
            r.call_worker(&default_config(), "host", &[ScalarValue::I64(n)])
                .unwrap_err()
                .category(),
            category
        );
    }
}

#[test]
fn crash_timeout_oversize_and_recovery_reap_child() {
    let r = registry("math", "isqrt", ValueShape::I64, ValueShape::I64);
    for behavior in [
        "os._exit(7)",
        "time.sleep(10)",
        "os.write(1, b'x' * 2000000)",
    ] {
        let script = Script::new(&format!(
            "import os,time\nopen(__file__+'.pid','w').write(str(os.getpid()))\n{behavior}\n"
        ));
        let start = Instant::now();
        let error = r
            .call_worker(&script.config(), "host", &[ScalarValue::I64(81)])
            .unwrap_err();
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(
            error.to_string().contains(if behavior.contains("sleep") {
                "timed out"
            } else if behavior.contains("_exit") {
                "unexpected exit"
            } else {
                "frame limit"
            }),
            "{error}"
        );
        let pid_path = script.0.with_extension("py.pid");
        let pid = fs::read_to_string(&pid_path).unwrap();
        assert!(
            !PathBuf::from(format!("/proc/{pid}")).exists(),
            "child must be reaped"
        );
        fs::remove_file(pid_path).unwrap();
        assert_eq!(
            r.call_worker(&default_config(), "host", &[ScalarValue::I64(81)])
                .unwrap()
                .value,
            ScalarValue::I64(9)
        );
    }
}

#[test]
fn missing_worker_interpreter_and_invalid_deadline_are_diagnosed() {
    let r = registry("math", "isqrt", ValueShape::I64, ValueShape::I64);
    let args = [ScalarValue::I64(81)];
    let mut config = default_config();
    config.worker_path = Some(PathBuf::from("/missing/cgx2-worker.py"));
    assert!(
        r.call_worker(&config, "host", &args)
            .unwrap_err()
            .to_string()
            .contains("worker override")
    );
    config.worker_path = None;
    config.interpreter = "/missing/cgx2-python".into();
    assert!(
        r.call_worker(&config, "host", &args)
            .unwrap_err()
            .to_string()
            .contains("cannot launch")
    );
    config.interpreter = "cgx2-no-such-python-472".into();
    assert!(
        r.call_worker(&config, "host", &args)
            .unwrap_err()
            .to_string()
            .contains("unavailable on PATH")
    );
    config.timeout = Duration::from_secs(6);
    assert!(
        r.call_worker(&config, "host", &args)
            .unwrap_err()
            .to_string()
            .contains("timeout must")
    );
}

#[test]
fn valid_frame_followed_by_crash_is_never_published() {
    let r = registry("math", "isqrt", ValueShape::I64, ValueShape::I64);
    let script = Script::new(
        "import os,struct,sys\ndef s(x):\n b=x.encode(); return struct.pack('>I',len(b))+b\nos.write(1,b'CGX2PY01R'+s('cpython')+s(sys.executable)+s(sys.version)+s(sys.prefix)+b'\\0i'+struct.pack('>q',9))\nos._exit(4)\n",
    );
    assert!(
        r.call_worker(&script.config(), "host", &[ScalarValue::I64(81)])
            .unwrap_err()
            .to_string()
            .contains("unexpected exit")
    );
}

#[test]
fn deadline_covers_a_child_that_never_reads_its_request() {
    let script = Script::new("import time\ntime.sleep(10)\n");
    let r = registry("builtins", "str", ValueShape::String, ValueShape::String);
    let start = Instant::now();
    assert!(
        r.call_worker(
            &script.config(),
            "host",
            &[ScalarValue::String("x".repeat(1_048_576))]
        )
        .unwrap_err()
        .to_string()
        .contains("timed out")
    );
    assert!(start.elapsed() < Duration::from_secs(3));
}

#[test]
fn module_stdout_and_raw_fd_writes_are_separate_from_protocol() {
    let prefix = "import sys,types,os\nm=types.ModuleType('math')\ndef noisy(x):\n print('not a protocol frame')\n os.write(1,b'raw module stdout')\n return 9\nm.isqrt=noisy\nsys.modules['math']=m\n";
    let script = Script::new(&format!("{prefix}\n{}", include_str!("../src/worker.py")));
    let r = registry("math", "isqrt", ValueShape::I64, ValueShape::I64);
    assert_eq!(
        r.call_worker(&script.config(), "host", &[ScalarValue::I64(81)])
            .unwrap()
            .value,
        ScalarValue::I64(9)
    );
}

#[test]
fn audited_worker_rejects_malformed_requests_before_import() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    fn string(out: &mut Vec<u8>, s: &[u8]) {
        out.extend_from_slice(&(s.len() as u32).to_be_bytes());
        out.extend_from_slice(s);
    }
    let mut request = b"CGX2PY01Q".to_vec();
    string(&mut request, b"math");
    string(&mut request, b"isqrt");
    request.push(b'i');
    request.extend_from_slice(&1u32.to_be_bytes());
    request.push(b'i');
    request.extend_from_slice(&81i64.to_be_bytes());
    let mut cases = Vec::new();
    for cut in [0, 8, 13, request.len() - 1] {
        cases.push(request[..cut].to_vec());
    }
    let mut trailing = request.clone();
    trailing.push(0);
    cases.push(trailing);
    let mut wrong_version = request.clone();
    wrong_version[7] = b'2';
    cases.push(wrong_version);
    let mut bad_utf8 = request.clone();
    bad_utf8[13] = 255;
    cases.push(bad_utf8);
    let mut bad_tag = request.clone();
    let index = bad_tag.len() - 9;
    bad_tag[index] = b'x';
    cases.push(bad_tag);
    let mut bad_bool = request.clone();
    let index = bad_bool.len() - 9;
    bad_bool[index] = b'b';
    bad_bool.truncate(index + 2);
    bad_bool[index + 1] = 2;
    cases.push(bad_bool);
    let mut oversized_string = request.clone();
    oversized_string[9..13].copy_from_slice(&u32::MAX.to_be_bytes());
    cases.push(oversized_string);
    for raw in cases {
        let mut child = Command::new("python3")
            .args(["-I", "-S", "-c", include_str!("../src/worker.py")])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&raw).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        assert!(
            output.stdout.is_empty(),
            "malformed request must publish nothing"
        );
    }
}

#[cfg(feature = "in-process")]
#[test]
fn worker_preserves_host_error_categories_for_existing_source_subset() {
    use cgx2_foreign::{BridgeValue, PythonBridge};
    let bridge = PythonBridge::new().unwrap();
    for (attribute, n) in [
        ("missing_cgx2_callable", 1),
        ("isqrt", -1),
        ("sqrt", 4),
        ("factorial", 21),
    ] {
        let r = registry("math", attribute, ValueShape::I64, ValueShape::I64);
        let host = r
            .call(&bridge, "host", &[BridgeValue::I64(n)], &[])
            .unwrap_err();
        let worker = r
            .call_worker(&default_config(), "host", &[ScalarValue::I64(n)])
            .unwrap_err();
        assert_eq!(worker.category(), host.category());
    }
}
