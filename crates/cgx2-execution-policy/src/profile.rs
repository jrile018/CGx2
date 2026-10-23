//! Versioned, local-only timing profiles for the reviewed checked `i64` CPU maps.
//! The sample hash is an integrity checksum, not authentication of a profile.

use crate::{
    CostInterval, CostProfile, MAX_WORKERS, MIN_CHUNK_ELEMENTS, MIN_PARALLEL_ELEMENTS, Measurements,
};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, SystemTime};

pub const SCHEMA_VERSION: u32 = 1;
pub const WORKLOAD_ID: &str = "checked-i64-map-v1";
pub const ZIP_SCHEMA_VERSION: u32 = 2;
pub const ZIP_WORKLOAD_ID: &str = "checked-i64-zip-map-v1";
const HEADER: &str =
    "schema,phase,kind,elements,workers,sample,elapsed_ns,os,arch,cpu,available_threads,rustc";
const MAX_RAW_BYTES: usize = 4 * 1024 * 1024;
const MAX_SAMPLES: usize = 100_000;
const MAX_TTL_SECS: u64 = 7 * 24 * 60 * 60;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachineFingerprint {
    pub os: String,
    pub arch: String,
    pub cpu: String,
    pub available_threads: usize,
    pub rustc: String,
}

impl MachineFingerprint {
    fn valid(&self) -> bool {
        self.available_threads > 0
            && [&self.os, &self.arch, &self.cpu, &self.rustc]
                .iter()
                .all(|s| !s.is_empty() && !s.chars().any(|c| matches!(c, ',' | '\n' | '\r' | '=')))
    }
}

