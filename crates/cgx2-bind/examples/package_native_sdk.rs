//! Offline fixture packaging: no source imports or installations.
use cgx2_bind::{render_lock, sdk_v2::native_facade_symbols, Binding, BindingLock};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::PathBuf;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: package_native_sdk ARTIFACT PACKAGE LIBRARY EXACT_VERSION".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&args[0])?
        .take(256 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 256 * 1024 * 1024 {
        return Err("artifact exceeds 256 MiB".into());
    }
    let root = PathBuf::from(&args[1]);
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::write(root.join("libsdk.so"), &bytes)?;
    std::fs::write(root.join("src/main.cg"), "module native_sdk_example\n")?;
    std::fs::write(root.join("cgx2.toml"), "schema_version = 2\nname = \"native_sdk_example\"\nentry = \"src/main.cg\"\nsources = [\"src/main.cg\"]\nbinding_lock = \"cgx2-bind.lock.json\"\n")?;
    let binding = Binding {
        id: "sdk".into(),
        route: "native-c".into(),
        library: args[2].clone(),
        version: args[3].clone(),
        artifact: "libsdk.so".into(),
        sha256: format!("{:x}", Sha256::digest(bytes)),
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
            package: "native_sdk_example".into(),
            bindings: vec![binding],
        })?,
    )?;
    if !cgx2_bind::check_package(&root).passed() {
        return Err("package preflight failed".into());
    }
    Ok(())
}
