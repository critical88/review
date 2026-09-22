from __future__ import annotations

import enum
import typing as t


class Sentinel(enum.Enum):
    """Enum used to define sentinel values.

    .. seealso::

        `PEP 661 - Sentinel Values <https://peps.python.org/pep-0661/>`_.
    """

    UNSET = object()
    FLAG_NEEDS_VALUE = object()

    def __repr__(self) -> str:
        return f"{self.__class__.__name__}.{self.name}"


UNSET = Sentinel.UNSET
"""Sentinel used to indicate that a value is not set."""

FLAG_NEEDS_VALUE = Sentinel.FLAG_NEEDS_VALUE
"""Sentinel used to indicate an option was passed as a flag without a
value but is not a flag option.

``Option.consume_value`` uses this to prompt or use the ``flag_value``.
"""

T_UNSET = t.Literal[UNSET]  # type: ignore[valid-type]
"""Type hint for the :data:`UNSET` sentinel value."""

T_FLAG_NEEDS_VALUE = t.Literal[FLAG_NEEDS_VALUE]  # type: ignore[valid-type]
"""Type hint for the :data:`FLAG_NEEDS_VALUE` sentinel value."""


def _resolve_option_storage(multiple: bool, count: bool) -> str:
    """Determine the base storage mode for an option parameter.

    Returns the storage mode string used by the parser to decide
    how to handle option values. This is separate from whether
    the option uses a constant value.
    """
    if multiple:
        return "append"
    elif count:
        return "count"
    return "store"


def _derive_action_name(storage_mode: str, use_const: bool) -> str:
    """Derive the full action name from the base storage mode and
    the const flag. When ``use_const`` is True, the action name
    will have ``_const`` appended (e.g. ``store`` becomes
    ``store_const``).
    """
    if use_const:
        return f"{storage_mode}_const"
    return storage_mode
