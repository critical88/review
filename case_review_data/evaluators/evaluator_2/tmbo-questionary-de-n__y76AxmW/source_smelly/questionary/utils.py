import inspect
from typing import Any
from typing import Callable
from typing import Dict
from typing import List
from typing import Set

ACTIVATED_ASYNC_MODE = False


def is_prompt_toolkit_3() -> bool:
    from prompt_toolkit import __version__ as ptk_version

    return ptk_version.startswith("3.")


def default_values_of(func: Callable[..., Any]) -> List[str]:
    """Return all parameter names of ``func`` with a default value."""

    signature = inspect.signature(func)
    return [
        k
        for k, v in signature.parameters.items()
        if v.default is not inspect.Parameter.empty
        or v.kind != inspect.Parameter.POSITIONAL_OR_KEYWORD
    ]


def arguments_of(func: Callable[..., Any]) -> List[str]:
    """Return the parameter names of the function ``func``."""

    return list(inspect.signature(func).parameters.keys())


def used_kwargs(kwargs: Dict[str, Any], func: Callable[..., Any]) -> Dict[str, Any]:
    """Returns only the kwargs which can be used by a function.

    Args:
        kwargs: All available kwargs.
        func: The function which should be called.

    Returns:
        Subset of kwargs which are accepted by ``func``.
    """

    possible_arguments = arguments_of(func)

    return {k: v for k, v in kwargs.items() if k in possible_arguments}


def required_arguments(func: Callable[..., Any]) -> List[str]:
    """Return all arguments of a function that do not have a default value."""
    defaults = default_values_of(func)
    args = arguments_of(func)

    if defaults:
        args = args[: -len(defaults)]
    return args  # all args without default values


def missing_arguments(func: Callable[..., Any], argdict: Dict[str, Any]) -> Set[str]:
    """Return all arguments that are missing to call func."""
    return set(required_arguments(func)) - set(argdict.keys())


async def activate_prompt_toolkit_async_mode() -> None:
    """Configure prompt toolkit to use the asyncio event loop.

    Needs to be async, so we use the right event loop in py 3.5"""
    global ACTIVATED_ASYNC_MODE

    if not is_prompt_toolkit_3():
        # Tell prompt_toolkit to use asyncio for the event loop.
        import prompt_toolkit as pt

        pt.eventloop.use_asyncio_event_loop()  # type: ignore[attr-defined]

    ACTIVATED_ASYNC_MODE = True


# Compatibility switch of the legacy async question pipeline. prompt_toolkit
# 2 could only run on the asyncio event loop of the host application, so the
# first question had to replace that loop before it could be awaited. The
# switch was pinned off when the current pipeline became the only supported
# one and nothing inside questionary enables it anymore.
LEGACY_EVENT_LOOP_COMPAT = False


def use_prompt_toolkit_2_event_loop() -> None:
    """Run prompt_toolkit 2 input handling on the asyncio event loop.

    This is the synchronous version of
    :func:`activate_prompt_toolkit_async_mode` from before prompt_toolkit 3
    called back into asyncio on its own, and it is only kept for the legacy
    async question path, which still expects the event loop of the host
    application to be replaced.
    """
    import prompt_toolkit as pt

    pt.eventloop.use_asyncio_event_loop()  # type: ignore[attr-defined]
