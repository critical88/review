//! Values that can be processed by jaq.
//!
//! To process your own value type with jaq,
//! you need to implement the [`ValT`] trait.

use crate::box_iter::BoxIter;
use crate::path::Opt;
use core::fmt::Display;
use core::ops::{Add, Div, Mul, Neg, Rem, Sub};

// Makes `f64::from_str` accessible as intra-doc link.
#[cfg(doc)]
use core::str::FromStr;

/// Value or eRror.
pub type ValR<T, V = T> = Result<T, crate::Error<V>>;
/// Stream of values and eRrors.
pub type ValRs<'a, T, V = T> = BoxIter<'a, ValR<T, V>>;
/// Value or eXception.
///
/// Use [`unwrap_valr`] to convert to [`ValR`].
pub type ValX<'a, T, V = T> = Result<T, crate::Exn<'a, V>>;
/// Stream of values and eXceptions.
pub type ValXs<'a, T, V = T> = BoxIter<'a, ValX<'a, T, V>>;

/// Convert a value exception [`ValX`] into a value result [`ValR`].
///
/// This should always succeed when called on results of a main filter.
/// For any other filter, this may not succeed, i.e. panic.
///
/// If you are writing a native filter, e.g. `f(f1; ...; fn)`,
/// do not use this function on outputs of `fi`!
///
/// This function will exit the current process if
/// the value exception results from a call to the filter `halt`.
/// If the `std` feature is disabled, this function panics instead.
/// In a future jaq 3.0, this function should only be provided if
/// the `std` feature is enabled.
pub fn unwrap_valr<T, V>(v: ValX<T, V>) -> ValR<T, V> {
    #[cfg(feature = "std")]
    let exit = |exit_code| std::process::exit(exit_code);
    #[cfg(not(feature = "std"))]
    let exit = |exit_code| panic!("halt({})", exit_code);

    let halt = |e: crate::Exn<_>| exit(e.get_halt().ok().unwrap());
    v.map_err(|e| e.get_err().unwrap_or_else(halt))
}

/// Range of options, used for iteration operations.
pub type Range<V> = core::ops::Range<Option<V>>;

