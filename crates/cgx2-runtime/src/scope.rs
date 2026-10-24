//! Compiler-consumable lexical root ownership. No source lowering is implied.
use std::ops::{Deref, DerefMut};

use crate::{AllocationError, Handle, MemoryError, ReserveSiteV2, Session};

/// Cleanup continues after an error and never replaces the caller's result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CleanupReport {
    pub released_roots: usize,
    pub first_error: Option<MemoryError>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ScopeOutcome<T, E> {
    pub result: Result<T, E>,
    pub cleanup: CleanupReport,
}

/// Owns exactly the registrations acquired by `ScopedRoots::root`.
///
/// Nested scopes borrow the session exclusively. Drop performs allocation-free
/// cleanup on normal return, cancellation by drop, and Rust panic unwind. As with
/// Rust RAII, abort, process termination and `mem::forget` do not run cleanup.
/// Manual `Session::unroot` cannot consume these protected registrations.
pub struct ScopedRoots<'a> {
    session: &'a mut Session,
    roots: Vec<Handle>,
}

impl Session {
    /// Scope entry itself does not allocate. Each root registration is fallible.
    pub fn scoped_roots(&mut self) -> ScopedRoots<'_> {
        ScopedRoots {
            session: self,
            roots: Vec::new(),
        }
    }
}

impl ScopedRoots<'_> {
    /// Nonallocating marker for a compiler expression's temporary registrations.
    pub fn checkpoint(&self) -> usize {
        self.roots.len()
    }

    pub fn contains_since(&self, checkpoint: usize, handle: Handle) -> bool {
        self.roots
            .get(checkpoint..)
            .is_some_and(|roots| roots.contains(&handle))
    }

    /// Releases expression temporaries, retaining one already registered result.
    /// Capacity is retained and this operation never allocates or collects.
    pub fn finish_since(&mut self, checkpoint: usize, retain: Option<Handle>) -> CleanupReport {
        if checkpoint > self.roots.len() {
            return CleanupReport {
                released_roots: 0,
                first_error: Some(MemoryError::NotRooted),
            };
        }
        let retained = retain.and_then(|handle| {
            self.roots[checkpoint..]
                .iter()
                .rposition(|h| *h == handle)
                .map(|index| self.roots.swap_remove(checkpoint + index))
        });
        let mut report = CleanupReport::default();
        while self.roots.len() > checkpoint {
            let handle = self.roots.pop().expect("temporary root");
            match self.session.object_mut(handle).and_then(|object| {
                if object.scoped_roots == 0 || object.roots == 0 {
                    return Err(MemoryError::NotRooted);
                }
                object.scoped_roots -= 1;
                object.roots -= 1;
                Ok(())
            }) {
                Ok(()) => report.released_roots += 1,
                Err(error) => {
                    report.first_error.get_or_insert(error);
                }
            }
        }
        if let Some(handle) = retained {
            self.roots.push(handle);
        }
        report
    }

    /// Releases one registration owned by this scope, without allocation or collection.
    /// This permits compiler temporaries and overwritten bindings to end promptly.
    pub fn release(&mut self, handle: Handle) -> Result<(), MemoryError> {
        let index = self
            .roots
            .iter()
            .rposition(|h| *h == handle)
            .ok_or(MemoryError::NotRooted)?;
        let object = self.session.object_mut(handle)?;
        if object.scoped_roots == 0 || object.roots == 0 {
            return Err(MemoryError::NotRooted);
        }
        object.scoped_roots -= 1;
        object.roots -= 1;
        self.roots.swap_remove(index);
        Ok(())
    }

    /// Runs a lexical guard with the same protected registration table and Session.
    /// Moving the table performs no allocation. Nested scopes still clean up on unwind.
    pub fn with_no_gc<T>(
        &mut self,
        budget: usize,
        body: impl FnOnce(&mut ScopedRoots<'_>) -> T,
    ) -> Result<T, AllocationError> {
        let mut guard = self.session.try_no_gc(budget)?;
        let registrations = std::mem::take(&mut self.roots);
        let mut nested = ScopedRoots {
            session: &mut guard,
            roots: registrations,
        };
        // Restore the registration owner even when a caller catches an unwind
        // without dropping its outer scope. Then resume the original panic.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(&mut nested)));
        self.roots = std::mem::take(&mut nested.roots);
        match result {
            Ok(value) => Ok(value),
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    /// Registers one protected root. Failure leaves registrations unchanged.
    /// Root-table growth is charged to every active no_gc budget, not a region.
    pub fn root(&mut self, handle: Handle) -> Result<(), MemoryError> {
        self.session
            .object_mut(handle)?
            .roots
            .checked_add(1)
            .ok_or(MemoryError::InvalidHandle)?;
        self.root_many(&[Some(handle)])
    }

    /// Registers a complete evaluated child list before an allocating safepoint.
    /// Invalid handles and counter overflow leave registrations unchanged. A
    /// successful table reserve remains charged even if registration fails.
    pub fn root_many(&mut self, handles: &[Option<Handle>]) -> Result<(), MemoryError> {
        let count = handles.iter().flatten().count();
        for handle in handles.iter().flatten() {
            self.session.read_bytes(*handle)?;
        }
        self.reserve_root_slots_v1(count)?;
        let initial = self.roots.len();
        for handle in handles.iter().flatten().copied() {
            let object = self.session.object_mut(handle)?;
            let Some(next) = object.roots.checked_add(1) else {
                while self.roots.len() > initial {
                    let registered = self.roots.pop().expect("new registration");
                    let object = self.session.object_mut(registered)?;
                    object.roots -= 1;
                    object.scoped_roots -= 1;
                }
                return Err(MemoryError::InvalidHandle);
            };
            object.roots = next;
            object.scoped_roots += 1;
            self.roots.push(handle);
        }
        Ok(())
    }

    /// Prepare publication before an irreversible graph transfer. Reservations
    /// remain charged even if later payload preparation fails.
    pub(crate) fn reserve_root_slots_v1(&mut self, count: usize) -> Result<(), MemoryError> {
        let needed = self
            .roots
            .len()
            .checked_add(count)
            .ok_or(MemoryError::Allocation(AllocationError::HostAllocation))?;
        if needed > self.roots.capacity() {
            let bytes = needed
                .checked_mul(std::mem::size_of::<Handle>())
                .ok_or(MemoryError::Allocation(AllocationError::HostAllocation))?;
            self.session
                .check_budgets(bytes)
                .map_err(MemoryError::Allocation)?;
            self.session
                .fault
                .before_reserve_v2(ReserveSiteV2::ScopedRoots)
                .map_err(|_| MemoryError::Allocation(AllocationError::HostAllocation))?;
            let mut replacement: Vec<Handle> = Vec::new();
            replacement
                .try_reserve_exact(needed)
                .map_err(|_| MemoryError::Allocation(AllocationError::HostAllocation))?;
            let charge = replacement
                .capacity()
                .checked_mul(std::mem::size_of::<Handle>())
                .ok_or(MemoryError::Allocation(AllocationError::HostAllocation))?;
            self.session
                .check_budgets(charge)
                .map_err(MemoryError::Allocation)?;
            self.session.charge_budgets(charge);
            replacement.extend_from_slice(&self.roots);
            self.roots = replacement;
        }
        Ok(())
    }

    /// Drains in reverse registration order, including duplicate handles.
    /// Repeated cleanup is a no-op; table capacity is retained for scope reuse.
    pub fn cleanup(&mut self) -> CleanupReport {
        let mut report = CleanupReport::default();
        while let Some(handle) = self.roots.pop() {
            let released = self.session.object_mut(handle).and_then(|object| {
                if object.scoped_roots == 0 || object.roots == 0 {
                    return Err(MemoryError::NotRooted);
                }
                object.scoped_roots -= 1;
                object.roots -= 1;
                Ok(())
            });
            match released {
                Ok(()) => report.released_roots += 1,
                Err(error) => {
                    report.first_error.get_or_insert(error);
                }
            }
        }
        report
    }

    /// Preserves success or the original error (including caller cancellation),
    /// with separate cleanup diagnostics. Also safe to call before Drop.
    pub fn finish<T, E>(&mut self, result: Result<T, E>) -> ScopeOutcome<T, E> {
        ScopeOutcome {
            result,
            cleanup: self.cleanup(),
        }
    }
}

impl Deref for ScopedRoots<'_> {
    type Target = Session;
    fn deref(&self) -> &Session {
        self.session
    }
}

impl DerefMut for ScopedRoots<'_> {
    fn deref_mut(&mut self) -> &mut Session {
        self.session
    }
}

