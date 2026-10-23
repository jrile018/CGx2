use cgx2_execution_policy::profile::{
    MachineFingerprint, ProfileError, ZIP_WORKLOAD_ID, build_profile, build_zip_profile,
    load_profile, load_zip_profile,
};
use cgx2_execution_policy::{Backend, Capabilities, Policy, ReasonCode, Workload, choose_at};
use std::time::{Duration, SystemTime};

fn machine() -> MachineFingerprint {
    MachineFingerprint {
        os: "linux".into(),
        arch: "x86_64".into(),
        cpu: "zip-test".into(),
        available_threads: 8,
        rustc: "rustc test".into(),
    }
}

fn raw(parallel: &[u128], sequential: &[u128]) -> String {
    let mut csv = String::from(
        "schema,phase,kind,elements,workers,sample,elapsed_ns,os,arch,cpu,available_threads,rustc\n",
    );
    for phase in ["calibration", "holdout"] {
        for (kind, times) in [("sequential", sequential), ("parallel", parallel)] {
            for (sample, elapsed) in times.iter().enumerate() {
                csv.push_str(&format!(
                    "2,{phase},{kind},4096,4,{sample},{elapsed},linux,x86_64,zip-test,8,rustc test\n"
                ));
            }
        }
    }
    csv
}

#[test]
fn exact_zip_identity_and_validated_holdout_enable_auto() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
    let csv = raw(&[100, 101, 102], &[1000, 1001, 1002]);
    let profile = build_zip_profile(&csv, &machine(), 4, now, Duration::from_secs(100)).unwrap();
    assert_eq!(profile.workload_id, ZIP_WORKLOAD_ID);
    let measurements = load_zip_profile(&profile.to_text(), &csv, &machine(), 4, now).unwrap();
    let caps = Capabilities {
        parallel_workers: 4,
        measurements: Some(measurements),
        ..Capabilities::default()
    };
    let zip = Workload::checked_i64_zip_map(4096);
    assert_eq!(
        choose_at(Policy::Auto, &zip, &caps, now).unwrap().selected,
        Backend::ParallelCpu
    );
    assert_eq!(
        choose_at(Policy::Auto, &Workload::checked_i64_map(4096), &caps, now)
            .unwrap()
            .reason,
        ReasonCode::MissingMeasurements
    );
    assert_eq!(
        choose_at(Policy::Auto, &zip, &caps, now + Duration::from_secs(100))
            .unwrap()
            .reason,
        ReasonCode::StaleMeasurements
    );
    assert_eq!(
        choose_at(
            Policy::Auto,
            &Workload::checked_i64_zip_map(4097),
            &caps,
            now
        )
        .unwrap()
        .reason,
        ReasonCode::MissingMeasurements
    );
    assert!(matches!(
        load_profile(&profile.to_text(), &csv, &machine(), 4, now),
        Err(ProfileError::InvalidProfile)
    ));
    assert!(build_profile(&csv, &machine(), 4, now, Duration::from_secs(100)).is_err());
}

#[test]
fn noisy_incomplete_mismatched_and_tampered_profiles_fail_closed() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
    let csv = raw(&[100, 101, 102], &[1000, 1001, 1002]);
    let profile = build_zip_profile(&csv, &machine(), 4, now, Duration::from_secs(100)).unwrap();
    let text = profile.to_text();
    assert_eq!(
        build_zip_profile(
            &raw(&[100, 101, 1000], &[1000, 1001, 1002]),
            &machine(),
            4,
            now,
            Duration::from_secs(100)
        ),
        Err(ProfileError::NoisySamples)
    );
    let incomplete = csv
        .lines()
        .filter(|line| !line.starts_with("2,holdout,parallel,4096,4,2,"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(
        build_zip_profile(&incomplete, &machine(), 4, now, Duration::from_secs(100)),
        Err(ProfileError::InsufficientSamples)
    );
    assert!(load_zip_profile(&text, &csv.replace(",1002,", ",1003,"), &machine(), 4, now).is_err());
    assert!(
        load_zip_profile(
            &text.replace("par_lower_ns=100", "par_lower_ns=1"),
            &csv,
            &machine(),
            4,
            now
        )
        .is_err()
    );
    assert!(
        load_zip_profile(
            &text.replace(ZIP_WORKLOAD_ID, "checked-i64-map-v1"),
            &csv,
            &machine(),
            4,
            now
        )
        .is_err()
    );
    assert!(load_zip_profile(&text, &csv, &machine(), 2, now).is_err());
    let mut other = machine();
    other.cpu = "different".into();
    assert!(load_zip_profile(&text, &csv, &other, 4, now).is_err());
    assert!(load_zip_profile(&text, &csv, &machine(), 4, now + Duration::from_secs(100)).is_err());
}
