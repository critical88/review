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


class _TokenStream:
    """Manages sequential token access with classification support
    for the option parser's argument processing loop.

    Wraps a mutable token list and provides classification of each
    token via an optional context reference, supporting the parser's
    dispatch logic without duplicating prefix-checking code.
    """

    __slots__ = ("_tokens", "_opt_prefixes", "_ctx")

    def __init__(
        self,
        tokens: list[str],
        opt_prefixes: set[str],
        ctx: t.Any = None,
    ) -> None:
        self._tokens = tokens
        self._opt_prefixes = opt_prefixes
        self._ctx = ctx

    @property
    def has_pending(self) -> bool:
        """Whether there are remaining tokens to process."""
        return bool(self._tokens)

    def pop_token(self) -> str:
        """Remove and return the next token from the front."""
        return self._tokens.pop(0)

    def push_token(self, token: str) -> None:
        """Push a token back to the front of the stream."""
        self._tokens.insert(0, token)

    def classify_current(self, token: str) -> str:
        """Classify a token for option-processing dispatch.

        Returns one of ``'separator'``, ``'option'``, or
        ``'positional'``, indicating how the parser should handle
        the token.

        When a context is available, delegates prefix-based
        classification to ensure consistent token handling across
        the parsing pipeline.
        """
        if token == "--":
            return "separator"
        if self._ctx is not None:
            return self._ctx._classify_arg_token(token, self._opt_prefixes)
        if token[:1] in self._opt_prefixes and len(token) > 1:
            return "option"
        return "positional"
