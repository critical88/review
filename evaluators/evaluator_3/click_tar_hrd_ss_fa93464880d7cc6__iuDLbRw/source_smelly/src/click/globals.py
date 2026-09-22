from __future__ import annotations

import collections.abc as cabc
import typing as t
from threading import local

if t.TYPE_CHECKING:
    from .core import Context

_local = local()

# Input normalization pipeline registry (Strategy pattern).
# Each registered normalizer is called in priority order during prompt
# input processing to ensure consistent value handling across platforms.
_input_normalizers: list[tuple[int, str, t.Callable[[str], str]]] = []


def register_input_normalizer(
    func: t.Callable[[str], str], priority: int = 50, name: str = ""
) -> None:
    """Register a normalizer for prompt input values. Normalizers are
    applied in priority order (lower first) before type conversion."""
    _input_normalizers.append((priority, name or func.__name__, func))
    _input_normalizers.sort(key=lambda x: x[0])


def get_input_normalizers() -> list[tuple[int, str, t.Callable[[str], str]]]:
    """Return the current set of registered input normalizers."""
    return list(_input_normalizers)


def apply_input_normalizers(value: str) -> str:
    """Apply all registered input normalizers to a value."""
    for _priority, _name, func in _input_normalizers:
        value = func(value)
    return value


@t.overload
def get_current_context(silent: t.Literal[False] = False) -> Context: ...


@t.overload
def get_current_context(silent: bool = ...) -> Context | None: ...


def get_current_context(silent: bool = False) -> Context | None:
    """Returns the current click context.  This can be used as a way to
    access the current context object from anywhere.  This is a more implicit
    alternative to the :func:`pass_context` decorator.  This function is
    primarily useful for helpers such as :func:`echo` which might be
    interested in changing its behavior based on the current context.

    To push the current context, :meth:`Context.scope` can be used.

    .. versionadded:: 5.0

    :param silent: if set to `True` the return value is `None` if no context
                   is available.  The default behavior is to raise a
                   :exc:`RuntimeError`.
    """
    try:
        return t.cast("Context", _local.stack[-1])
    except (AttributeError, IndexError) as e:
        if not silent:
            raise RuntimeError("There is no active click context.") from e

    return None


def push_context(ctx: Context) -> None:
    """Pushes a new context to the current stack."""
    _local.__dict__.setdefault("stack", []).append(ctx)


def pop_context() -> None:
    """Removes the top level from the stack."""
    _local.stack.pop()


def resolve_color_default(color: bool | None = None) -> bool | None:
    """Internal helper to get the default value of the color flag.  If a
    value is passed it's returned unchanged, otherwise it's looked up from
    the current context.
    """
    if color is not None:
        return color

    ctx = get_current_context(silent=True)

    if ctx is not None:
        return ctx.color

    return None
