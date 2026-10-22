//! Standalone, single-threaded CGx2 byte-object tracing kernel.
#![forbid(unsafe_code)]

mod budget;
mod heap;
mod region;

use std::sync::atomic::{AtomicU64, Ordering};

use budget::Budget;
pub use budget::NoGcGuard;
use heap::{Object, Region};
pub use region::{
    AllocationError, CollectionMode, Handle, MemoryError, RegionId, RegionStats,
    EDGE_METADATA_BYTES, REGION_METADATA_BYTES, SLOT_METADATA_BYTES,
};

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

pub struct Session {
    id: u64,
    regions: Vec<Region>,
    pub(crate) budgets: Vec<Budget>,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new() -> Self {
        let id = NEXT_SESSION_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .expect("session identifier space exhausted");
        Self {
            id,
            regions: vec![Region::new(
                CollectionMode::Auto {
                    trigger_bytes: 1 << 20,
                },
                64 << 20,
            )],
            budgets: Vec::new(),
        }
    }

    pub fn default_region(&self) -> RegionId {
        RegionId {
            session: self.id,
            index: 0,
        }
    }

    pub fn no_gc(&mut self, budget_bytes: usize) -> NoGcGuard<'_> {
        self.budgets.push(Budget {
            limit: budget_bytes,
            spent: 0,
        });
        NoGcGuard { session: self }
    }

    pub fn create_region(
        &mut self,
        mode: CollectionMode,
        limit_bytes: usize,
    ) -> Result<RegionId, AllocationError> {
        self.check_budgets(REGION_METADATA_BYTES)?;
        self.regions
            .try_reserve(1)
            .map_err(|_| AllocationError::HostAllocation)?;
        let id = RegionId {
            session: self.id,
            index: self.regions.len(),
        };
        self.regions.push(Region::new(mode, limit_bytes));
        self.charge_budgets(REGION_METADATA_BYTES);
        Ok(id)
    }

    fn region(&self, id: RegionId) -> Result<&Region, MemoryError> {
        if id.session != self.id {
            return Err(MemoryError::InvalidRegion);
        }
        self.regions
            .get(id.index)
            .filter(|region| !region.closed)
            .ok_or(MemoryError::InvalidRegion)
    }

    fn region_mut(&mut self, id: RegionId) -> Result<&mut Region, MemoryError> {
        if id.session != self.id {
            return Err(MemoryError::InvalidRegion);
        }
        self.regions
            .get_mut(id.index)
            .filter(|region| !region.closed)
            .ok_or(MemoryError::InvalidRegion)
    }

    pub fn mode(&self, region: RegionId) -> Result<CollectionMode, MemoryError> {
        Ok(self.region(region)?.mode)
    }
    pub fn limit_bytes(&self, region: RegionId) -> Result<usize, MemoryError> {
        Ok(self.region(region)?.limit_bytes)
    }
    pub fn stats(&self, region: RegionId) -> Result<RegionStats, MemoryError> {
        Ok(self.region(region)?.stats)
    }

    pub fn alloc_bytes(&mut self, region: RegionId, len: usize) -> Result<Handle, AllocationError> {
        let needs_collect = {
            let r = self
                .region(region)
                .map_err(|_| AllocationError::InvalidRegion)?;
            self.budgets.is_empty()
                && matches!(r.mode, CollectionMode::Auto { trigger_bytes } if r.stats.committed_bytes >= trigger_bytes)
        };
        if needs_collect {
            self.collect(region)
                .map_err(|_| AllocationError::InvalidRegion)?;
        }
        let requested = match len.checked_add(SLOT_METADATA_BYTES) {
            Some(requested) => requested,
            None => {
                self.region_mut(region)
                    .expect("validated region")
                    .stats
                    .allocation_failures += 1;
                return Err(AllocationError::HostAllocation);
            }
        };
        if let Err(error) = self.check_budgets(requested) {
            self.region_mut(region)
                .expect("validated region")
                .stats
                .allocation_failures += 1;
            return Err(error);
        }
        let r = self
            .region_mut(region)
            .map_err(|_| AllocationError::InvalidRegion)?;
        if requested > r.limit_bytes.saturating_sub(r.stats.committed_bytes) {
            r.stats.allocation_failures += 1;
            return Err(AllocationError::RegionLimit);
        }
        let mut bytes = Vec::new();
        if bytes.try_reserve_exact(len).is_err() {
            r.stats.allocation_failures += 1;
            return Err(AllocationError::HostAllocation);
        }
        let charge = match bytes.capacity().checked_add(SLOT_METADATA_BYTES) {
            Some(charge) => charge,
            None => {
                r.stats.allocation_failures += 1;
                return Err(AllocationError::HostAllocation);
            }
        };
        // Release the mutable region borrow before checking all active budgets.
        if let Err(error) = self.check_budgets(charge) {
            self.region_mut(region)
                .expect("validated region")
                .stats
                .allocation_failures += 1;
            return Err(error);
        }
        let r = self.region_mut(region).expect("validated region");
        if charge > r.limit_bytes.saturating_sub(r.stats.committed_bytes) {
            r.stats.allocation_failures += 1;
            return Err(AllocationError::RegionLimit);
        }
        bytes.resize(len, 0);
        let index = match r.vacant_slot() {
            Ok(index) => index,
            Err(()) => {
                r.stats.allocation_failures += 1;
                return Err(AllocationError::HostAllocation);
            }
        };
        let slot = &mut r.slots[index];
        slot.object = Some(Object {
            bytes,
            edges: Vec::new(),
            roots: 0,
            charge,
        });
        r.stats.committed_bytes += charge;
        r.stats.live_bytes += charge;
        r.stats.peak_bytes = r.stats.peak_bytes.max(r.stats.committed_bytes);
        let generation = slot.generation;
        self.charge_budgets(charge);
        Ok(Handle {
            region,
            slot: index,
            generation,
        })
    }

    pub fn read_bytes(&self, handle: Handle) -> Result<&[u8], MemoryError> {
        let r = self
            .region(handle.region)
            .map_err(|_| MemoryError::InvalidHandle)?;
        let slot = r.slots.get(handle.slot).ok_or(MemoryError::InvalidHandle)?;
        if slot.generation != handle.generation {
            return Err(MemoryError::InvalidHandle);
        }
        Ok(&slot
            .object
            .as_ref()
            .ok_or(MemoryError::InvalidHandle)?
            .bytes)
    }

    fn object_mut(&mut self, handle: Handle) -> Result<&mut Object, MemoryError> {
        let r = self
            .region_mut(handle.region)
            .map_err(|_| MemoryError::InvalidHandle)?;
        let slot = r
            .slots
            .get_mut(handle.slot)
            .ok_or(MemoryError::InvalidHandle)?;
        if slot.generation != handle.generation {
            return Err(MemoryError::InvalidHandle);
        }
        slot.object.as_mut().ok_or(MemoryError::InvalidHandle)
    }

    pub fn root(&mut self, handle: Handle) -> Result<(), MemoryError> {
        let object = self.object_mut(handle)?;
        object.roots = object
            .roots
            .checked_add(1)
            .ok_or(MemoryError::InvalidHandle)?;
        Ok(())
    }

    pub fn unroot(&mut self, handle: Handle) -> Result<(), MemoryError> {
        let object = self.object_mut(handle)?;
        if object.roots == 0 {
            return Err(MemoryError::NotRooted);
        }
        object.roots -= 1;
        Ok(())
    }

    pub fn link(&mut self, parent: Handle, child: Handle) -> Result<(), MemoryError> {
        self.read_bytes(parent)?;
        self.read_bytes(child)?;
        if parent.region != child.region {
            return Err(MemoryError::CrossRegionEdge);
        }
        let (old_len, old_capacity) = {
            let r = self.region(parent.region)?;
            let object = r.slots[parent.slot]
                .object
                .as_ref()
                .ok_or(MemoryError::InvalidHandle)?;
            if object.edges.contains(&child) {
                return Ok(());
            }
            (object.edges.len(), object.edges.capacity())
        };
        if old_len < old_capacity {
            self.object_mut(parent)?.edges.push(child);
            return Ok(());
        }
        if let Err(error) = self.check_budgets(EDGE_METADATA_BYTES) {
            self.region_mut(parent.region)?.stats.allocation_failures += 1;
            return Err(MemoryError::Allocation(error));
        }
        let remaining = {
            let r = self.region(parent.region)?;
            r.limit_bytes.saturating_sub(r.stats.committed_bytes)
        };
        if EDGE_METADATA_BYTES > remaining {
            self.region_mut(parent.region)?.stats.allocation_failures += 1;
            return Err(MemoryError::Allocation(AllocationError::RegionLimit));
        }
        let mut edges = Vec::new();
        let needed = old_len.checked_add(1).ok_or_else(|| {
            self.region_mut(parent.region)
                .expect("validated region")
                .stats
                .allocation_failures += 1;
            MemoryError::Allocation(AllocationError::HostAllocation)
        })?;
        if edges.try_reserve_exact(needed).is_err() {
            self.region_mut(parent.region)?.stats.allocation_failures += 1;
            return Err(MemoryError::Allocation(AllocationError::HostAllocation));
        }
        let added = edges
            .capacity()
            .checked_sub(old_capacity)
            .and_then(|count| count.checked_mul(EDGE_METADATA_BYTES))
            .ok_or_else(|| {
                self.region_mut(parent.region)
                    .expect("validated region")
                    .stats
                    .allocation_failures += 1;
                MemoryError::Allocation(AllocationError::HostAllocation)
            })?;
        let budget_charge = edges
            .capacity()
            .checked_mul(EDGE_METADATA_BYTES)
            .ok_or_else(|| {
                self.region_mut(parent.region)
                    .expect("validated region")
                    .stats
                    .allocation_failures += 1;
                MemoryError::Allocation(AllocationError::HostAllocation)
            })?;
        if let Err(error) = self.check_budgets(budget_charge) {
            self.region_mut(parent.region)?.stats.allocation_failures += 1;
            return Err(MemoryError::Allocation(error));
        }
        if added > remaining {
            self.region_mut(parent.region)?.stats.allocation_failures += 1;
            return Err(MemoryError::Allocation(AllocationError::RegionLimit));
        }
        let r = self.region_mut(parent.region)?;
        let object = r.slots[parent.slot]
            .object
            .as_mut()
            .ok_or(MemoryError::InvalidHandle)?;
        edges.extend_from_slice(&object.edges);
        edges.push(child);
        object.edges = edges;
        object.charge += added;
        r.stats.committed_bytes += added;
        r.stats.live_bytes += added;
        r.stats.peak_bytes = r.stats.peak_bytes.max(r.stats.committed_bytes);
        self.charge_budgets(budget_charge);
        Ok(())
    }

    pub fn close_region(&mut self, region: RegionId) -> Result<(), MemoryError> {
        let r = self.region_mut(region)?;
        if r.slots
            .iter()
            .any(|slot| slot.object.as_ref().is_some_and(|object| object.roots > 0))
        {
            return Err(MemoryError::RegionHasRoots);
        }
        r.slots = Vec::new();
        r.stats.committed_bytes = 0;
        r.stats.live_bytes = 0;
        r.closed = true;
        Ok(())
    }

    pub fn collect(&mut self, region: RegionId) -> Result<(), MemoryError> {
        if !self.budgets.is_empty() {
            return Err(MemoryError::CollectionForbidden);
        }
        let r = self.region_mut(region)?;
        for slot in &mut r.slots {
            slot.marked = slot.object.as_ref().is_some_and(|object| object.roots > 0);
        }
        loop {
            let mut changed = false;
            for i in 0..r.slots.len() {
                if !r.slots[i].marked {
                    continue;
                }
                let edge_count = r.slots[i]
                    .object
                    .as_ref()
                    .map_or(0, |object| object.edges.len());
                for j in 0..edge_count {
                    let child = r.slots[i]
                        .object
                        .as_ref()
                        .expect("marked slot occupied")
                        .edges[j];
                    let target = &mut r.slots[child.slot];
                    if target.generation == child.generation
                        && target.object.is_some()
                        && !target.marked
                    {
                        target.marked = true;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let mut collected = 0;
        for slot in &mut r.slots {
            if slot.object.is_some() && !slot.marked {
                let object = slot.object.take().expect("occupied slot");
                collected += object.charge;
                slot.generation = slot.generation.saturating_add(1);
            }
            slot.marked = false;
        }
        r.stats.committed_bytes -= collected;
        r.stats.live_bytes -= collected;
        r.stats.total_collected_bytes += collected;
        r.stats.collection_count += 1;
        Ok(())
    }
}
