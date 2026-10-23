use cgx2_ffi_core::Limits;
use cgx2_ffi_native::sdk_v2::Sdk;
use std::path::Path;
fn main() {
    let package = std::env::args()
        .nth(1)
        .expect("usage: sdk_roundtrip PACKAGE");
    // SAFETY: example packages only the reviewed SDK fixture. Arbitrary native
    // libraries require their own review against sdk_v2.h before using this API.
    let sdk = unsafe { Sdk::open_pinned(Path::new(&package), "sdk") }.expect("pinned SDK");
    let value = sdk
        .copy("héllo".as_bytes(), true, Limits::default())
        .expect("bounded UTF-8");
    let owner = sdk.create(42).expect("owner");
    let shared = owner.into_shared();
    let retained = shared.clone();
    drop(shared);
    drop(sdk);
    assert_eq!(retained.read().unwrap(), 42);
    println!("{value:?}; retained_handle=42");
}