/// Captures the local CPU model and toolchain. A different CPU or toolchain
/// cannot reuse a saved profile, even when OS and architecture agree.
pub fn current_machine() -> Result<MachineFingerprint, ProfileError> {
    let rustc_output = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .map_err(|_| ProfileError::MachineMismatch)?;
    if !rustc_output.status.success() {
        return Err(ProfileError::MachineMismatch);
    }
    let rustc = String::from_utf8(rustc_output.stdout)
        .map_err(|_| ProfileError::MachineMismatch)?
        .trim()
        .to_owned();
    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|contents| {
            contents.lines().find_map(|line| {
                let (key, value) = line.split_once(':')?;
                matches!(key.trim(), "model name" | "Hardware" | "Processor")
                    .then(|| value.trim().to_owned())
            })
        })
        .unwrap_or_else(|| std::env::consts::ARCH.to_owned());
    let machine = MachineFingerprint {
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        cpu,
        available_threads: std::thread::available_parallelism().map_or(1, |n| n.get()),
        rustc,
    };
    machine
        .valid()
        .then_some(machine)
        .ok_or(ProfileError::MachineMismatch)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileError {
    InvalidSamples,
    InsufficientSamples,
    NoisySamples,
    InvalidProfile,
    StaleProfile,
    MachineMismatch,
    WorkerMismatch,
    HashMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Profile {
    pub schema_version: u32,
    pub workload_id: String,
    pub machine: MachineFingerprint,
    pub worker_count: usize,
    pub minimum_elements: usize,
    pub maximum_elements: usize,
    /// Minimum paired repetitions per backend and phase at each calibrated size.
    pub calibration_samples: usize,
    pub heldout_samples: usize,
    pub created_epoch_secs: u64,
    pub expires_epoch_secs: u64,
    pub raw_sample_hash: String,
    pub sequential_cost: CostProfile,
    pub parallel_cost: CostProfile,
    /// Maximum latency regret of the selected backend over paired held-out samples.
    pub heldout_max_regret_ppm: u64,
}

impl Profile {
    pub fn to_text(&self) -> String {
        let s = self.sequential_cost.launch_setup;
        let p = self.parallel_cost.launch_setup;
        format!(
            "schema={}\nworkload={}\nos={}\narch={}\ncpu={}\nthreads={}\nrustc={}\nworkers={}\nmin_elements={}\nmax_elements={}\ncalibration_samples={}\nheldout_samples={}\ncreated={}\nexpires={}\nraw_hash={}\nseq_lower_ns={}\nseq_upper_ns={}\npar_lower_ns={}\npar_upper_ns={}\nheldout_max_regret_ppm={}\n",
            self.schema_version,
            self.workload_id,
            self.machine.os,
            self.machine.arch,
            self.machine.cpu,
            self.machine.available_threads,
            self.machine.rustc,
            self.worker_count,
            self.minimum_elements,
            self.maximum_elements,
            self.calibration_samples,
            self.heldout_samples,
            self.created_epoch_secs,
            self.expires_epoch_secs,
            self.raw_sample_hash,
            s.lower_ns,
            s.upper_ns,
            p.lower_ns,
            p.upper_ns,
            self.heldout_max_regret_ppm
        )
    }

    pub fn measurements(&self) -> Measurements {
        Measurements {
            workload_id: self.workload_id.clone(),
            valid_until: SystemTime::UNIX_EPOCH + Duration::from_secs(self.expires_epoch_secs),
            minimum_elements: self.minimum_elements,
            maximum_elements: self.maximum_elements,
            sample_count: self.calibration_samples.min(self.heldout_samples),
            worker_count: self.worker_count,
            scalar: self.sequential_cost,
            parallel: self.parallel_cost,
        }
    }
}

/// Stable FNV-1a checksum of exact raw CSV bytes. Profiles are local evidence,
/// not signed or safe to accept from untrusted publishers.
pub fn sample_hash(bytes: &[u8]) -> String {
    let mut value: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(0x100000001b3);
    }
    format!("{value:016x}")
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Phase {
    Calibration,
    Holdout,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Kind {
    Sequential,
    Parallel,
}
type Key = (Phase, Kind, usize, usize);

fn parse_raw(
    csv: &str,
    machine: &MachineFingerprint,
    workers: usize,
    schema_version: u32,
) -> Result<BTreeMap<Key, u128>, ProfileError> {
    if csv.len() > MAX_RAW_BYTES || !machine.valid() || !(2..=MAX_WORKERS).contains(&workers) {
        return Err(ProfileError::InvalidSamples);
    }
    let mut lines = csv.lines();
    if lines.next() != Some(HEADER) {
        return Err(ProfileError::InvalidSamples);
    }
    let mut samples = BTreeMap::new();
    for line in lines {
        if line.is_empty() || samples.len() >= MAX_SAMPLES {
            return Err(ProfileError::InvalidSamples);
        }
        let fields: Vec<_> = line.split(',').collect();
        if fields.len() != 12 || fields[0] != schema_version.to_string() {
            return Err(ProfileError::InvalidSamples);
        }
        let phase = match fields[1] {
            "calibration" => Phase::Calibration,
            "holdout" => Phase::Holdout,
            _ => return Err(ProfileError::InvalidSamples),
        };
        let kind = match fields[2] {
            "sequential" => Kind::Sequential,
            "parallel" => Kind::Parallel,
            _ => return Err(ProfileError::InvalidSamples),
        };
        let elements = fields[3]
            .parse::<usize>()
            .map_err(|_| ProfileError::InvalidSamples)?;
        let row_workers = fields[4]
            .parse::<usize>()
            .map_err(|_| ProfileError::InvalidSamples)?;
        let sample = fields[5]
            .parse::<usize>()
            .map_err(|_| ProfileError::InvalidSamples)?;
        let elapsed = fields[6]
            .parse::<u128>()
            .map_err(|_| ProfileError::InvalidSamples)?;
        if !(MIN_PARALLEL_ELEMENTS..=100_000_000).contains(&elements)
            || !(2..=MAX_WORKERS).contains(&row_workers)
            || elements / MIN_CHUNK_ELEMENTS < row_workers
            || sample >= 10_000
            || elapsed == 0
        {
            return Err(ProfileError::InvalidSamples);
        }
        if fields[7] != machine.os
            || fields[8] != machine.arch
            || fields[9] != machine.cpu
            || fields[10] != machine.available_threads.to_string()
            || fields[11] != machine.rustc
        {
            return Err(ProfileError::MachineMismatch);
        }
        if row_workers != workers {
            continue;
        }
        if samples
            .insert((phase, kind, elements, sample), elapsed)
            .is_some()
        {
            return Err(ProfileError::InvalidSamples);
        }
    }
    if samples.is_empty() {
        return Err(ProfileError::WorkerMismatch);
    }
    Ok(samples)
}

fn zero_profile(lower_ns: u128, upper_ns: u128) -> CostProfile {
    let zero = CostInterval {
        lower_ns: 0,
        upper_ns: 0,
    };
    CostProfile {
        // Complete execute_map timings, including planning and thread launch/join.
        launch_setup: CostInterval { lower_ns, upper_ns },
        cpu_work_per_element: zero,
        host_device_transfer: zero,
        synchronization: zero,
        memory_pressure: zero,
    }
}

fn epoch_secs(time: SystemTime) -> Result<u64, ProfileError> {
    Ok(time
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|_| ProfileError::InvalidProfile)?
        .as_secs())
}

/// Build a conservative profile from paired calibration and independent
/// held-out repetitions. A profile may cover several sampled sizes; its lower
/// and upper bounds enclose *all* observed end-to-end costs across that range.
pub fn build_profile(
    csv: &str,
    machine: &MachineFingerprint,
    workers: usize,
    now: SystemTime,
    ttl: Duration,
) -> Result<Profile, ProfileError> {
    build_profile_inner(
        csv,
        machine,
        workers,
        now,
        ttl,
        None,
        (SCHEMA_VERSION, WORKLOAD_ID),
    )
}

/// Produce an exact-size profile from a multi-size raw CSV. This avoids a wide
/// interval when small and large inputs have very different absolute latency.
pub fn build_profile_for_size(
    csv: &str,
    machine: &MachineFingerprint,
    workers: usize,
    elements: usize,
    now: SystemTime,
    ttl: Duration,
) -> Result<Profile, ProfileError> {
    build_profile_inner(
        csv,
        machine,
        workers,
        now,
        ttl,
        Some(elements),
        (SCHEMA_VERSION, WORKLOAD_ID),
    )
}

/// Build a profile from checked two-input zip-map samples only.
pub fn build_zip_profile(
    csv: &str,
    machine: &MachineFingerprint,
    workers: usize,
    now: SystemTime,
    ttl: Duration,
) -> Result<Profile, ProfileError> {
    build_profile_inner(
        csv,
        machine,
        workers,
        now,
        ttl,
        None,
        (ZIP_SCHEMA_VERSION, ZIP_WORKLOAD_ID),
    )
}

/// Restrict a zip-map profile to one calibrated input length.
pub fn build_zip_profile_for_size(
    csv: &str,
    machine: &MachineFingerprint,
    workers: usize,
    elements: usize,
    now: SystemTime,
    ttl: Duration,
) -> Result<Profile, ProfileError> {
    build_profile_inner(
        csv,
        machine,
        workers,
        now,
        ttl,
        Some(elements),
        (ZIP_SCHEMA_VERSION, ZIP_WORKLOAD_ID),
    )
}

pub(crate) fn build_profile_inner(
    csv: &str,
    machine: &MachineFingerprint,
    workers: usize,
    now: SystemTime,
    ttl: Duration,
    size_filter: Option<usize>,
    identity: (u32, &str),
) -> Result<Profile, ProfileError> {
    let (schema_version, workload_id) = identity;
    let created = epoch_secs(now)?;
    if ttl.is_zero() || ttl.as_secs() == 0 || ttl.as_secs() > MAX_TTL_SECS {
        return Err(ProfileError::InvalidProfile);
    }
    let expires = created
        .checked_add(ttl.as_secs())
        .ok_or(ProfileError::InvalidProfile)?;
    let samples = parse_raw(csv, machine, workers, schema_version)?;
    let sizes: BTreeSet<_> = samples
        .keys()
        .map(|key| key.2)
        .filter(|s| size_filter.is_none_or(|filter| filter == *s))
        .collect();
    if sizes.is_empty() {
        return Err(ProfileError::InsufficientSamples);
    }
    let mut calibration_samples = usize::MAX;
    let mut heldout_samples = usize::MAX;
    let mut heldout_pairs = Vec::new();
    let mut seq_min = u128::MAX;
    let mut seq_max = 0_u128;
    let mut par_min = u128::MAX;
    let mut par_max = 0_u128;
    for size in &sizes {
        for phase in [Phase::Calibration, Phase::Holdout] {
            let seq: BTreeMap<_, _> = samples
                .iter()
                .filter(|(key, _)| key.0 == phase && key.1 == Kind::Sequential && key.2 == *size)
                .map(|(key, time)| (key.3, *time))
                .collect();
            let par: BTreeMap<_, _> = samples
                .iter()
                .filter(|(key, _)| key.0 == phase && key.1 == Kind::Parallel && key.2 == *size)
                .map(|(key, time)| (key.3, *time))
                .collect();
            if seq.len() < 3 || seq.keys().ne(par.keys()) {
                return Err(ProfileError::InsufficientSamples);
            }
            match phase {
                Phase::Calibration => calibration_samples = calibration_samples.min(seq.len()),
                Phase::Holdout => heldout_samples = heldout_samples.min(seq.len()),
            }
            let local_seq_min = *seq.values().min().unwrap();
            let local_seq_max = *seq.values().max().unwrap();
            let local_par_min = *par.values().min().unwrap();
            let local_par_max = *par.values().max().unwrap();
            if local_seq_max > local_seq_min.saturating_mul(2)
                || local_par_max > local_par_min.saturating_mul(2)
            {
                return Err(ProfileError::NoisySamples);
            }
            seq_min = seq_min.min(local_seq_min);
            seq_max = seq_max.max(local_seq_max);
            par_min = par_min.min(local_par_min);
            par_max = par_max.max(local_par_max);
            if phase == Phase::Holdout {
                for (sample, scalar_ns) in seq {
                    heldout_pairs.push((scalar_ns, par[&sample]));
                }
            }
        }
    }
    // 10% high-side margin covers small clock and dispatch variations. The
    // planner can select parallel only if this upper bound beats the scalar low.
    let upper = |n: u128| n.saturating_mul(11).div_ceil(10);
    let parallel_selected = upper(par_max) < seq_min;
    let regret = heldout_pairs
        .into_iter()
        .map(|(scalar_ns, parallel_ns)| {
            let selected = if parallel_selected {
                parallel_ns
            } else {
                scalar_ns
            };
            let best = scalar_ns.min(parallel_ns);
            selected.saturating_sub(best).saturating_mul(1_000_000) / best
        })
        .max()
        .unwrap_or(0)
        .min(u64::MAX as u128) as u64;
    Ok(Profile {
        schema_version,
        workload_id: workload_id.into(),
        machine: machine.clone(),
        worker_count: workers,
        minimum_elements: *sizes.first().unwrap(),
        maximum_elements: *sizes.last().unwrap(),
        calibration_samples,
        heldout_samples,
        created_epoch_secs: created,
        expires_epoch_secs: expires,
        raw_sample_hash: sample_hash(csv.as_bytes()),
        sequential_cost: zero_profile(seq_min, upper(seq_max)),
        parallel_cost: zero_profile(par_min, upper(par_max)),
        heldout_max_regret_ppm: regret,
    })
}

pub(crate) fn parse_profile(text: &str) -> Result<Profile, ProfileError> {
    if text.len() > 16_384 {
        return Err(ProfileError::InvalidProfile);
    }
    let mut fields = BTreeMap::new();
    for line in text.lines() {
        let (key, value) = line.split_once('=').ok_or(ProfileError::InvalidProfile)?;
        if value.is_empty() || fields.insert(key, value).is_some() {
            return Err(ProfileError::InvalidProfile);
        }
    }
    if fields.len() != 20 {
        return Err(ProfileError::InvalidProfile);
    }
    let get = |key| fields.get(key).copied().ok_or(ProfileError::InvalidProfile);
    let number = |key| {
        get(key)?
            .parse::<u128>()
            .map_err(|_| ProfileError::InvalidProfile)
    };
    let usize_field = |key| usize::try_from(number(key)?).map_err(|_| ProfileError::InvalidProfile);
    let u64_field = |key| u64::try_from(number(key)?).map_err(|_| ProfileError::InvalidProfile);
    Ok(Profile {
        schema_version: u32::try_from(number("schema")?)
            .map_err(|_| ProfileError::InvalidProfile)?,
        workload_id: get("workload")?.into(),
        machine: MachineFingerprint {
            os: get("os")?.into(),
            arch: get("arch")?.into(),
            cpu: get("cpu")?.into(),
            available_threads: usize_field("threads")?,
            rustc: get("rustc")?.into(),
        },
        worker_count: usize_field("workers")?,
        minimum_elements: usize_field("min_elements")?,
        maximum_elements: usize_field("max_elements")?,
        calibration_samples: usize_field("calibration_samples")?,
        heldout_samples: usize_field("heldout_samples")?,
        created_epoch_secs: u64_field("created")?,
        expires_epoch_secs: u64_field("expires")?,
        raw_sample_hash: get("raw_hash")?.into(),
        sequential_cost: zero_profile(number("seq_lower_ns")?, number("seq_upper_ns")?),
        parallel_cost: zero_profile(number("par_lower_ns")?, number("par_upper_ns")?),
        heldout_max_regret_ppm: u64_field("heldout_max_regret_ppm")?,
    })
}

/// Validate schema, checksum, provenance, clock and all interval fields against
/// the saved raw samples before returning planner-compatible measurements.
pub fn load_profile(
    text: &str,
    csv: &str,
    machine: &MachineFingerprint,
    workers: usize,
    now: SystemTime,
) -> Result<Measurements, ProfileError> {
    load_profile_inner(
        text,
        csv,
        machine,
        workers,
        now,
        SCHEMA_VERSION,
        WORKLOAD_ID,
    )
}

/// Validate exact zip-map profile and raw sample identity before enabling Auto.
pub fn load_zip_profile(
    text: &str,
    csv: &str,
    machine: &MachineFingerprint,
    workers: usize,
    now: SystemTime,
) -> Result<Measurements, ProfileError> {
    load_profile_inner(
        text,
        csv,
        machine,
        workers,
        now,
        ZIP_SCHEMA_VERSION,
        ZIP_WORKLOAD_ID,
    )
}

pub(crate) fn load_profile_inner(
    text: &str,
    csv: &str,
    machine: &MachineFingerprint,
    workers: usize,
    now: SystemTime,
    schema_version: u32,
    workload_id: &str,
) -> Result<Measurements, ProfileError> {
    let saved = parse_profile(text)?;
    if saved.schema_version != schema_version || saved.workload_id != workload_id {
        return Err(ProfileError::InvalidProfile);
    }
    if saved.machine != *machine {
        return Err(ProfileError::MachineMismatch);
    }
    if saved.worker_count != workers {
        return Err(ProfileError::WorkerMismatch);
    }
    let current = epoch_secs(now)?;
    if current < saved.created_epoch_secs || current >= saved.expires_epoch_secs {
        return Err(ProfileError::StaleProfile);
    }
    if saved.raw_sample_hash != sample_hash(csv.as_bytes()) {
        return Err(ProfileError::HashMismatch);
    }
    let ttl = saved
        .expires_epoch_secs
        .checked_sub(saved.created_epoch_secs)
        .ok_or(ProfileError::InvalidProfile)?;
    let rebuilt = build_profile_inner(
        csv,
        machine,
        workers,
        SystemTime::UNIX_EPOCH + Duration::from_secs(saved.created_epoch_secs),
        Duration::from_secs(ttl),
        (saved.minimum_elements == saved.maximum_elements).then_some(saved.minimum_elements),
        (schema_version, workload_id),
    )?;
    if rebuilt != saved {
        return Err(ProfileError::InvalidProfile);
    }
    Ok(saved.measurements())
}
