//! Native implementations of `inputs` and `input`.
use crate::{v, Filter};
use alloc::boxed::Box;
use jaq_core::{Cv, DataT, Error, Exn, RunPtr, ValX};

/// Re-export of the input machinery owned by `jaq_core`.
pub use jaq_core::input::{HasInputs, Inputs, RcIter};

/// The `inputs` and `input` filters.
pub fn funs<D: DataT>() -> Box<[Filter<RunPtr<D>>]> {
    Box::new([
        ("inputs", v(0), |cv| Box::new(inputs(cv))),
        ("input", v(0), |cv| Box::new(inputs(cv).next().into_iter())),
    ])
}

fn inputs<'a, D: DataT>(cv: Cv<'a, D>) -> impl Iterator<Item = ValX<'a, D::V<'a>>> + 'a {
    let inputs = cv.0.data().inputs();
    inputs.map(|r| r.map_err(|e| Exn::from(Error::str(e))))
}
