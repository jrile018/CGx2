use crate::region::{CollectionMode, Handle, RegionStats};
use crate::PauseReport;

pub(crate) struct Object {
    pub bytes: Vec<u8>,
    pub edges: Vec<Handle>,
    pub roots: usize,
    pub charge: usize,
}

#[derive(Default)]
pub(crate) struct Slot {
    pub generation: u64,
    pub object: Option<Object>,
    pub marked: bool,
}

pub(crate) struct Region {
    pub mode: CollectionMode,
    pub limit_bytes: usize,
    pub slots: Vec<Slot>,
    pub stats: RegionStats,
    pub pause: PauseReport,
    pub closed: bool,
}

impl Region {
    pub fn new(mode: CollectionMode, limit_bytes: usize) -> Self {
        Self {
            mode,
            limit_bytes,
            slots: Vec::new(),
            stats: RegionStats::default(),
            pause: PauseReport::default(),
            closed: false,
        }
    }

    pub fn vacant_slot(&mut self) -> Result<usize, ()> {
        if let Some(index) = self
            .slots
            .iter()
            .position(|slot| slot.object.is_none() && slot.generation < u64::MAX)
        {
            return Ok(index);
        }
        self.slots.try_reserve(1).map_err(|_| ())?;
        self.slots.push(Slot::default());
        Ok(self.slots.len() - 1)
    }
}
