//! Shared cursor mechanics for the buffer adapter family.
//!
//! Every adapter in this module tree — [`Take`], [`Limit`], [`Chain`],
//! [`Reader`], [`Writer`] and the byte [`IntoIter`] — is a windowed view over
//! one or two byte regions. They used to each re-implement the same cursor
//! arithmetic (region traversal, windowing, slice accounting) with slightly
//! different rules, and their `advance` bookkeeping drifted apart.
//!
//! [`BufCursor`] centralizes those rules in one engine: which region is
//! active, how a request is split across paired regions, how a window budget
//! is applied and decremented, and how much traffic has moved through the
//! cursor. The adapters keep their public shapes and delegate the mechanics
//! here.
//!
//! [`Take`]: crate::buf::Take
//! [`Limit`]: crate::buf::Limit
//! [`Chain`]: crate::buf::Chain
//! [`Reader`]: crate::buf::Reader
//! [`Writer`]: crate::buf::Writer
//! [`IntoIter`]: crate::buf::IntoIter

use crate::buf::UninitSlice;
use crate::{Buf, BufMut};

use core::cmp;

#[cfg(feature = "std")]
use std::io::IoSlice;

/// The empty region backing single-region cursors.
///
/// Single-region cursors carry `()` as their region type so that every
/// construction satisfies the same bounds; no bytes are ever read from or
/// written to it.
impl Buf for () {
    fn remaining(&self) -> usize {
        0
    }

    fn chunk(&self) -> &[u8] {
        &[]
    }

    fn advance(&mut self, cnt: usize) {
        assert!(cnt == 0, "an empty region cannot advance past its end");
    }
}

// SAFETY: the empty region never exposes a writable chunk, and `advance_mut`
// rejects every non-zero request, so no uninitialized byte is ever promised.
unsafe impl BufMut for () {
    fn remaining_mut(&self) -> usize {
        0
    }

    fn chunk_mut(&mut self) -> &mut UninitSlice {
        UninitSlice::new(&mut [])
    }

    unsafe fn advance_mut(&mut self, cnt: usize) {
        assert!(cnt == 0, "an empty region cannot advance past its end");
    }
}

/// A unified cursor over one or two byte regions.
///
/// A cursor is either *single-region* — it exposes only `front` — or
/// *paired* — reads and writes drain `front` first and then continue into
/// `tail`. A `limit` acts as a window budget in front of the active region;
/// unlimited cursors carry the `usize::MAX` sentinel so that window tests
/// never reject them. `consumed` accumulates the traffic the cursor has
/// moved, in either direction; no adapter reports it yet, but keeping the
/// accounting in the shared engine means a future telemetry pass can read
/// it without re-instrumenting every cursor rule.
#[derive(Debug)]
pub(crate) struct BufCursor<T, U = ()> {
    /// The region that is always consulted first.
    front: T,
    /// The second region of a paired cursor; `None` for single-region
    /// cursors.
    tail: Option<U>,
    /// The number of bytes the window still admits, `usize::MAX` when the
    /// cursor is unlimited.
    limit: usize,
    /// The number of bytes moved through this cursor in either direction.
    consumed: usize,
}

impl<T> BufCursor<T> {
    /// Creates a single-region cursor over `front` with no window.
    pub(crate) fn single(front: T) -> Self {
        BufCursor {
            front,
            tail: None,
            limit: usize::MAX,
            consumed: 0,
        }
    }

    /// Creates a single-region cursor over `front` whose window admits at
    /// most `limit` bytes.
    pub(crate) fn with_limit(front: T, limit: usize) -> Self {
        BufCursor {
            front,
            tail: None,
            limit,
            consumed: 0,
        }
    }

    /// Returns the number of bytes the window still admits.
    pub(crate) fn limit(&self) -> usize {
        self.limit
    }

    /// Shrinks or grows the window to `lim` bytes.
    pub(crate) fn set_limit(&mut self, lim: usize) {
        self.limit = lim
    }
}

impl<T, U> BufCursor<T, U> {
    /// Creates a paired cursor draining `front` before `tail`.
    pub(crate) fn paired(front: T, tail: U) -> Self {
        BufCursor {
            front,
            tail: Some(tail),
            limit: usize::MAX,
            consumed: 0,
        }
    }

    /// Gets a reference to the front region.
    pub(crate) fn first_ref(&self) -> &T {
        &self.front
    }

