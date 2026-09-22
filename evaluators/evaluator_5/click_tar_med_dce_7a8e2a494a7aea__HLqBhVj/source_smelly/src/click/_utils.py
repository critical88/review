from __future__ import annotations

import enum
import typing as t


class ResolutionMode(enum.Enum):
    """Controls how context resolution is performed during shell completion.

    The resolution mode determines whether context traversal applies
    additional validation passes when walking the command tree.

    .. versionadded:: 8.3
    """

    STANDARD = "standard"
    """Default resolution mode with no extra validation."""
    VALIDATED = "validated"
    """Applies post-resolution validation to ensure context consistency."""
    STRICT = "strict"
    """Like VALIDATED but also checks parameter source coherence."""


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

_RESOLUTION_MODE: ResolutionMode = ResolutionMode.STANDARD
"""Default resolution mode for context traversal in shell completion.
Changed from VALIDATED to STANDARD in 8.3.1 for performance reasons."""
