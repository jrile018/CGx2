use crate::{AllocationError, Handle, MemoryError, ReserveSite, Session};

/// Fixed logical header charge for a separately owned byte buffer.
pub const OWNED_METADATA_BYTES: usize = 32;

/// A uniquely owned byte buffer, released when the Rust value is dropped.
///
/// This value cannot be cloned; an explicit managed-to-owned copy is available
/// through [`Session::copy_managed_to_owned`].
///
/// ```compile_fail
/// use cgx2_runtime::Session;
/// let mut session = Session::new();
/// let value = session.alloc_owned_bytes(1).unwrap();
/// let second_owner = value.clone();
/// ```
#[derive(Debug)]
pub struct OwnedBytes {
    pub(crate) bytes: Vec<u8>,
}

impl OwnedBytes {
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl Session {
    pub fn alloc_owned_bytes(&mut self, len: usize) -> Result<OwnedBytes, AllocationError> {
        let minimum = len
            .checked_add(OWNED_METADATA_BYTES)
            .ok_or(AllocationError::HostAllocation)?;
        self.check_budgets(minimum)?;
        let mut bytes = Vec::new();
        if len > 0 {
            self.fault
                .before_reserve(ReserveSite::OwnedBytes)
                .map_err(|_| AllocationError::HostAllocation)?;
        }
        bytes
            .try_reserve_exact(len)
            .map_err(|_| AllocationError::HostAllocation)?;
        let charge = bytes
            .capacity()
            .checked_add(OWNED_METADATA_BYTES)
            .ok_or(AllocationError::HostAllocation)?;
        self.check_budgets(charge)?;
        bytes.resize(len, 0);
        self.charge_budgets(charge);
        Ok(OwnedBytes { bytes })
    }

    /// Copies the managed payload into separately owned storage. The handle
    /// remains valid, subject to the ordinary region collection rules.
    pub fn copy_managed_to_owned(&mut self, handle: Handle) -> Result<OwnedBytes, MemoryError> {
        let len = self.read_bytes(handle)?.len();
        let minimum = len
            .checked_add(OWNED_METADATA_BYTES)
            .ok_or(MemoryError::Allocation(AllocationError::HostAllocation))?;
        self.check_budgets(minimum)
            .map_err(MemoryError::Allocation)?;
        let mut bytes = Vec::new();
        if len > 0 {
            self.fault
                .before_reserve(ReserveSite::ManagedToOwned)
                .map_err(|_| MemoryError::Allocation(AllocationError::HostAllocation))?;
        }
        bytes
            .try_reserve_exact(len)
            .map_err(|_| MemoryError::Allocation(AllocationError::HostAllocation))?;
        let charge = bytes
            .capacity()
            .checked_add(OWNED_METADATA_BYTES)
            .ok_or(MemoryError::Allocation(AllocationError::HostAllocation))?;
        self.check_budgets(charge)
            .map_err(MemoryError::Allocation)?;
        bytes.extend_from_slice(self.read_bytes(handle)?);
        self.charge_budgets(charge);
        Ok(OwnedBytes { bytes })
    }
}
