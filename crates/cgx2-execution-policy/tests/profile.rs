use cgx2_execution_policy::profile::{
    MachineFingerprint, ProfileError, build_profile, load_profile, sample_hash,
};
use cgx2_execution_policy::{Backend, Capabilities, Policy, ReasonCode, Workload, choose_at};
use cgx2_execution_policy::{ExecuteError, MapError, execute_map};
use std::time::{Duration, SystemTime};

fn machine() -> MachineFingerprint {
    MachineFingerprint {
        os: "linux".into(),
        arch: "x86_64".into(),
        cpu: "test-cpu".into(),
        available_threads: 8,
        rustc: "rustc 1.98.1".into(),
    }
}

fn raw(parallel: &[u128], scalar: &[u128]) -> String {
    let mut csv = String::from(
        "schema,phase,kind,elements,workers,sample,elapsed_ns,os,arch,cpu,available_threads,rustc\n",
    );
    for phase in ["calibration", "holdout"] {
        for (kind, times) in [("sequential", scalar), ("parallel", parallel)] {
            for (sample, time) in times.iter().enumerate() {
                csv.push_str(&format!(
                    "1,{phase},{kind},4096,4,{sample},{time},linux,x86_64,test-cpu,8,rustc 1.98.1\n"
                ));
            }
        }
    }
    csv
}

#[test]
fn missing_noisy_and_malformed_samples_are_rejected() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
    assert!(build_profile("", &machine(), 4, now, Duration::from_secs(100)).is_err());
    let noisy = raw(&[100, 200, 1000], &[1000, 1000, 1000]);
    assert!(matches!(
        build_profile(&noisy, &machine(), 4, now, Duration::from_secs(100)),
        Err(ProfileError::NoisySamples)
    ));
    let mut malformed = raw(&[100, 101, 102], &[1000, 1001, 1002]);
    malformed.push_str("1,holdout,parallel,4096,4,0,100,linux,x86_64,test-cpu,8,rustc 1.98.1\n");
    assert!(build_profile(&malformed, &machine(), 4, now, Duration::from_secs(100)).is_err());
    let insufficient: String = raw(&[100, 101, 102], &[1000, 1001, 1002])
        .lines()
        .filter(|line| !line.starts_with("1,holdout,parallel,4096,4,2,"))
        .map(|line| format!("{line}\n"))
        .collect();
    assert!(matches!(
        build_profile(&insufficient, &machine(), 4, now, Duration::from_secs(100)),
        Err(ProfileError::InsufficientSamples)
    ));
}

#[test]
fn profile_loader_checks_provenance_expiry_workers_range_and_hash() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
    let csv = raw(&[100, 101, 102], &[1000, 1001, 1002]);
    let profile = build_profile(&csv, &machine(), 4, now, Duration::from_secs(100)).unwrap();
    assert_eq!(profile.raw_sample_hash, sample_hash(csv.as_bytes()));
    let text = profile.to_text();
    let measurements = load_profile(&text, &csv, &machine(), 4, now).unwrap();
    assert_eq!(measurements.minimum_elements, 4096);
    let caps = Capabilities {
        parallel_workers: 4,
        measurements: Some(measurements),
        ..Capabilities::default()
    };
    assert_eq!(
        choose_at(Policy::Auto, &Workload::checked_i64_map(4096), &caps, now)
            .unwrap()
            .selected,
        Backend::ParallelCpu
    );
    assert_eq!(
        choose_at(Policy::Auto, &Workload::checked_i64_map(4097), &caps, now)
            .unwrap()
            .reason,
        ReasonCode::MissingMeasurements
    );
    assert!(load_profile(&text, &csv, &machine(), 4, now + Duration::from_secs(100)).is_err());
    assert!(load_profile(&text, &csv, &machine(), 2, now).is_err());
    let mut other = machine();
    other.cpu = "other-cpu".into();
    assert!(load_profile(&text, &csv, &other, 4, now).is_err());
    assert!(load_profile(&text, &csv.replace(",1002,", ",1003,"), &machine(), 4, now).is_err());
    assert!(
        load_profile(
            &text.replace("schema=1", "schema=2"),
            &csv,
            &machine(),
            4,
            now
        )
        .is_err()
    );
    assert!(
        load_profile(
            &text.replace("workload=checked-i64-map-v1", "workload=other"),
            &csv,
            &machine(),
            4,
            now
        )
        .is_err()
    );
    assert!(
        load_profile(
            &text.replace("par_upper_ns=113", "par_upper_ns=1"),
            &csv,
            &machine(),
            4,
            now
        )
        .is_err()
    );
}