impl Drop for ScopedRoots<'_> {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow_and_invalid_handles_leave_scope_empty() {
        let mut session = Session::try_new().unwrap();
        let handle = session.alloc_bytes(session.default_region(), 1).unwrap();
        session.object_mut(handle).unwrap().roots = usize::MAX;
        let mut scope = session.scoped_roots();
        assert_eq!(scope.root(handle), Err(MemoryError::InvalidHandle));
        assert_eq!(scope.cleanup(), CleanupReport::default());
        scope.object_mut(handle).unwrap().roots = 0;
        scope.collect(handle.region()).unwrap();
        assert_eq!(scope.root(handle), Err(MemoryError::InvalidHandle));
        assert_eq!(scope.cleanup(), CleanupReport::default());
    }

    #[test]
    fn cleanup_continues_after_corruption_and_preserves_primary_error() {
        // Internal corruption injection, not a supported compiler operation.
        let mut session = Session::try_new().unwrap();
        let a = session.alloc_bytes(session.default_region(), 1).unwrap();
        let b = session.alloc_bytes(session.default_region(), 1).unwrap();
        let mut scope = session.scoped_roots();
        scope.root(a).unwrap();
        scope.root(b).unwrap();
        scope.object_mut(b).unwrap().scoped_roots = 0;
        let outcome = scope.finish::<(), _>(Err("primary failure"));
        assert_eq!(outcome.result, Err("primary failure"));
        assert_eq!(
            outcome.cleanup,
            CleanupReport {
                released_roots: 1,
                first_error: Some(MemoryError::NotRooted),
            }
        );
        assert_eq!(scope.cleanup(), CleanupReport::default());
        assert_eq!(scope.object_mut(a).unwrap().roots, 0);
        scope.unroot(b).unwrap();
    }

    #[test]
    fn identifier_exhaustion_is_fallible_and_does_not_wrap() {
        use std::sync::atomic::{AtomicU64, Ordering};
        let counter = AtomicU64::new(u64::MAX - 1);
        assert_eq!(crate::try_session_id(&counter), Ok(u64::MAX - 1));
        assert_eq!(
            crate::try_session_id(&counter),
            Err(crate::SessionEntryError::IdentifierExhausted)
        );
        assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
    }
}
