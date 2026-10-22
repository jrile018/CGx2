use cgx2_ffi_native::{
    AbiType, Declaration, Error, HandleDeclaration, Library, LibraryDeclaration, Ownership,
};
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture() -> PathBuf {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tiny.c");
    let output = std::env::temp_dir().join(format!(
        "cgx2-tiny-{}-{}.so",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let status = Command::new("cc")
        .args([
            "-std=c11", "-fPIC", "-shared", "-Wall", "-Wextra", "-Werror",
        ])
        .arg(source)
        .arg("-o")
        .arg(&output)
        .status()
        .expect("C compiler");
    assert!(status.success());
    output
}

fn lib(path: &Path) -> Library {
    // SAFETY: this fixture is compiled from tiny.c with the reviewed metadata exports.
    unsafe { Library::open(path, &LibraryDeclaration::new("cgx2.test.tiny", "1.0.0")) }.unwrap()
}

#[test]
fn checked_scalar_bytes_and_errors() {
    // SAFETY: every selected fixture symbol has the signature in tiny.c.
    unsafe {
        let path = fixture();
        let lib = lib(&path);
        let scalar = lib.bind(&Declaration::scalar("tiny_double")).unwrap();
        assert_eq!(scalar.call_i64(21).unwrap(), 42);
        assert_eq!(scalar.call_i64(-1), Err(Error::ForeignStatus(17)));
        let bytes = lib.bind(&Declaration::bytes("tiny_copy")).unwrap();
        assert_eq!(
            bytes.copy_bytes("héllo".as_bytes()).unwrap().value,
            "héllo".as_bytes()
        );
        assert_eq!(bytes.copy_bytes(b"abc").unwrap().copy.bytes, 3);
        assert_eq!(bytes.copy_bytes(b"abc").unwrap().copy.host_allocations, 1);
        assert_eq!(
            lib.bind(&Declaration::scalar("absent")).err(),
            Some(Error::MissingSymbol("absent".into()))
        );
        assert_eq!(
            lib.bind(&Declaration::scalar("tiny_bad_result"))
                .unwrap()
                .call_i64(1),
            Err(Error::ForeignStatus(23))
        );
        assert_eq!(
            Library::open(&path, &LibraryDeclaration::new("cgx2.test.tiny", "2.0.0")).err(),
            Some(Error::VersionMismatch)
        );
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn invalid_declarations_fail_before_call() {
    // SAFETY: valid fixture symbols have reviewed signatures; invalid declarations
    // are rejected before resolving or invoking a mismatched function pointer.
    unsafe {
        let path = fixture();
        let lib = lib(&path);
        let mut wrong = Declaration::scalar("tiny_double");
        wrong.calling_convention = "stdcall";
        assert_eq!(lib.bind(&wrong).err(), Some(Error::UnsupportedDeclaration));
        wrong = Declaration::scalar("tiny_double");
        wrong.parameters = vec![AbiType::I32];
        assert_eq!(lib.bind(&wrong).err(), Some(Error::UnsupportedDeclaration));
        wrong = Declaration::scalar("tiny_double");
        wrong.ownership = Ownership::ForeignOwned;
        assert_eq!(lib.bind(&wrong).err(), Some(Error::UnsupportedDeclaration));
        let mut platform = LibraryDeclaration::new("cgx2.test.tiny", "1.0.0");
        platform.target_arch = "aarch64";
        assert_eq!(
            Library::open(&path, &platform).err(),
            Some(Error::UnsupportedPlatform)
        );
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn handle_release_and_failure_cleanup() {
    // SAFETY: tiny.c defines each selected create/read/release signature.
    unsafe {
        let path = fixture();
        let lib = lib(&path);
        let live = lib.bind(&Declaration::scalar("tiny_live")).unwrap();
        let mut handle = lib
            .bind_handle(&HandleDeclaration::new(
                "tiny_create",
                "tiny_read",
                "tiny_release",
            ))
            .unwrap()
            .create(7)
            .unwrap();
        assert_eq!(live.call_i64(0).unwrap(), 1);
        assert_eq!(handle.read().unwrap(), 7);
        handle.release().unwrap();
        assert_eq!(handle.release(), Err(Error::AlreadyReleased));
        assert_eq!(live.call_i64(0).unwrap(), 0);
        let mut retry = lib
            .bind_handle(&HandleDeclaration::new(
                "tiny_create",
                "tiny_read",
                "tiny_release_retry",
            ))
            .unwrap()
            .create(99)
            .unwrap();
        assert_eq!(retry.release(), Err(Error::ForeignStatus(24)));
        assert_eq!(retry.read().unwrap(), 99);
        assert_eq!(live.call_i64(0).unwrap(), 1);
        retry.release().unwrap();
        assert_eq!(live.call_i64(0).unwrap(), 0);
        {
            let _dropped = lib
                .bind_handle(&HandleDeclaration::new(
                    "tiny_create",
                    "tiny_read",
                    "tiny_release",
                ))
                .unwrap()
                .create(8)
                .unwrap();
        }
        assert_eq!(live.call_i64(0).unwrap(), 0);
        let failed = lib
            .bind_handle(&HandleDeclaration::new(
                "tiny_create_partial_failure",
                "tiny_read",
                "tiny_release",
            ))
            .unwrap()
            .create(8);
        assert_eq!(failed.err(), Some(Error::ForeignStatus(20)));
        assert_eq!(live.call_i64(0).unwrap(), 0);
        assert_eq!(
            lib.bind_handle(&HandleDeclaration::new(
                "tiny_create",
                "tiny_read",
                "absent"
            ))
            .err(),
            Some(Error::MissingSymbol("absent".into()))
        );
        std::fs::remove_file(path).unwrap();
    }
}
