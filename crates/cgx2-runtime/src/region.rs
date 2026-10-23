/// The first runtime charges this fixed amount for each occupied object slot.
pub const SLOT_METADATA_BYTES: usize = 64;
/// Session bookkeeping charged to active no-GC budgets when a region is created.
pub const REGION_METADATA_BYTES: usize = 128;
/// One stored reference is charged by its reserved vector capacity.
pub const EDGE_METADATA_BYTES: usize = std::mem::size_of::<Handle>();
/// One preallocated checked reference slot, charged by reserved vector capacity.
pub const REFERENCE_SLOT_METADATA_BYTES: usize = std::mem::size_of::<Option<Handle>>();
/// Maximum payload in the direct managed-object profile.
pub const MAX_OBJECT_PAYLOAD_BYTES: usize = 1 << 20;
/// Maximum checked references in one managed object.
pub const MAX_OBJECT_REFERENCE_SLOTS: usize = 256;

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
impl RegionId {
    /// Stable index within the creating session. Closed indices are never reused.
    pub fn index(self) -> usize {
        self.index
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Handle {
    pub(crate) region: RegionId,
    pub(crate) slot: usize,
    pub(crate) generation: u64,
}
impl Handle {
    pub fn region(self) -> RegionId {
        self.region
    }

    pub fn generation(self) -> u64 {
        self.generation
    }
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
/// Direct count of live runtime root registrations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RootCensus {
    /// Number of live objects with at least one root registration.
    pub rooted_handles: usize,
    /// Total registrations, including repeated roots of the same handle.
    pub root_registrations: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllocationError {
    BudgetExceeded,
    RegionLimit,
    HostAllocation,
    InvalidRegion,
    ShapeLimit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryError {
    InvalidHandle,
    InvalidRegion,
    NotObject,
    InvalidReferenceSlot,
    CrossRegionEdge,
    RegionHasRoots,
    CollectionForbidden,
    NotRooted,
    RootCensusOverflow,
    Allocation(AllocationError),
}
