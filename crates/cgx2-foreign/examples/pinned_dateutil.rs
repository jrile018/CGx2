//! Operator-run real package acceptance; never installs a dependency.
use cgx2_foreign::package_v1::{
    ArtifactPinV1, PackageAuthorizationV1, PackageEnvironmentV1, RuntimePinV1, hash_file_v1,
};
use cgx2_foreign::{BridgeValue, PythonBridge};
use std::{collections::BTreeSet, error::Error};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--identity"] {
        let bridge = PythonBridge::new()?;
        let id = bridge.embedded_identity_v1()?;
        let runtime = RuntimePinV1 {
            interpreter: ArtifactPinV1 {
                sha256: hash_file_v1(&id.executable)?,
                path: id.executable,
            },
            library: ArtifactPinV1 {
                sha256: hash_file_v1(&id.library)?,
                path: id.library,
            },
            version: id.version,
            prefix: id.prefix,
            base_prefix: id.base_prefix,
            stdlib: vec![],
        };
        println!("{}", serde_json::to_string_pretty(&runtime)?);
        return Ok(());
    }
    if args.len() != 2 {
        return Err(
            "usage: pinned_dateutil LOCK.json EXPECTED_ENVIRONMENT_ID (or --identity)".into(),
        );
    }
    let lock = PackageEnvironmentV1::from_json(&std::fs::read(&args[0])?)?;
    // SAFETY: main has not started threads or used CPython; this standalone runner
    // initializes it exactly once. --identity is a separate, trusted probe process.
    #[cfg(target_os = "linux")]
    let bridge = unsafe { lock.start_isolated()? };
    #[cfg(not(target_os = "linux"))]
    return Err("isolated pinned runtime v1 requires Linux".into());
    let grant = PackageAuthorizationV1 {
        environment_id: args[1].clone(),
        allowed_calls: BTreeSet::from(["dateutil.isoformat".into()]),
    };
    let result = lock.invoke(
        &bridge,
        &grant,
        "dateutil.isoformat",
        &[BridgeValue::String("2024-02-29T12:34:56Z".into())],
        &[],
    )?;
    if !matches!(&result, BridgeValue::String(s) if s == "2024-02-29T12:34:56+00:00") {
        return Err("real dateutil result did not match the acceptance fixture".into());
    }
    println!("PASS real pinned python-dateutil: {result:?}");
    println!(
        "environment={} runtime={}",
        lock.environment_id, lock.runtime.version
    );
    for package in &lock.distributions {
        println!("distribution={} version={}", package.name, package.version);
    }
    Ok(())
}
