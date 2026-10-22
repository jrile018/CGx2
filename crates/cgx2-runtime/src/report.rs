use std::fmt::Write;

use crate::{CollectionMode, RegionId, RegionStats};

/// Version of the stable text/JSON memory report schema.
pub const MEMORY_REPORT_SCHEMA_VERSION: u32 = 1;

/// Bounded, log2 pause histogram. Bucket 0 is exactly zero nanoseconds;
/// bucket n (1..=64) contains [2^(n-1), 2^n) nanoseconds, with bucket 64
/// extending through `u64::MAX`. Durations larger than `u64::MAX` saturate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PauseReport {
    pub sample_count: u64,
    pub total_ns: u128,
    pub min_ns: u64,
    pub max_ns: u64,
    pub buckets: [u64; 65],
}

impl Default for PauseReport {
    fn default() -> Self {
        Self {
            sample_count: 0,
            total_ns: 0,
            min_ns: 0,
            max_ns: 0,
            buckets: [0; 65],
        }
    }
}

impl PauseReport {
    pub(crate) fn record(&mut self, elapsed_ns: u128) {
        let ns = elapsed_ns.min(u128::from(u64::MAX)) as u64;
        let bucket = if ns == 0 {
            0
        } else {
            (u64::BITS - ns.leading_zeros()) as usize
        };
        self.sample_count = self.sample_count.saturating_add(1);
        self.total_ns = self.total_ns.saturating_add(elapsed_ns);
        self.buckets[bucket] = self.buckets[bucket].saturating_add(1);
        if self.sample_count == 1 {
            self.min_ns = ns;
            self.max_ns = ns;
        } else {
            self.min_ns = self.min_ns.min(ns);
            self.max_ns = self.max_ns.max(ns);
        }
    }
}

/// A region's current or terminal counters. Byte fields are logical CGx2
/// region charges, including slot and edge capacity, excluding foreign memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegionReport {
    pub id: RegionId,
    pub is_open: bool,
    pub mode: CollectionMode,
    pub limit_bytes: usize,
    pub committed_bytes: usize,
    pub live_bytes: usize,
    pub peak_bytes: usize,
    pub allocation_failures: u64,
    pub collection_count: u64,
    pub total_collected_bytes: usize,
    pub pause: PauseReport,
}

/// A read-only, deterministic snapshot. `totals.peak_bytes` sums each region's
/// individual high-water mark; it is not a simultaneous session peak.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryReport {
    pub schema_version: u32,
    pub regions: Vec<RegionReport>,
    pub totals: RegionStats,
}

impl MemoryReport {
    /// Renders versioned JSON without user-provided strings or external crates.
    pub fn to_json(&self) -> String {
        let mut out = format!("{{\"schema_version\":{},\"regions\":[", self.schema_version);
        for (index, row) in self.regions.iter().enumerate() {
            if index != 0 {
                out.push(',');
            }
            let (mode, trigger) = match row.mode {
                CollectionMode::Auto { trigger_bytes } => ("auto", trigger_bytes.to_string()),
                CollectionMode::Manual => ("manual", "null".to_owned()),
            };
            write!(
                out,
                "{{\"region_id\":{{\"session\":{},\"index\":{}}},\"is_open\":{},\"mode\":\"{}\",\"trigger_bytes\":{},\"limit_bytes\":{},\"committed_bytes\":{},\"live_bytes\":{},\"peak_bytes\":{},\"allocation_failures\":{},\"collection_count\":{},\"total_collected_bytes\":{},\"pause\":{{\"sample_count\":{},\"total_ns\":{},\"min_ns\":{},\"max_ns\":{},\"buckets\":[",
                row.id.session,
                row.id.index,
                row.is_open,
                mode,
                trigger,
                row.limit_bytes,
                row.committed_bytes,
                row.live_bytes,
                row.peak_bytes,
                row.allocation_failures,
                row.collection_count,
                row.total_collected_bytes,
                row.pause.sample_count,
                row.pause.total_ns,
                row.pause.min_ns,
                row.pause.max_ns,
            )
            .expect("writing to a String cannot fail");
            for (bucket, count) in row.pause.buckets.iter().enumerate() {
                if bucket != 0 {
                    out.push(',');
                }
                write!(out, "{count}").expect("writing to a String cannot fail");
            }
            out.push_str("]}}");
        }
        let totals = &self.totals;
        write!(
            out,
            "],\"totals\":{{\"committed_bytes\":{},\"live_bytes\":{},\"peak_bytes\":{},\"allocation_failures\":{},\"collection_count\":{},\"total_collected_bytes\":{}}}}}",
            totals.committed_bytes,
            totals.live_bytes,
            totals.peak_bytes,
            totals.allocation_failures,
            totals.collection_count,
            totals.total_collected_bytes,
        )
        .expect("writing to a String cannot fail");
        out
    }

    /// Renders the same counters in a stable, line-oriented human format.
    pub fn to_text(&self) -> String {
        let mut out = format!("CGx2 memory report v{}\n", self.schema_version);
        for row in &self.regions {
            let mode = match row.mode {
                CollectionMode::Auto { trigger_bytes } => {
                    format!("auto trigger_bytes={trigger_bytes}")
                }
                CollectionMode::Manual => "manual".to_owned(),
            };
            let state = if row.is_open { "open" } else { "closed" };
            writeln!(
                out,
                "region {}:{} {} mode={} limit_bytes={} committed_bytes={} live_bytes={} peak_bytes={} allocation_failures={} collection_count={} total_collected_bytes={} pause_samples={} pause_total_ns={} pause_min_ns={} pause_max_ns={} pause_buckets={:?}",
                row.id.session,
                row.id.index,
                state,
                mode,
                row.limit_bytes,
                row.committed_bytes,
                row.live_bytes,
                row.peak_bytes,
                row.allocation_failures,
                row.collection_count,
                row.total_collected_bytes,
                row.pause.sample_count,
                row.pause.total_ns,
                row.pause.min_ns,
                row.pause.max_ns,
                row.pause.buckets,
            )
            .expect("writing to a String cannot fail");
        }
        let totals = &self.totals;
        writeln!(
            out,
            "totals committed_bytes={} live_bytes={} peak_bytes={} allocation_failures={} collection_count={} total_collected_bytes={}",
            totals.committed_bytes,
            totals.live_bytes,
            totals.peak_bytes,
            totals.allocation_failures,
            totals.collection_count,
            totals.total_collected_bytes,
        )
        .expect("writing to a String cannot fail");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::PauseReport;

    #[test]
    fn pause_histogram_bounds_and_saturating_units() {
        let mut pause = PauseReport::default();
        for ns in [0, 1, 2, 3, u128::from(u64::MAX) + 1] {
            pause.record(ns);
        }
        assert_eq!(pause.sample_count, 5);
        assert_eq!(pause.buckets[0], 1);
        assert_eq!(pause.buckets[1], 1);
        assert_eq!(pause.buckets[2], 2);
        assert_eq!(pause.buckets[64], 1);
        assert_eq!(pause.min_ns, 0);
        assert_eq!(pause.max_ns, u64::MAX);
        assert_eq!(pause.total_ns, u128::from(u64::MAX) + 7);
        pause.record(u128::MAX);
        assert_eq!(pause.total_ns, u128::MAX);
    }
}