    /// Gets a mutable reference to the front region.
    pub(crate) fn first_mut(&mut self) -> &mut T {
        &mut self.front
    }

    /// Consumes the cursor, returning the front region; the cursor must not
    /// be paired.
    pub(crate) fn into_front(self) -> T {
        self.front
    }

    /// Consumes the cursor, returning both regions; the cursor must be
    /// paired.
    pub(crate) fn into_regions(self) -> (T, U) {
        let tail = self
            .tail
            .expect("paired cursors always carry a tail region");
        (self.front, tail)
    }

    /// Gets a reference to the second region; the cursor must be paired.
    pub(crate) fn last_ref(&self) -> &U {
        self.paired_tail()
    }

    /// Gets a mutable reference to the second region; the cursor must be
    /// paired.
    pub(crate) fn last_mut(&mut self) -> &mut U {
        self.paired_tail_mut()
    }

    fn paired_tail(&self) -> &U {
        self.tail
            .as_ref()
            .expect("paired cursors always carry a tail region")
    }

    fn paired_tail_mut(&mut self) -> &mut U {
        self.tail
            .as_mut()
            .expect("paired cursors always carry a tail region")
    }
}

impl<T: Buf, U: Buf> Buf for BufCursor<T, U> {
    fn remaining(&self) -> usize {
        match self.tail.as_ref() {
            Some(back) => self.front.remaining().saturating_add(back.remaining()),
            None => cmp::min(self.front.remaining(), self.limit),
        }
    }

    fn chunk(&self) -> &[u8] {
        if let Some(back) = self.tail.as_ref() {
            if self.front.has_remaining() {
                self.front.chunk()
            } else {
                back.chunk()
            }
        } else {
            let bytes = self.front.chunk();
            &bytes[..cmp::min(bytes.len(), self.limit)]
        }
    }

    fn advance(&mut self, mut cnt: usize) {
        let requested = cnt;
        assert!(cnt <= self.limit);

        if let Some(back) = self.tail.as_mut() {
            let a_rem = self.front.remaining();

            if a_rem != 0 {
                if a_rem >= cnt {
                    self.front.advance(cnt);
                    self.consumed += requested;
                    return;
                }

                // Consume what is left of the front region.
                self.front.advance(a_rem);

                cnt -= a_rem;
            }

            back.advance(cnt);
            self.consumed += requested;
            return;
        }

        self.front.advance(cnt);
        self.limit -= cnt;
        self.consumed += requested;
    }

    fn copy_to_bytes(&mut self, len: usize) -> crate::Bytes {
        assert!(len <= self.remaining(), "`len` greater than remaining");

        if let Some(back) = self.tail.as_mut() {
            let a_rem = self.front.remaining();
            let ret = if a_rem >= len {
                self.front.copy_to_bytes(len)
            } else if a_rem == 0 {
                back.copy_to_bytes(len)
            } else {
                assert!(
                    len - a_rem <= back.remaining(),
                    "`len` greater than remaining"
                );
                let mut ret = crate::BytesMut::with_capacity(len);
                ret.put(&mut self.front);
                ret.put((&mut *back).take(len - a_rem));
                ret.freeze()
            };
            self.consumed += len;
            return ret;
        }

        let ret = self.front.copy_to_bytes(len);
        self.limit -= len;
        self.consumed += len;
        ret
    }

