//! Deterministic failure of the runtime's fallible host reserve calls.

/// The fallible host reserve reached by an allocation attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReserveSite {
    RegionTable,
    ManagedBytes,
    ObjectPayload,
    ObjectReferences,
    SlotTable,
    OwnedBytes,
    ManagedToOwned,
    Edges,
}

#[derive(Default)]
pub(crate) struct FaultInjection {
    #[cfg(feature = "fault-injection")]
    pub(crate) attempts: usize,
    #[cfg(feature = "fault-injection")]
    fail_at: Option<usize>,
    #[cfg(feature = "fault-injection")]
    pub(crate) failed_site: Option<ReserveSite>,
}

impl FaultInjection {
    pub(crate) fn before_reserve(&mut self, site: ReserveSite) -> Result<(), ()> {
        #[cfg(feature = "fault-injection")]
        {
            self.attempts += 1;
            if self.fail_at == Some(self.attempts) {
                self.fail_at = None;
                self.failed_site = Some(site);
                return Err(());
            }
        }
        #[cfg(not(feature = "fault-injection"))]
        let _ = site;
        Ok(())
    }

    #[cfg(feature = "fault-injection")]
    pub(crate) fn arm(&mut self, index: usize) {
        assert!(index > 0, "reserve index is one-based");
        self.attempts = 0;
        self.fail_at = Some(index);
        self.failed_site = None;
    }

    #[cfg(feature = "fault-injection")]
    pub(crate) fn disable(&mut self) {
        self.fail_at = None;
    }
}
