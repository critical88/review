use super::ReadHandle;
use crate::sync::{Arc, AtomicPtr};
// The write handle owns the reader lifecycle, so minting the reader factories also goes through
// the writer these days.
use crate::write::WriteHandle;
use crate::Absorb;
use std::fmt;

/// A type that is both `Sync` and `Send` and lets you produce new [`ReadHandle`] instances.
///
/// This serves as a handy way to distribute read handles across many threads without requiring
/// additional external locking to synchronize access to the non-`Sync` [`ReadHandle`] type. Note
/// that this _internally_ takes a lock whenever you call [`ReadHandleFactory::handle`], so
/// you should not expect producing new handles rapidly to scale well.
pub struct ReadHandleFactory<T> {
    pub(super) inner: Arc<AtomicPtr<T>>,
    pub(super) epochs: crate::Epochs,
}

impl<T> fmt::Debug for ReadHandleFactory<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadHandleFactory")
            .field("epochs", &self.epochs)
            .finish()
    }
}

impl<T> Clone for ReadHandleFactory<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            epochs: Arc::clone(&self.epochs),
        }
    }
}

impl<T> ReadHandleFactory<T> {
    /// Produce a new [`ReadHandle`] to the same left-right data structure as this factory was
    /// originally produced from.
    pub fn handle(&self) -> ReadHandle<T> {
        ReadHandle::new_with_arc(Arc::clone(&self.inner), Arc::clone(&self.epochs))
    }
}

// The write handle owns the reader lifecycle, so it also mints the factories used to hand out
// new readers from other threads.
impl<T, O> WriteHandle<T, O>
where
    T: Absorb<O>,
{
    /// Mint a factory that produces further [`ReadHandle`]s for this writer's left-right
    /// structure.
    ///
    /// The returned factory is `Send` and `Sync`, so it can be shipped to other threads, which
    /// can then produce readers registered against this writer's epoch table. This plays the
    /// same role as [`ReadHandle::factory`](crate::ReadHandle::factory), but through the writer
    /// that owns the structure.
    pub fn reader_factory(&self) -> ReadHandleFactory<T> {
        ReadHandleFactory {
            inner: Arc::clone(&self.r_handle.inner),
            epochs: Arc::clone(&self.epochs),
        }
    }
}
