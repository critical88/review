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


class ParseResultProcessor:
    """Encapsulates post-parse normalization logic for parsed parameter values.

    This processor handles the conversion of sentinel values and validation
    of residual arguments after the option parser has finished. It acts as a
    strategy object that can be shared across different parsing contexts.

    .. versionadded:: 8.3
    """

    _sentinel = UNSET
    _replacement: t.Any = None

    def normalize_params(
        self,
        params: dict[str, t.Any],
    ) -> None:
        """Replace sentinel values in the parameter dict in-place.

        After initial parsing, parameter values may contain :data:`UNSET`
        sentinels. This normalizes them to ``None`` so downstream code
        never observes internal sentinel objects.
        """
        sentinel = self._sentinel
        replacement = self._replacement
        for key in params:
            if params[key] is sentinel:
                params[key] = replacement

    def check_residual_args(
        self,
        args: list[str],
        allow_extra: bool,
        resilient: bool,
    ) -> str | None:
        """Validate whether residual arguments are acceptable.

        Returns an error message if extra arguments are present and not
        allowed, or ``None`` if no error.
        """
        if args and not allow_extra and not resilient:
            return self._format_extra_args_message(args)
        return None

    @staticmethod
    def _format_extra_args_message(args: list[str]) -> str:
        from gettext import ngettext as _ngettext

        return _ngettext(
            "Got unexpected extra argument ({args})",
            "Got unexpected extra arguments ({args})",
            len(args),
        ).format(args=" ".join(map(str, args)))

    def apply_context_updates(
        self,
        ctx_args: list[str],
        ctx_opt_prefixes: set[str],
        remaining_args: list[str],
        parser_opt_prefixes: set[str],
    ) -> tuple[list[str], set[str]]:
        """Compute updated context state after parsing completes.

        Returns the new ``(args, opt_prefixes)`` tuple to be applied to
        the context, merging parser-level prefix information.
        """
        merged_prefixes = ctx_opt_prefixes | parser_opt_prefixes
        return list(remaining_args), merged_prefixes
