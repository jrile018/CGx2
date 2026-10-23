use cgx2_bind::{Binding, BindingLock, render_lock, sdk_v2::native_facade_symbols};
use cgx2_ffi_core::{Error as ValueError, ForeignValue, Limits};
use cgx2_ffi_native::{
    Library, LibraryDeclaration,
    sdk_v2::{Error, Sdk},
};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Package(PathBuf);
impl Package {
    fn new(flags: &[&str]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "cgx2-sdk-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).unwrap();
        let artifact = root.join("libsdk.so");
        assert!(
            Command::new("c++")
                .args([
                    "-std=c++17",
                    "-shared",
                    "-fPIC",
                    "-Wall",
                    "-Wextra",
                    "-Werror"
                ])
                .args(flags)
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sdk.cpp"))
                .arg("-o")
                .arg(&artifact)
                .status()
                .unwrap()
                .success()
        );
        std::fs::write(root.join("src/main.cg"), "module sdk_fixture\n").unwrap();
        std::fs::write(root.join("cgx2.toml"), "schema_version = 2\nname = \"sdk_fixture\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\nbinding_lock = \"cgx2-bind.lock.json\"\n").unwrap();
        let binding = Binding {
            id: "sdk".into(),
            route: "native-c".into(),
            library: "sdk_fixture".into(),
            version: "1.0.0".into(),
            artifact: "libsdk.so".into(),
            sha256: format!("{:x}", Sha256::digest(std::fs::read(artifact).unwrap())),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            abi: "c".into(),
            calling_convention: "c".into(),
            layout: "native-sdk-v2".into(),
            symbols: native_facade_symbols(),
        };
        std::fs::write(
            root.join("cgx2-bind.lock.json"),
            render_lock(&BindingLock {
                schema_version: 3,
                package: "sdk_fixture".into(),
                bindings: vec![binding],
            })
            .unwrap(),
        )
        .unwrap();
        Self(root)
    }
    fn open(&self) -> Result<Sdk, Error> {
        // SAFETY: sdk.cpp is reviewed against sdk_v2.h, flags only inject tested failures.
        unsafe { Sdk::open_pinned(&self.0, "sdk") }
    }
    fn edit(&self, body: impl FnOnce(&mut serde_json::Value)) {
        let path = self.0.join("cgx2-bind.lock.json");
        let mut value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        body(&mut value);
        std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    }
}
impl Drop for Package {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn copied_outputs_bounds_utf8_and_exception() {
    let package = Package::new(&[]);
    let sdk = package.open().unwrap();
    let limits = Limits {
        max_bytes: 64,
        ..Limits::default()
    };
    assert_eq!(
        sdk.copy("hé\0".as_bytes(), true, limits).unwrap(),
        ForeignValue::Utf8("hé\0hé\0".into())
    );
    assert_eq!(
        sdk.copy(&[0, 255], false, limits).unwrap(),
        ForeignValue::Bytes(vec![0, 255, 0, 255])
    );
    assert_eq!(
        sdk.copy(&[], true, limits).unwrap(),
        ForeignValue::Utf8(String::new())
    );
    assert!(matches!(
        sdk.copy(b"~", true, limits),
        Err(Error::Value(ValueError::InvalidUtf8))
    ));
    assert!(matches!(
        sdk.copy(b"?", false, limits),
        Err(Error::Value(ValueError::Limit))
    ));
    assert!(matches!(
        sdk.copy(&[0; 65], false, limits),
        Err(Error::Value(ValueError::Limit))
    ));
    assert!(matches!(
        sdk.copy(&[0; 33], false, limits),
        Err(Error::Value(ValueError::ForeignFailure(3)))
    ));
    assert!(matches!(
        sdk.copy(b"!", false, limits),
        Err(Error::Value(ValueError::ForeignFailure(17)))
    ));
    assert_eq!(
        sdk.copy(b"ok", false, limits).unwrap(),
        ForeignValue::Bytes(b"okok".to_vec())
    );
}

#[test]
fn unique_shared_error_cleanup_and_unwind() {
    let package = Package::new(&[]);
    let sdk = package.open().unwrap();
    // SAFETY: reviewed test-only statistics exports, retained DSO for counters.
    let library = unsafe {
        Library::open(
            &package.0.join("libsdk.so"),
            &LibraryDeclaration::new("sdk_fixture", "1.0.0"),
        )
    }
    .unwrap();
    let live: unsafe extern "C" fn() -> i32 = unsafe { library.symbol("sdk_test_live") }.unwrap();
    let releases: unsafe extern "C" fn() -> i32 =
        unsafe { library.symbol("sdk_test_releases") }.unwrap();
    let mut owner = sdk.create(42).unwrap();
    assert!(matches!(sdk.create(-8), Err(Error::Alias)));
    let second_adapter = package.open().unwrap();
    assert!(matches!(second_adapter.create(-8), Err(Error::Alias)));
    drop(second_adapter);
    assert_eq!(owner.read().unwrap(), 42);
    owner.release().unwrap();
    assert!(matches!(owner.release(), Err(Error::AlreadyReleased)));
    assert!(matches!(owner.read(), Err(Error::AlreadyReleased)));
    assert!(matches!(
        sdk.create(-7),
        Err(Error::Value(ValueError::ForeignFailure(17)))
    ));
    assert_eq!(unsafe { live() }, 0);
    let mut consumed = sdk.create(-9).unwrap();
    assert!(matches!(
        consumed.release(),
        Err(Error::Value(ValueError::ForeignFailure(9)))
    ));
    drop(consumed);
    let shared = sdk.create(81).unwrap().into_shared();
    let peer = shared.clone();
    drop(shared);
    drop(sdk);
    assert_eq!(peer.read().unwrap(), 81);
    assert_eq!(unsafe { live() }, 1);
    drop(peer);
    assert_eq!(unsafe { live() }, 0);
    assert_eq!(unsafe { releases() }, 4);
    let sdk = package.open().unwrap();
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _handle = sdk.create(10).unwrap();
        panic!("caller unwind drops handle");
    }));
    assert_eq!(unsafe { live() }, 0);
}

#[test]
fn preflight_and_runtime_identity_reject_before_calls() {
    for flags in [
        vec!["-DSDK_VERSION=3"],
        vec!["-DSDK_ABI=9"],
        vec!["-DSDK_ID=\"wrong\""],
        vec!["-DMISSING_UTF8"],
    ] {
        assert!(Package::new(&flags).open().is_err());
    }
    for field in ["sha256", "layout", "arch", "abi"] {
        let package = Package::new(&[]);
        package.edit(|v| v["bindings"][0][field] = "wrong".into());
        assert!(matches!(package.open(), Err(Error::Preflight(_))));
    }
    let package = Package::new(&[]);
    package.edit(|v| v["schema_version"] = 2.into());
    assert!(matches!(package.open(), Err(Error::Preflight(_))));
    package.edit(|v| {
        v["schema_version"] = 3.into();
        v["bindings"][0]["symbols"][0]["parameters"] = serde_json::json!(["i64"]);
    });
    assert!(matches!(package.open(), Err(Error::Preflight(_))));
}

#[test]
fn changed_inode_is_rejected_even_when_hash_still_matches() {
    let package = Package::new(&[]);
    let sdk = package.open().unwrap();
    let original = package.0.join("libsdk.so");
    let replacement = package.0.join("replacement.so");
    std::fs::copy(&original, &replacement).unwrap();
    std::fs::rename(&replacement, &original).unwrap();
    assert!(matches!(
        sdk.copy(b"input", false, Limits::default()),
        Err(Error::Pin(_))
    ));
}
