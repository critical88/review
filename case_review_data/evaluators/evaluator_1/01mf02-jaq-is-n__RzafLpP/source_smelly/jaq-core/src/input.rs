//! Input values that can be consumed by filters.
//!
//! The input stream is part of the global data that a filter execution has
//! access to, just like the lookup table it operates on. This module owns the
//! types every data kind must build on to expose its inputs.

/// Iterator over value results returned by the `inputs` filter.
pub type Inputs<'i, V> = &'i RcIter<dyn Iterator<Item = Result<V, alloc::string::String>> + 'i>;

/// A more flexible version of `&mut impl Iterator`.
pub struct RcIter<I: ?Sized>(core::cell::RefCell<I>);

impl<T, I: Iterator<Item = T> + ?Sized> Iterator for &RcIter<I> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.0.borrow_mut().next()
    }
}

impl<I> RcIter<I> {
    /// Construct a new mutable iterator.
    pub const fn new(iter: I) -> Self {
        Self(core::cell::RefCell::new(iter))
    }
}

/// Global data that provides mutable access to input values.
pub trait HasInputs<'a, V> {
    /// Obtain the inputs from global data.
    fn inputs(&self) -> Inputs<'a, V>;
}

impl<'a, V> HasInputs<'a, V> for Inputs<'a, V> {
    fn inputs(&self) -> Inputs<'a, V> {
        self
    }
}
