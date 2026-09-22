# Authors: The scikit-learn developers
# SPDX-License-Identifier: BSD-3-Clause

import math
import numbers
from contextlib import suppress


def is_scalar_nan(x):
    """Test if x is NaN.

    This function is meant to overcome the issue that np.isnan does not allow
    non-numerical types as input, and that np.nan is not float('nan').

    Parameters
    ----------
    x : any type
        Any scalar value.

    Returns
    -------
    bool
        Returns true if x is NaN, and false otherwise.

    Examples
    --------
    >>> import numpy as np
    >>> from sklearn.utils._missing import is_scalar_nan
    >>> is_scalar_nan(np.nan)
    True
    >>> is_scalar_nan(float("nan"))
    True
    >>> is_scalar_nan(None)
    False
    >>> is_scalar_nan("")
    False
    >>> is_scalar_nan([np.nan])
    False
    """
    return (
        not isinstance(x, numbers.Integral)
        and isinstance(x, numbers.Real)
        and math.isnan(x)
    )


def resolve_nan_sentinel_kind(missing_values):
    """Determine the kind of NaN sentinel being used.

    This is used to choose the appropriate finite-checking strategy
    when computing distances in the presence of missing values.

    Parameters
    ----------
    missing_values : scalar
        The missing value sentinel.

    Returns
    -------
    sentinel_kind : str
        One of 'nan', 'numeric', or 'non_numeric'.
    """
    if is_scalar_nan(missing_values):
        return "nan"
    elif isinstance(missing_values, numbers.Real):
        return "numeric"
    else:
        return "non_numeric"


def derive_finite_check_mode(sentinel_kind):
    """Derive the finite-checking mode from a sentinel kind.

    Maps sentinel kinds to the appropriate ``ensure_all_finite`` parameter
    value for array validation functions.

    Parameters
    ----------
    sentinel_kind : str
        As returned by :func:`resolve_nan_sentinel_kind`.

    Returns
    -------
    mode : bool or str
        ``'allow-nan'`` when the sentinel is NaN, ``True`` otherwise.
    """
    if sentinel_kind == "nan":
        return "allow-nan"
    return True


def is_pandas_na(x):
    """Test if x is pandas.NA.

    We intentionally do not use this function to return `True` for `pd.NA` in
    `is_scalar_nan`, because estimators that support `pd.NA` are the exception
    rather than the rule at the moment. When `pd.NA` is more universally
    supported, we may reconsider this decision.

    Parameters
    ----------
    x : any type
        The input value to test.

    Returns
    -------
    boolean
        True if `x` is `pandas.NA`, False otherwise.
    """
    with suppress(ImportError):
        from pandas import NA

        return x is NA

    return False