/// Values that can be processed by jaq.
///
/// Implement this trait if you want jaq to process your own type of values.
///
/// The trait carries the *complete* value contract of the jaq ecosystem:
/// besides the structural operations used by the core term language, it
/// also covers the value capabilities required by the standard library,
/// such as sequence conversion, numerical views, and string/byte access.
/// A type that implements this single trait can therefore be used with
/// the entire standard library of filters, without any second
/// implementation effort.
pub trait ValT:
    Clone
    + Display
    + From<bool>
    + From<isize>
    + From<alloc::string::String>
    + From<Range<Self>>
    + FromIterator<Self>
    + PartialEq
    + PartialOrd
    + Ord
    + From<f64>
    + Add<Output = ValR<Self>>
    + Sub<Output = ValR<Self>>
    + Mul<Output = ValR<Self>>
    + Div<Output = ValR<Self>>
    + Rem<Output = ValR<Self>>
    + Neg<Output = ValR<Self>>
{
    /// Create a number from a string.
    ///
    /// The number should adhere to the format accepted by [`f64::from_str`].
    fn from_num(n: &str) -> ValR<Self>;

    /// Create an associative map (or object) from a sequence of key-value pairs.
    ///
    /// This is used when creating values with the syntax `{k: v}`.
    fn from_map<I: IntoIterator<Item = (Self, Self)>>(iter: I) -> ValR<Self>;

    /// Yield the key-value pairs of a value.
    ///
    /// This is used to collect the paths of `.[]`.
    /// It should yield any `key` for which `value | .[key]` is defined,
    /// as well as its output.
    fn key_values(self) -> BoxIter<'static, ValR<(Self, Self), Self>>;

    /// Yield the children of a value.
    ///
    /// This is used by `.[]`.
    fn values(self) -> alloc::boxed::Box<dyn Iterator<Item = ValR<Self>>>;

    /// Yield the child of a value at the given index.
    ///
    /// This is used by `.[k]`.
    ///
    /// If `v.index(k)` is `Ok(_)`, then it is contained in `v.values()`.
    fn index(self, index: &Self) -> ValR<Self>;

    /// Yield a slice of the value with the given range.
    ///
    /// This is used by `.[s:e]`, `.[s:]`, and `.[:e]`.
    fn range(self, range: Range<&Self>) -> ValR<Self>;

    /// Map a function over the children of the value.
    ///
    /// This is used by
    /// - `.[]  |= f` (`opt` = [`Opt::Essential`]) and
    /// - `.[]? |= f` (`opt` = [`Opt::Optional`]).
    ///
    /// If the children of the value are undefined, then:
    ///
    /// - If `opt` is [`Opt::Essential`], return an error.
    /// - If `opt` is [`Opt::Optional`] , return the input value.
    fn map_values<'a, I: Iterator<Item = ValX<'a, Self>>>(
        self,
        opt: Opt,
        f: impl Fn(Self) -> I,
    ) -> ValX<'a, Self>;

    /// Map a function over the child of the value at the given index.
    ///
    /// This is used by `.[k] |= f`.
    ///
    /// See [`Self::map_values`] for the behaviour of `opt`.
    fn map_index<'a, I: Iterator<Item = ValX<'a, Self>>>(
        self,
        index: &Self,
        opt: Opt,
        f: impl Fn(Self) -> I,
    ) -> ValX<'a, Self>;

    /// Map a function over the slice of the value with the given range.
    ///
    /// This is used by `.[s:e] |= f`, `.[s:] |= f`, and `.[:e] |= f`.
    ///
    /// See [`Self::map_values`] for the behaviour of `opt`.
    fn map_range<'a, I: Iterator<Item = ValX<'a, Self>>>(
        self,
        range: Range<&Self>,
        opt: Opt,
        f: impl Fn(Self) -> I,
    ) -> ValX<'a, Self>;

    /// Return a boolean representation of the value.
    ///
    /// This is used by `if v then ...`.
    fn as_bool(&self) -> bool;

    /// Convert value into a string value.
    ///
    /// This is used by `"\(v)"`.
    fn into_string(self) -> Self;

    /// Convert an array into a sequence.
    ///
    /// This returns the original value as `Err` if it is not an array.
    fn into_seq<S: FromIterator<Self>>(self) -> Result<S, Self>;

    /// True if the value is integer.
    fn is_int(&self) -> bool;

    /// Use the value as machine-sized integer.
    ///
    /// If this function returns `Some(_)`, then [`Self::is_int`] must return true.
    /// However, the other direction must not necessarily be the case, because
    /// there may be integer values that are not representable by `isize`.
    fn as_isize(&self) -> Option<isize>;

    /// Use the value as floating-point number.
    ///
    /// This succeeds for all numeric values,
    /// rounding too large/small ones to +/- Infinity.
    fn as_f64(&self) -> Option<f64>;

    /// True if the value is interpreted as UTF-8 string.
    fn is_utf8_str(&self) -> bool;

    /// If the value is a string (whatever its interpretation), return its bytes.
    fn as_bytes(&self) -> Option<&[u8]>;

    /// If the value is interpreted as UTF-8 string, return its bytes.
    fn as_utf8_bytes(&self) -> Option<&[u8]> {
        self.is_utf8_str().then(|| self.as_bytes()).flatten()
    }

    /// If the value is a string (whatever its interpretation), return its bytes, else fail.
    fn try_as_bytes(&self) -> Result<&[u8], crate::Error<Self>> {
        self.as_bytes()
            .ok_or_else(|| crate::Error::typ(self.clone(), "string"))
    }

    /// If the value is interpreted as UTF-8 string, return its bytes, else fail.
    fn try_as_utf8_bytes(&self) -> Result<&[u8], crate::Error<Self>> {
        self.as_utf8_bytes()
            .ok_or_else(|| crate::Error::typ(self.clone(), "string"))
    }

    /// If the value is a string and `sub` points to a slice of the string,
    /// shorten the string to `sub`, else panic.
    fn as_sub_str(&self, sub: &[u8]) -> Self;

    /// Interpret bytes as UTF-8 string value.
    fn from_utf8_bytes(b: impl AsRef<[u8]> + Send + 'static) -> Self;
}
