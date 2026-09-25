use crate::buf::cursor::BufCursor;
use crate::Buf;

use std::io;

/// A `Buf` adapter which implements `io::Read` for the inner value.
///
/// This struct is generally created by calling `reader()` on `Buf`. See
/// documentation of [`reader()`](Buf::reader) for more
/// details.
#[derive(Debug)]
pub struct Reader<B> {
    cursor: BufCursor<B>,
}

pub fn new<B>(buf: B) -> Reader<B> {
    Reader {
        cursor: BufCursor::single(buf),
    }
}

impl<B: Buf> Reader<B> {
    /// Gets a reference to the underlying `Buf`.
    ///
    /// It is inadvisable to directly read from the underlying `Buf`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bytes::Buf;
    ///
    /// let buf = b"hello world".reader();
    ///
    /// assert_eq!(b"hello world", buf.get_ref());
    /// ```
    pub fn get_ref(&self) -> &B {
        self.cursor.first_ref()
    }

    /// Gets a mutable reference to the underlying `Buf`.
    ///
    /// It is inadvisable to directly read from the underlying `Buf`.
    pub fn get_mut(&mut self) -> &mut B {
        self.cursor.first_mut()
    }

    /// Consumes this `Reader`, returning the underlying value.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bytes::Buf;
    /// use std::io;
    ///
    /// let mut buf = b"hello world".reader();
    /// let mut dst = vec![];
    ///
    /// io::copy(&mut buf, &mut dst).unwrap();
    ///
    /// let buf = buf.into_inner();
    /// assert_eq!(0, buf.remaining());
    /// ```
    pub fn into_inner(self) -> B {
        self.cursor.into_front()
    }
}

impl<B: Buf + Sized> io::Read for Reader<B> {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        self.cursor.read(dst)
    }
}

impl<B: Buf + Sized> io::BufRead for Reader<B> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.cursor.fill_buf()
    }
    fn consume(&mut self, amt: usize) {
        self.cursor.consume(amt)
    }
}