    #[cfg(feature = "std")]
    fn chunks_vectored<'a>(&'a self, dst: &mut [IoSlice<'a>]) -> usize {
        if self.limit == 0 {
            return 0;
        }

        if let Some(back) = self.tail.as_ref() {
            let mut n = self.front.chunks_vectored(dst);
            n += back.chunks_vectored(&mut dst[n..]);
            return n;
        }

        const LEN: usize = 16;
        let mut slices: [IoSlice<'a>; LEN] = [IoSlice::new(&[]); LEN];

        let cnt = self
            .front
            .chunks_vectored(&mut slices[..dst.len().min(LEN)]);
        let mut limit = self.limit;
        for (i, (dst, slice)) in dst[..cnt].iter_mut().zip(slices.iter()).enumerate() {
            if let Some(buf) = slice.get(..limit) {
                // SAFETY: We could do this safely with `IoSlice::advance` if we had a larger MSRV.
                let buf = unsafe { std::mem::transmute::<&[u8], &'a [u8]>(buf) };
                *dst = IoSlice::new(buf);
                return i + 1;
            } else {
                // SAFETY: We could do this safely with `IoSlice::advance` if we had a larger MSRV.
                let buf = unsafe { std::mem::transmute::<&[u8], &'a [u8]>(slice) };
                *dst = IoSlice::new(buf);
                limit -= slice.len();
            }
        }
        cnt
    }
}

// SAFETY: `BufCursor` forwards every write to the active region's own
// `BufMut` implementation and never initializes bytes the region did not
// promise.
unsafe impl<T: BufMut, U: BufMut> BufMut for BufCursor<T, U> {
    fn remaining_mut(&self) -> usize {
        match self.tail.as_ref() {
            Some(back) => self
                .front
                .remaining_mut()
                .saturating_add(back.remaining_mut()),
            None => cmp::min(self.front.remaining_mut(), self.limit),
        }
    }

    fn chunk_mut(&mut self) -> &mut UninitSlice {
        if let Some(back) = self.tail.as_mut() {
            if self.front.has_remaining_mut() {
                self.front.chunk_mut()
            } else {
                back.chunk_mut()
            }
        } else {
            let bytes = self.front.chunk_mut();
            let end = cmp::min(bytes.len(), self.limit);
            &mut bytes[..end]
        }
    }

    unsafe fn advance_mut(&mut self, mut cnt: usize) {
        let requested = cnt;
        assert!(cnt <= self.limit);

        if let Some(back) = self.tail.as_mut() {
            let a_rem = self.front.remaining_mut();

            if a_rem != 0 {
                if a_rem >= cnt {
                    self.front.advance_mut(cnt);
                    self.consumed += requested;
                    return;
                }

                // Consume what is left of the front region.
                self.front.advance_mut(a_rem);

                cnt -= a_rem;
            }

            back.advance_mut(cnt);
            self.consumed += requested;
            return;
        }

        self.front.advance_mut(cnt);
        self.limit -= cnt;
        self.consumed += requested;
    }
}

/// Bridges the cursor's read side into the framed `io` read family.
#[cfg(feature = "std")]
impl<T: Buf, U: Buf> std::io::Read for BufCursor<T, U> {
    fn read(&mut self, dst: &mut [u8]) -> std::io::Result<usize> {
        // Fast path: a single-region cursor transfers straight out of its
        // front region and books the traffic locally, without entering the
        // generic cursor machinery.
        if self.tail.is_none() {
            let len = cmp::min(self.front.remaining(), dst.len());
            Buf::copy_to_slice(&mut self.front, &mut dst[0..len]);
            self.consumed += len;
            return Ok(len);
        }

        let len = cmp::min(self.remaining(), dst.len());
        Buf::copy_to_slice(self, &mut dst[0..len]);
        Ok(len)
    }
}

/// Presents the active region's bytes as an `io` buffer.
#[cfg(feature = "std")]
impl<T: Buf, U: Buf> std::io::BufRead for BufCursor<T, U> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        let chunk: &[u8] = Buf::chunk(self);
        Ok(chunk)
    }

    fn consume(&mut self, amt: usize) {
        self.advance(amt)
    }
}

/// Bridges the cursor's write side into the framed `io` write family.
#[cfg(feature = "std")]
impl<T: BufMut, U: BufMut> std::io::Write for BufCursor<T, U> {
    fn write(&mut self, src: &[u8]) -> std::io::Result<usize> {
        // Fast path: a single-region cursor accepts straight into its front
        // region and books the traffic locally.
        if self.tail.is_none() {
            let n = cmp::min(self.front.remaining_mut(), src.len());
            self.front.put_slice(&src[..n]);
            self.consumed += n;
            return Ok(n);
        }

        let n = cmp::min(self.remaining_mut(), src.len());
        BufMut::put_slice(self, &src[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Presents the cursor as a stream of individual bytes.
impl<T: Buf, U: Buf> Iterator for BufCursor<T, U> {
    type Item = u8;

    fn next(&mut self) -> Option<u8> {
        // Fast path: a single-region cursor yields straight from its front
        // region and books the traffic locally.
        if self.tail.is_none() {
            if !self.front.has_remaining() {
                return None;
            }

            let b = self.front.chunk()[0];
            self.front.advance(1);
            self.consumed += 1;
            return Some(b);
        }

        if !self.has_remaining() {
            return None;
        }

        let b = self.chunk()[0];
        self.advance(1);
        Some(b)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let rem = self.remaining();
        (rem, Some(rem))
    }
}
