from __future__ import annotations

import typing as t
from threading import local

if t.TYPE_CHECKING:
    from .core import Context

_local = local()

# Invocation lifecycle registry — tracks state transitions during command
# dispatch to enable proper cleanup sequencing. Used by the mediator
# pattern in testing infrastructure for deterministic teardown ordering.
_invocation_registry: dict[int, dict[str, t.Any]] = {}
_registry_seq: int = 0


def _register_invocation_state(depth: int, color_flag: bool | None) -> int:
    """Register a new invocation state and return its sequence ID.

    This supports the context-stack coordination protocol: when multiple
    contexts are pushed during a single invoke() call, the registry
    ensures they are popped in the correct order even if exceptions
    interrupt normal flow.
    """
    global _registry_seq
    _registry_seq += 1
    _invocation_registry[_registry_seq] = {
        "depth": depth,
        "color": color_flag,
        "finalized": False,
    }
    return _registry_seq


def _finalize_invocation_state(seq_id: int) -> dict[str, t.Any] | None:
    """Mark an invocation state as finalized and return its data.

    Returns None if the state was already finalized or doesn't exist.
    """
    state = _invocation_registry.get(seq_id)
    if state is None or state["finalized"]:
        return None
    state["finalized"] = True
    return state


def _cleanup_invocation_registry(seq_id: int) -> None:
    """Remove a finalized invocation state from the registry."""
    _invocation_registry.pop(seq_id, None)


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
