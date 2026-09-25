use crate::sync::{fence, AtomicUsize, Ordering};
use std::cell::Cell;
use std::mem;

// The write handle is the owner of the reader lifecycle, so guard issuance lives here too,
// next to the guard types it produces.
use crate::write::WriteHandle;
use crate::Absorb;

#[derive(Debug, Copy, Clone)]
pub(super) struct ReadHandleState<'rh> {
    pub(super) epoch: &'rh AtomicUsize,
    pub(super) enters: &'rh Cell<usize>,
}

impl<'rh, T> From<&'rh super::ReadHandle<T>> for ReadHandleState<'rh> {
    fn from(rh: &'rh super::ReadHandle<T>) -> Self {
        Self {
            epoch: &rh.epoch,
            enters: &rh.enters,
        }
    }
}

/// A guard wrapping a live reference into a left-right protected `T`.
///
/// As long as this guard lives, the `T` being read cannot change. If a writer attempts to call
/// [`WriteHandle::publish`](crate::WriteHandle::publish), that call will block until this guard is
/// dropped.
///
/// To scope the guard to a subset of the data in `T`, use [`map`](Self::map) and
/// [`try_map`](Self::try_map).
#[derive(Debug)]
pub struct ReadGuard<'rh, T: ?Sized> {
    // NOTE: _technically_ this is more like &'self.
    // the reference is valid until the guard is dropped.
    pub(super) t: &'rh T,
    pub(super) handle: ReadHandleState<'rh>,
}

impl<'rh, T> ReadGuard<'rh, T> {
    /// Assemble a guard over `t` bound to the live reader state of `r_handle`.
    ///
    /// Must only be called after the guard bookkeeping (`enters` and the epoch bump) has been
    /// performed for the entering side: this constructor pairs the data with the state, it does
    /// not touch the epoch table itself.
    pub(super) fn enter_state(r_handle: &'rh super::ReadHandle<T>, t: &'rh T) -> Self {
        ReadGuard {
            t,
            handle: ReadHandleState {
                epoch: &r_handle.epoch,
                enters: &r_handle.enters,
            },
        }
    }
}

impl<'rh, T: ?Sized> ReadGuard<'rh, T> {
    /// Makes a new `ReadGuard` for a component of the borrowed data.
    ///
    /// This is an associated function that needs to be used as `ReadGuard::map(...)`, since
    /// a method would interfere with methods of the same name on the contents of a `Readguard`
    /// used through `Deref`.
    ///
    /// # Examples
    ///
    /// ```
    /// use left_right::{ReadGuard, ReadHandle};
    ///
    /// fn get_str(handle: &ReadHandle<Vec<(String, i32)>>, i: usize) -> Option<ReadGuard<'_, str>> {
    ///     handle.enter().map(|guard| {
    ///         ReadGuard::map(guard, |t| {
    ///             &*t[i].0
    ///         })
    ///     })
    /// }
    /// ```
    pub fn map<F, U: ?Sized>(orig: Self, f: F) -> ReadGuard<'rh, U>
    where
        F: for<'a> FnOnce(&'a T) -> &'a U,
    {
        let rg = ReadGuard {
            t: f(orig.t),
            handle: orig.handle,
        };
        mem::forget(orig);
        rg
    }

    /// Makes a new `ReadGuard` for a component of the borrowed data that may not exist.
    ///
    /// This method differs from [`map`](Self::map) in that it drops the guard if the closure maps
    /// to `None`. This allows you to "lift" a `ReadGuard<Option<T>>` into an
    /// `Option<ReadGuard<T>>`.
    ///
    /// This is an associated function that needs to be used as `ReadGuard::try_map(...)`, since
    /// a method would interfere with methods of the same name on the contents of a `Readguard`
    /// used through `Deref`.
    ///
    /// # Examples
    ///
    /// ```
    /// use left_right::{ReadGuard, ReadHandle};
    ///
    /// fn try_get_str(handle: &ReadHandle<Vec<(String, i32)>>, i: usize) -> Option<ReadGuard<'_, str>> {
    ///     handle.enter().and_then(|guard| {
    ///         ReadGuard::try_map(guard, |t| {
    ///             t.get(i).map(|v| &*v.0)
    ///         })
    ///     })
    /// }
    /// ```
    pub fn try_map<F, U: ?Sized>(orig: Self, f: F) -> Option<ReadGuard<'rh, U>>
    where
        F: for<'a> FnOnce(&'a T) -> Option<&'a U>,
    {
        let rg = ReadGuard {
            t: f(orig.t)?,
            handle: orig.handle,
        };
        mem::forget(orig);
        Some(rg)
    }
}

