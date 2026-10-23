#![cfg(feature = "in-process")]
use cgx2_foreign::package_v1::*;
use cgx2_foreign::{BridgeValue, PythonBridge};
use cgx2_python_bridge::{BridgeError, MAX_VALUE_BYTES};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

static HOST_TEST: Mutex<()> = Mutex::new(());
static FIXTURE_ID: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
    bridge: PythonBridge,
    lock: PackageEnvironmentV1,
    grant: PackageAuthorizationV1,
}

fn pin(path: &Path) -> ArtifactPinV1 {
    ArtifactPinV1 {
        path: path.to_owned(),
        sha256: hash_file_v1(path).unwrap(),
    }
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "cgx2-pinned-package-{}-{}",
            std::process::id(),
            FIXTURE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir(root.join("pinned_demo")).unwrap();
        std::fs::write(
            root.join("pinned_demo/__init__.py"),
            include_str!("fixtures/pinned_demo/__init__.py"),
        )
        .unwrap();
        std::fs::write(
            root.join("METADATA"),
            "Name: pinned-demo\nVersion: 1\nRequires-Dist: pinned-dep\n",
        )
        .unwrap();
        std::fs::write(root.join("dep.METADATA"), "Name: pinned-dep\nVersion: 2\n").unwrap();
        std::fs::write(
            root.join("pinned_dep.py"),
            "def transform(value):\n    return value + 2\n",
        )
        .unwrap();
        // This neighboring but unlocked package must never resolve from cwd/package root.
        std::fs::write(
            root.join("cgx2_not_locked.py"),
            "raise AssertionError('unlocked import ran')\n",
        )
        .unwrap();
        let root = root.canonicalize().unwrap();
        let bridge = PythonBridge::new().unwrap();
        let identity = bridge.embedded_identity_v1().unwrap();
        let source = |module: &str, relative: &str, is_package| SourcePinV1 {
            module: module.into(),
            path: relative.into(),
            sha256: hash_file_v1(&root.join(relative)).unwrap(),
            is_package,
        };
        let mut calls = BTreeMap::new();
        for (key, attribute, positional, returns) in [
            (
                "echo",
                "echo",
                vec![CopiedTypeV1::String],
                CopiedTypeV1::String,
            ),
            (
                "list",
                "echo",
                vec![CopiedTypeV1::List(Box::new(CopiedTypeV1::I64))],
                CopiedTypeV1::List(Box::new(CopiedTypeV1::I64)),
            ),
            (
                "dict",
                "echo",
                vec![CopiedTypeV1::Dict(Box::new(CopiedTypeV1::Bytes))],
                CopiedTypeV1::Dict(Box::new(CopiedTypeV1::Bytes)),
            ),
            (
                "tuple",
                "echo",
                vec![CopiedTypeV1::Tuple(vec![
                    CopiedTypeV1::None,
                    CopiedTypeV1::Bool,
                    CopiedTypeV1::F64,
                ])],
                CopiedTypeV1::Tuple(vec![
                    CopiedTypeV1::None,
                    CopiedTypeV1::Bool,
                    CopiedTypeV1::F64,
                ]),
            ),
            (
                "array",
                "echo",
                vec![CopiedTypeV1::F64Array1dCopyV1],
                CopiedTypeV1::F64Array1dCopyV1,
            ),
            ("wrong", "wrong", vec![], CopiedTypeV1::I64),
            ("fail", "fail", vec![], CopiedTypeV1::None),
            ("large", "large", vec![], CopiedTypeV1::String),
            ("deep", "deep", vec![], CopiedTypeV1::None),
            ("cycle", "cycle", vec![], CopiedTypeV1::None),
            ("nonstring_key", "nonstring_key", vec![], CopiedTypeV1::None),
            ("hostile", "hostile", vec![], CopiedTypeV1::None),
            ("unlisted", "unlisted", vec![], CopiedTypeV1::I64),
            (
                "dependency",
                "dependency",
                vec![CopiedTypeV1::I64],
                CopiedTypeV1::I64,
            ),
        ] {
            calls.insert(
                key.into(),
                CallableV1 {
                    module: "pinned_demo".into(),
                    attribute: attribute.into(),
                    positional,
                    keywords: BTreeMap::new(),
                    returns,
                    effect: "Foreign".into(),
                    capability: "ForeignCall".into(),
                    ownership: "copied".into(),
                },
            );
        }
        let mut keyword = calls["echo"].clone();
        keyword.attribute = "keywords".into();
        keyword
            .keywords
            .insert("suffix".into(), CopiedTypeV1::String);
        calls.insert("keywords".into(), keyword);
        let grant = PackageAuthorizationV1 {
            environment_id: "fixture-v1".into(),
            allowed_calls: calls.keys().cloned().collect(),
        };
        let lock = PackageEnvironmentV1 {
            schema_version: 1,
            environment_id: grant.environment_id.clone(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            runtime: RuntimePinV1 {
                interpreter: pin(&identity.executable),
                library: pin(&identity.library),
                version: identity.version,
                prefix: identity.prefix,
                base_prefix: identity.base_prefix,
                stdlib: vec![],
            },
            distributions: vec![
                DistributionPinV1 {
                    name: "pinned-demo".into(),
                    version: "1".into(),
                    root: root.clone(),
                    metadata: ArtifactPinV1 {
                        path: "METADATA".into(),
                        sha256: hash_file_v1(&root.join("METADATA")).unwrap(),
                    },
                    dependencies: vec!["pinned-dep".into()],
                    sources: vec![source("pinned_demo", "pinned_demo/__init__.py", true)],
                },
                DistributionPinV1 {
                    name: "pinned-dep".into(),
                    version: "2".into(),
                    root: root.clone(),
                    metadata: ArtifactPinV1 {
                        path: "dep.METADATA".into(),
                        sha256: hash_file_v1(&root.join("dep.METADATA")).unwrap(),
                    },
                    dependencies: vec![],
                    sources: vec![source("pinned_dep", "pinned_dep.py", false)],
                },
            ],
            calls,
        };
        Self {
            root,
            bridge,
            lock,
            grant,
        }
    }
    fn invoke(
        &self,
        lock: &PackageEnvironmentV1,
        key: &str,
        args: &[BridgeValue],
    ) -> std::result::Result<BridgeValue, PackageErrorV1> {
        lock.invoke(&self.bridge, &self.grant, key, args, &[])
    }
    fn marker(&self) -> PathBuf {
        self.root.join("pinned_demo/__init__.py.initialized")
    }
    fn assert_no_import(&self, result: std::result::Result<BridgeValue, PackageErrorV1>) {
        assert!(result.is_err(), "invalid preflight unexpectedly passed");
        assert!(
            !self.marker().exists(),
            "package initialized before preflight completed"
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn pinned_package_preflight_copy_failures_and_cleanup() {
    let _exclusive = HOST_TEST.lock().unwrap();
    // One sequential host test: this profile intentionally scopes process-global import state.
    let f = Fixture::new();
    let json = serde_json::to_vec(&f.lock).unwrap();
    PackageEnvironmentV1::from_json(&json).unwrap();
    for modify in [
        |l: &mut PackageEnvironmentV1| l.schema_version = 2,
        |l: &mut PackageEnvironmentV1| l.os = "wrong".into(),
        |l: &mut PackageEnvironmentV1| l.arch = "wrong".into(),
        |l: &mut PackageEnvironmentV1| l.environment_id = "wrong".into(),
        |l: &mut PackageEnvironmentV1| l.runtime.version.push('x'),
        |l: &mut PackageEnvironmentV1| l.runtime.interpreter.path = l.runtime.library.path.clone(),
        |l: &mut PackageEnvironmentV1| l.runtime.library.path = l.runtime.interpreter.path.clone(),
        |l: &mut PackageEnvironmentV1| l.runtime.interpreter.sha256 = "0".repeat(64),
        |l: &mut PackageEnvironmentV1| l.runtime.library.sha256 = "0".repeat(64),
        |l: &mut PackageEnvironmentV1| l.runtime.prefix = l.distributions[0].root.clone(),
        |l: &mut PackageEnvironmentV1| l.runtime.base_prefix = l.distributions[0].root.clone(),
        |l: &mut PackageEnvironmentV1| l.distributions[0].version = "wrong".into(),
        |l: &mut PackageEnvironmentV1| l.distributions[0].metadata.sha256 = "0".repeat(64),
        |l: &mut PackageEnvironmentV1| l.distributions[0].sources[0].sha256 = "0".repeat(64),
        |l: &mut PackageEnvironmentV1| l.distributions[1].sources[0].sha256 = "0".repeat(64),
        |l: &mut PackageEnvironmentV1| l.distributions[0].dependencies.clear(),
        |l: &mut PackageEnvironmentV1| {
            l.distributions.pop();
        },
        |l: &mut PackageEnvironmentV1| l.calls.get_mut("echo").unwrap().module = "unlocked".into(),
        |l: &mut PackageEnvironmentV1| l.calls.get_mut("echo").unwrap().capability = "none".into(),
        |l: &mut PackageEnvironmentV1| {
            l.calls.get_mut("echo").unwrap().ownership = "borrowed".into()
        },
        |l: &mut PackageEnvironmentV1| {
            l.calls.get_mut("echo").unwrap().attribute = "_private".into()
        },
        |l: &mut PackageEnvironmentV1| l.distributions[0].sources[0].path = "../escape.py".into(),
    ] {
        let mut lock = f.lock.clone();
        modify(&mut lock);
        f.assert_no_import(f.invoke(&lock, "echo", &[BridgeValue::String("x".into())]));
    }
    f.assert_no_import(f.invoke(&f.lock, "echo", &[BridgeValue::I64(1)]));
    f.assert_no_import(f.invoke(&f.lock, "echo", &[]));
    let handle = f.bridge.import_module("math").unwrap();
    f.assert_no_import(f.invoke(&f.lock, "echo", &[BridgeValue::Handle(handle)]));
    f.assert_no_import(f.invoke(
        &f.lock,
        "echo",
        &[BridgeValue::String("x".repeat(MAX_VALUE_BYTES + 1))],
    ));
    f.assert_no_import(f.invoke(
        &f.lock,
        "array",
        &[BridgeValue::F64Array1DCopyV1(vec![f64::NAN])],
    ));
    f.assert_no_import(f.invoke(
        &f.lock,
        "list",
        &[BridgeValue::List(vec![BridgeValue::I64(0); 4096])],
    ));
    f.assert_no_import(f.lock.invoke(
        &f.bridge,
        &f.grant,
        "keywords",
        &[BridgeValue::String("x".into())],
        &[("bad", BridgeValue::String("y".into()))],
    ));
    let no_grant = PackageAuthorizationV1 {
        environment_id: f.grant.environment_id.clone(),
        allowed_calls: BTreeSet::new(),
    };
    f.assert_no_import(f.lock.invoke(
        &f.bridge,
        &no_grant,
        "echo",
        &[BridgeValue::String("x".into())],
        &[],
    ));

    let unknown = String::from_utf8(json.clone()).unwrap().replacen(
        "\"schema_version\":1",
        "\"unknown\":1,\"schema_version\":1",
        1,
    );
    assert!(PackageEnvironmentV1::from_json(unknown.as_bytes()).is_err());
    let duplicate = String::from_utf8(json).unwrap().replacen(
        "\"schema_version\":1",
        "\"schema_version\":1,\"schema_version\":1",
        1,
    );
    assert!(PackageEnvironmentV1::from_json(duplicate.as_bytes()).is_err());
    assert!(PackageEnvironmentV1::from_json(&vec![b' '; MAX_PACKAGE_LOCK_BYTES + 1]).is_err());

    // Source/dependency drift on disk rejects before any package initialization.
    std::fs::write(
        f.root.join("pinned_dep.py"),
        "raise AssertionError('drift')\n",
    )
    .unwrap();
    f.assert_no_import(f.invoke(&f.lock, "echo", &[BridgeValue::String("x".into())]));
    std::fs::write(
        f.root.join("pinned_dep.py"),
        "def transform(value):\n    return value + 2\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        std::fs::rename(f.root.join("pinned_dep.py"), f.root.join("dep-real.py")).unwrap();
        std::os::unix::fs::symlink(f.root.join("dep-real.py"), f.root.join("pinned_dep.py"))
            .unwrap();
        f.assert_no_import(f.invoke(&f.lock, "echo", &[BridgeValue::String("x".into())]));
        std::fs::remove_file(f.root.join("pinned_dep.py")).unwrap();
        std::fs::rename(f.root.join("dep-real.py"), f.root.join("pinned_dep.py")).unwrap();
    }
    // Unlisted malicious bytecode is never used by the snapshot loader.
    std::fs::create_dir(f.root.join("pinned_demo/__pycache__")).unwrap();
    std::fs::write(
        f.root.join("pinned_demo/__pycache__/__init__.pyc"),
        b"not bytecode",
    )
    .unwrap();

    assert!(
        matches!(f.invoke(&f.lock, "echo", &[BridgeValue::String("雪".into())]).unwrap(), BridgeValue::String(s) if s == "雪")
    );
    assert!(matches!(
        f.invoke(&f.lock, "dependency", &[BridgeValue::I64(40)])
            .unwrap(),
        BridgeValue::I64(42)
    ));
    assert!(
        matches!(f.invoke(&f.lock, "list", &[BridgeValue::List(vec![BridgeValue::I64(1), BridgeValue::I64(2)])]).unwrap(), BridgeValue::List(v) if v.len() == 2)
    );
    let dict = BTreeMap::from([("雪".into(), BridgeValue::Bytes(vec![0, 255]))]);
    assert!(
        matches!(f.invoke(&f.lock, "dict", &[BridgeValue::Dict(dict)]).unwrap(), BridgeValue::Dict(v) if matches!(&v["雪"], BridgeValue::Bytes(b) if b == &[0,255]))
    );
    assert!(
        matches!(f.invoke(&f.lock, "tuple", &[BridgeValue::Tuple(vec![BridgeValue::None, BridgeValue::Bool(true), BridgeValue::F64(1.5)])]).unwrap(), BridgeValue::Tuple(v) if v.len() == 3)
    );
    assert!(
        matches!(f.lock.invoke(&f.bridge, &f.grant, "keywords", &[BridgeValue::String("a".into())], &[("suffix", BridgeValue::String("b".into()))]).unwrap(), BridgeValue::String(s) if s == "ab")
    );
    assert!(
        matches!(f.invoke(&f.lock, "array", &[BridgeValue::F64Array1DCopyV1(vec![1.0,2.0])]).unwrap(), BridgeValue::F64Array1DCopyV1(v) if v == [1.0,2.0])
    );
    assert!(matches!(
        f.invoke(&f.lock, "wrong", &[]),
        Err(PackageErrorV1::Result(_))
    ));
    assert!(
        matches!(f.invoke(&f.lock, "fail", &[]), Err(PackageErrorV1::Python(BridgeError::PythonException(e))) if e.exception_type == "ValueError" && e.message.contains("fixture exploded"))
    );
    let mut actual_signature = f.lock.clone();
    actual_signature
        .calls
        .get_mut("echo")
        .unwrap()
        .positional
        .clear();
    assert!(
        matches!(f.invoke(&actual_signature, "echo", &[]), Err(PackageErrorV1::Python(BridgeError::PythonException(e))) if e.exception_type == "TypeError")
    );
    let mut absent_attribute = f.lock.clone();
    absent_attribute.calls.get_mut("echo").unwrap().attribute = "absent".into();
    assert!(
        matches!(f.invoke(&absent_attribute, "echo", &[BridgeValue::String("x".into())]), Err(PackageErrorV1::Python(BridgeError::PythonException(e))) if e.exception_type == "AttributeError")
    );
    for key in [
        "large",
        "deep",
        "cycle",
        "nonstring_key",
        "hostile",
        "unlisted",
    ] {
        assert!(
            matches!(f.invoke(&f.lock, key, &[]), Err(PackageErrorV1::Python(_))),
            "{key}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(f.root.join("pinned_demo/__init__.py.released")).unwrap(),
        "R"
    );
    // Repeated calls prove scoped namespace removal; destructor counts prove returned
    // unsupported objects release immediately without waiting for a later bridge call.
    for _ in 0..8 {
        assert!(f.invoke(&f.lock, "hostile", &[]).is_err());
        assert!(matches!(
            f.invoke(&f.lock, "echo", &[BridgeValue::String("again".into())])
                .unwrap(),
            BridgeValue::String(_)
        ));
    }
    assert_eq!(
        std::fs::read_to_string(f.root.join("pinned_demo/__init__.py.released")).unwrap(),
        "R".repeat(9)
    );
    // Recheck pins on every call, including after a previous successful invocation.
    std::fs::write(f.root.join("pinned_dep.py"), "# drift after success\n").unwrap();
    assert!(matches!(
        f.invoke(&f.lock, "echo", &[BridgeValue::String("x".into())]),
        Err(PackageErrorV1::Identity(_))
    ));
}

#[test]
fn isolated_startup_preflight_has_no_site_hooks() {
    if let Some(path) = std::env::var_os("CGX2_PACKAGE_ISOLATED_TEST_LOCK_PATH") {
        let lock = PackageEnvironmentV1::from_json(&std::fs::read(path).unwrap()).unwrap();
        let mut bad = lock.clone();
        bad.runtime.interpreter.sha256 = "0".repeat(64);
        // SAFETY: this child test selects only this test, has not used CPython, and
        // has no concurrent CPython clients. Bad preflight must not initialize it.
        assert!(matches!(
            unsafe { bad.start_isolated() },
            Err(PackageErrorV1::Identity(_))
        ));
        let cache = lock.runtime.stdlib[0].path.join("__pycache__");
        std::fs::create_dir(&cache).unwrap();
        // SAFETY: still before startup; cache rejection must leave CPython uninitialized.
        assert!(matches!(
            unsafe { lock.start_isolated() },
            Err(PackageErrorV1::Identity(_))
        ));
        std::fs::remove_dir(cache).unwrap();
        // SAFETY: bad preflights did not initialize CPython; same exclusive startup.
        let bridge = unsafe { lock.start_isolated() }.unwrap();
        let grant = PackageAuthorizationV1 {
            environment_id: lock.environment_id.clone(),
            allowed_calls: BTreeSet::from(["startup".into()]),
        };
        let value = lock.invoke(&bridge, &grant, "startup", &[], &[]).unwrap();
        assert!(
            matches!(value, BridgeValue::Tuple(v) if matches!(v.as_slice(), [BridgeValue::Bool(true), BridgeValue::Bool(false), BridgeValue::Bool(true)]))
        );
        assert!(!lock.distributions[0].root.join("site-hook-ran").exists());
        return;
    }
    let _exclusive = HOST_TEST.lock().unwrap();
    let mut f = Fixture::new();
    let sysconfig = f.bridge.import_module("sysconfig").unwrap();
    let get_path = f.bridge.getattr(&sysconfig, "get_path").unwrap();
    let path = f
        .bridge
        .call(&get_path, &[BridgeValue::String("stdlib".into())])
        .unwrap();
    let BridgeValue::String(path) = f.bridge.to_value(&path).unwrap() else {
        panic!("stdlib path");
    };
    let stdlib = f.root.join("stdlib");
    copy_stdlib(Path::new(&path), &stdlib);
    f.lock.runtime.stdlib = vec![ArtifactPinV1 {
        path: stdlib.clone(),
        sha256: hash_stdlib_tree_v1(&stdlib).unwrap(),
    }];
    let mut startup = f.lock.calls["wrong"].clone();
    startup.attribute = "startup".into();
    startup.returns = CopiedTypeV1::Tuple(vec![CopiedTypeV1::Bool; 3]);
    f.lock.calls.insert("startup".into(), startup);
    let lock_path = f.root.join("lock.json");
    std::fs::write(&lock_path, serde_json::to_vec(&f.lock).unwrap()).unwrap();
    std::fs::write(
        f.root.join("sitecustomize.py"),
        format!(
            "open({:?}, 'w').write('bad')\n",
            f.root.join("site-hook-ran").to_str().unwrap()
        ),
    )
    .unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "isolated_startup_preflight_has_no_site_hooks",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("CGX2_PACKAGE_ISOLATED_TEST_LOCK_PATH", lock_path)
        .env("PYTHONPATH", &f.root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "isolated child failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn copy_stdlib(source: &Path, destination: &Path) {
    std::fs::create_dir_all(destination).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap();
        if matches!(
            name.to_str(),
            Some("__pycache__" | "site-packages" | "dist-packages")
        ) || matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("pyc" | "pyo")
        ) {
            continue;
        }
        if path.is_dir() {
            copy_stdlib(&path, &destination.join(name));
        } else {
            std::fs::copy(&path, destination.join(name)).unwrap();
        }
    }
}

#[test]
fn sha256_and_stdlib_tree_are_deterministic_and_fail_closed() {
    let root = std::env::temp_dir().join(format!("cgx2-stdlib-pin-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.py"), b"abc").unwrap();
    assert_eq!(
        hash_file_v1(&root.join("a.py")).unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let before = hash_stdlib_tree_v1(&root).unwrap();
    std::fs::create_dir_all(root.join("__pycache__")).unwrap();
    std::fs::write(root.join("__pycache__/ignored.pyc"), "ignored").unwrap();
    assert_eq!(hash_stdlib_tree_v1(&root).unwrap(), before);
    std::fs::write(root.join("a.py"), "changed").unwrap();
    assert_ne!(hash_stdlib_tree_v1(&root).unwrap(), before);
    std::fs::write(root.join("bad.pyc"), "bad").unwrap();
    assert!(hash_stdlib_tree_v1(&root).is_err());
    std::fs::remove_file(root.join("bad.pyc")).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("a.py"), root.join("link.py")).unwrap();
        assert!(hash_stdlib_tree_v1(&root).is_err());
    }
    std::fs::remove_dir_all(root).unwrap();
}
