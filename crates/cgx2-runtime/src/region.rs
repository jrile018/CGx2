/// The first runtime charges this fixed amount for each occupied object slot.
pub const SLOT_METADATA_BYTES: usize = 64;
/// Session bookkeeping charged to active no-GC budgets when a region is created.
pub const REGION_METADATA_BYTES: usize = 128;
/// One stored reference is charged by its reserved vector capacity.
pub const EDGE_METADATA_BYTES: usize = std::mem::size_of::<Handle>();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollectionMode {
    Auto { trigger_bytes: usize },
    Manual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RegionId {
    pub(crate) session: u64,
    pub(crate) index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Handle {
    pub(crate) region: RegionId,
    pub(crate) slot: usize,
    pub(crate) generation: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RegionStats {
    pub committed_bytes: usize,
    pub live_bytes: usize,
    pub peak_bytes: usize,
    pub allocation_failures: u64,
    pub collection_count: u64,
    pub total_collected_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllocationError {
    BudgetExceeded,
    RegionLimit,
    HostAllocation,
    InvalidRegion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryError {
    InvalidHandle,
    InvalidRegion,
    CrossRegionEdge,
    RegionHasRoots,
    CollectionForbidden,
    NotRooted,
    Allocation(AllocationError),
}