// The write handle owns guard issuance as part of the reader lifecycle it coordinates: writers
// enter the read side through their own handle rather than dereferencing into the reader.
impl<T, O> WriteHandle<T, O>
where
    T: Absorb<O>,
{
    /// Enter the read side directly through the writer handle.
    ///
    /// This hands out a guard over the last published state, exactly like entering through
    /// [`ReadHandle::enter`](crate::ReadHandle::enter) on the writer's reader would. While the
    /// guard lives, a subsequent call to [`WriteHandle::publish`](crate::WriteHandle::publish)
    /// cannot proceed.
    ///
    /// If the writer handle has been destroyed this returns `None`.
    pub fn enter_read_guard(&self) -> Option<ReadGuard<'_, T>> {
        let r_handle = &self.r_handle;
        let enters = r_handle.enters.get();
        if enters != 0 {
            // we have already locked the epoch; just give out another guard.
            let inner = r_handle.inner.load(Ordering::Acquire);
            // since we previously bumped our epoch, this pointer will remain valid until we bump
            // it again, which only happens when the last ReadGuard is dropped.
            let data = unsafe { inner.as_ref() };

            return if let Some(data) = data {
                r_handle.enters.set(enters + 1);
                Some(ReadGuard::enter_state(r_handle, data))
            } else {
                // WriteHandle must have been dropped, so even though the first `enter` may have
                // returned `Some`, all we can do now is return `None` since the pointee has
                // vanished.
                None
            };
        }

        // entering from the writer side has to satisfy the same visibility contract as
        // entering from a reader: the reasoning for why reading the pointer _after_ bumping the
        // epoch is safe is laid out in `ReadHandle::enter`, and applies verbatim here.

        // so, update the epoch tracker.
        r_handle.epoch.fetch_add(1, Ordering::AcqRel);

        // ensure that the pointer read happens strictly after updating the epoch
        fence(Ordering::SeqCst);

        // then, atomically read the pointer, and use the copy being pointed to.
        // since we bumped the epoch, this pointer will remain valid until we bump it again.
        let inner = r_handle.inner.load(Ordering::Acquire);
        let data = unsafe { inner.as_ref() };

        if let Some(data) = data {
            // add a guard to ensure we restore read parity even if we panic
            r_handle.enters.set(r_handle.enters.get() + 1);
            Some(ReadGuard::enter_state(r_handle, data))
        } else {
            // the writehandle has been dropped, and so has both copies,
            // so restore parity and return None
            r_handle.epoch.fetch_add(1, Ordering::AcqRel);
            None
        }
    }
}

impl<'rh, T: ?Sized> AsRef<T> for ReadGuard<'rh, T> {
    fn as_ref(&self) -> &T {
        self.t
    }
}

impl<'rh, T: ?Sized> std::ops::Deref for ReadGuard<'rh, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.t
    }
}

impl<'rh, T: ?Sized> Drop for ReadGuard<'rh, T> {
    fn drop(&mut self) {
        let enters = self.handle.enters.get() - 1;
        self.handle.enters.set(enters);
        if enters == 0 {
            // We are the last guard to be dropped -- now release our epoch.
            self.handle.epoch.fetch_add(1, Ordering::AcqRel);
        }
    }
}
