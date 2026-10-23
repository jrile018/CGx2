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

/// Additive reserve audit for fallible entry and scoped lifetime APIs.
/// The original `ReserveSite` remains exhaustive and unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReserveSiteV2 {
    Existing(ReserveSite),
    SessionRegions,
    GuardBudgets,
    ScopedRoots,
}

#[derive(Default)]
pub(crate) struct FaultInjection {
    #[cfg(feature = "fault-injection")]
    pub(crate) attempts: usize,
    #[cfg(feature = "fault-injection")]
    fail_at: Option<usize>,
    #[cfg(feature = "fault-injection")]
    pub(crate) failed_site: Option<ReserveSite>,
    #[cfg(feature = "fault-injection")]
    pub(crate) failed_site_v2: Option<ReserveSiteV2>,
}

impl FaultInjection {
    pub(crate) fn before_reserve(&mut self, site: ReserveSite) -> Result<(), ()> {
        self.before_reserve_v2(ReserveSiteV2::Existing(site))
    }

    pub(crate) fn before_reserve_v2(&mut self, site: ReserveSiteV2) -> Result<(), ()> {
        #[cfg(feature = "fault-injection")]
        {
            self.attempts += 1;
            if self.fail_at == Some(self.attempts) {
                self.fail_at = None;
                self.failed_site = match site {
                    ReserveSiteV2::Existing(legacy) => Some(legacy),
                    _ => None,
                };
                self.failed_site_v2 = Some(site);
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
        self.failed_site_v2 = None;
    }

    #[cfg(feature = "fault-injection")]
    pub(crate) fn disable(&mut self) {
        self.fail_at = None;
    }
}
