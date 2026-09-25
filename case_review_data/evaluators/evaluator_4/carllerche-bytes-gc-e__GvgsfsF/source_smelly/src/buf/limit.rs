use crate::buf::cursor::BufCursor;
use crate::buf::UninitSlice;
use crate::BufMut;

/// A `BufMut` adapter which limits the amount of bytes that can be written
/// to an underlying buffer.
#[derive(Debug)]
pub struct Limit<T> {
    cursor: BufCursor<T>,
}

pub(super) fn new<T>(inner: T, limit: usize) -> Limit<T> {
    Limit {
        cursor: BufCursor::with_limit(inner, limit),
    }
}

impl<T> Limit<T> {
    /// Consumes this `Limit`, returning the underlying value.
    pub fn into_inner(self) -> T {
        self.cursor.into_front()
    }

    /// Gets a reference to the underlying `BufMut`.
    ///
    /// It is inadvisable to directly write to the underlying `BufMut`.
    pub fn get_ref(&self) -> &T {
        self.cursor.first_ref()
    }

    /// Gets a mutable reference to the underlying `BufMut`.
    ///
    /// It is inadvisable to directly write to the underlying `BufMut`.
    pub fn get_mut(&mut self) -> &mut T {
        self.cursor.first_mut()
    }

    /// Returns the maximum number of bytes that can be written
    ///
    /// # Note
    ///
    /// If the inner `BufMut` has fewer bytes than indicated by this method then
    /// that is the actual number of available bytes.
    pub fn limit(&self) -> usize {
        self.cursor.limit()
    }

    /// Sets the maximum number of bytes that can be written.
    ///
    /// # Note
    ///
    /// If the inner `BufMut` has fewer bytes than `lim` then that is the actual
    /// number of available bytes.
    pub fn set_limit(&mut self, lim: usize) {
        self.cursor.set_limit(lim)
    }
}

unsafe impl<T: BufMut> BufMut for Limit<T> {
    fn remaining_mut(&self) -> usize {
        self.cursor.remaining_mut()
    }

    fn chunk_mut(&mut self) -> &mut UninitSlice {
        self.cursor.chunk_mut()
    }

    unsafe fn advance_mut(&mut self, cnt: usize) {
        self.cursor.advance_mut(cnt)
    }
}
