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
    /// Registers one protected root. Failure leaves registrations unchanged.
    /// Root-table growth is charged to every active no_gc budget, not a region.
    pub fn root(&mut self, handle: Handle) -> Result<(), MemoryError> {
        self.session
            .object_mut(handle)?
            .roots
            .checked_add(1)
            .ok_or(MemoryError::InvalidHandle)?;
        if self.roots.len() == self.roots.capacity() {
            let needed = self
                .roots
                .len()
                .checked_add(1)
                .ok_or(MemoryError::Allocation(AllocationError::HostAllocation))?;
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
            let mut replacement = Vec::new();
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
        let object = self.session.object_mut(handle)?;
        object.roots += 1;
        object.scoped_roots += 1;
        self.roots.push(handle);
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