#[test]
fn intervals_are_deterministic_and_conservative() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
    let csv = raw(&[100, 101, 102], &[1000, 1001, 1002]);
    let a = build_profile(&csv, &machine(), 4, now, Duration::from_secs(100)).unwrap();
    let b = build_profile(&csv, &machine(), 4, now, Duration::from_secs(100)).unwrap();
    assert_eq!(a.to_text(), b.to_text());
    assert!(
        a.parallel_cost.estimate(4096).unwrap().upper_ns
            < a.sequential_cost.estimate(4096).unwrap().lower_ns
    );
    assert_eq!(a.heldout_max_regret_ppm, 0);
    let uncertain = raw(&[900, 1100, 900], &[1000, 1000, 1000]);
    let profile = build_profile(&uncertain, &machine(), 4, now, Duration::from_secs(100)).unwrap();
    assert_eq!(profile.heldout_max_regret_ppm, 111_111);
}

#[test]
fn loaded_auto_matches_both_backends_and_preserves_first_overflow() {
    let now = SystemTime::now();
    for workers in [2, 3, 4, 8, 32] {
        let size = (workers * 1024).max(4096);
        let mut csv = String::from(
            "schema,phase,kind,elements,workers,sample,elapsed_ns,os,arch,cpu,available_threads,rustc\n",
        );
        for phase in ["calibration", "holdout"] {
            for sample in 0..3 {
                for (kind, elapsed) in [
                    ("sequential", 100_000 + sample),
                    ("parallel", 10_000 + sample),
                ] {
                    csv.push_str(&format!("1,{phase},{kind},{size},{workers},{sample},{elapsed},linux,x86_64,test-cpu,8,rustc 1.98.1\n"));
                }
            }
        }
        let profile =
            build_profile(&csv, &machine(), workers, now, Duration::from_secs(3600)).unwrap();
        let measurements =
            load_profile(&profile.to_text(), &csv, &machine(), workers, now).unwrap();
        let caps = Capabilities {
            parallel_workers: workers,
            measurements: Some(measurements),
            ..Capabilities::default()
        };
        let input: Vec<i64> = (0..size).map(|i| (i % 97) as i64 - 48).collect();
        let sequential = execute_map(&input, -3, 19, Policy::SequentialCpu, &caps).unwrap();
        let parallel = execute_map(&input, -3, 19, Policy::ParallelCpu, &caps).unwrap();
        let auto = execute_map(&input, -3, 19, Policy::Auto, &caps).unwrap();
        assert_eq!(auto.decision.selected, Backend::ParallelCpu);
        assert_eq!(auto.values, sequential.values);
        assert_eq!(auto.values, parallel.values);
        let small = execute_map(&input[..1024], -3, 19, Policy::Auto, &caps).unwrap();
        assert_eq!(
            (small.decision.selected, small.decision.reason),
            (Backend::SequentialCpu, ReasonCode::SmallWorkload)
        );
        let mut overflowing = input;
        let first = size / workers - 1;
        overflowing[first] = i64::MAX;
        overflowing[size - 1] = i64::MIN;
        for policy in [Policy::SequentialCpu, Policy::ParallelCpu, Policy::Auto] {
            assert_eq!(
                execute_map(&overflowing, 2, 0, policy, &caps),
                Err(ExecuteError::Map(MapError::Overflow { index: first }))
            );
        }
    }
}
