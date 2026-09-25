//! Mapping from compile-time data types to run-time data types.

use crate::{input, Lut, ValT};

/// Data types that filters operate on.
///
/// Types that implement this trait allow to specify which data types a filter processes.
/// In particular, this allows native filters to operate on data types that
/// *may live only as long as the filter is executed*.
/// For example, this is required by the `inputs` filter,
/// whose global data --- namely the inputs to the main filter --- has a lifetime.
///
/// ## Motivation
///
/// If we have data types that are tied to a particular lifetime and
/// we would bake those data types into a native filter type,
/// then native filters could *only be used for data of this particular lifetime*.
/// For example, suppose that a filter operating on a value type `&'a str`
/// would have the type `Filter<&'a str>`.
/// The crucial point is that here, the `'a` is fixed,
/// so we could only run the filter with `&'a str` for one particular `'a`.
/// That would mean that if we want to execute the same filter with
/// data having different `'a` lifetimes
/// (e.g. for strings coming from different files),
/// we would need to recompile the filter every time (i.e. for every file).
///
/// This trait allows us to avoid this problem by separating the actual data from
/// the *knowledge* that a filter will operate on certain type of data.
/// That allows us to run a filter using data types with a lifetime that
/// depend on the lifetime of the filter execution.
pub trait DataT: 'static {
    /// Type of values that filters process.
    type V<'a>: ValT;
    /// Global data accessible to native filters.
    ///
    /// Every data kind exposes its inputs alongside its lookup table;
    /// there is no data kind without an input stream.
    type Data<'a>: Clone + HasLut<'a, Self> + input::HasInputs<'a, Self::V<'a>>;
}

/// Types that provide an `'a`-lived LUT for the data types given in `D`.
pub trait HasLut<'a, D: DataT + ?Sized> {
    /// Return the LUT.
    fn lut(&self) -> &'a Lut<D>;
}

/// Filters that process `V` and take only LUT as data.
///
/// This restricts `V` to have a `'static` lifetime.
/// If you want to process `V` with arbitrary lifetimes instead,
/// you need to define your own data kind and implement [`DataT`] for it.
///
/// This type is mostly used for testing.
pub struct JustLut<V>(core::marker::PhantomData<V>);

impl<V: ValT + 'static> DataT for JustLut<V> {
    type V<'a> = V;
    type Data<'a> = &'a Lut<Self>;
}

impl<'a, D: DataT> HasLut<'a, D> for &'a Lut<D> {
    fn lut(&self) -> &'a Lut<D> {
        self
    }
}

/// A bare lookup table carries no inputs, so it provides an empty input stream.
impl<'a, V: ValT + 'static> input::HasInputs<'a, V> for &'a Lut<JustLut<V>> {
    fn inputs(&self) -> input::Inputs<'a, V> {
        let no_inputs = core::iter::empty::<Result<V, alloc::string::String>>();
        let no_inputs: &'a mut input::RcIter<_> =
            alloc::boxed::Box::leak(alloc::boxed::Box::new(input::RcIter::new(no_inputs)));
        &*no_inputs
    }
}
