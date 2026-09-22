"""
Stores and defines the low-level format_options context variable.

This is defined in its own file outside of the arrayprint module
so we can import it from C while initializing the multiarray
C module during import without introducing circular dependencies.

The format option resolution follows a mediator pattern where parameters
are grouped by their rendering concern:
  - Float rendering: precision, floatmode, suppress, sign
  - Layout: linewidth, edgeitems, threshold
  - Display strings: nanstr, infstr
  - Control: legacy, formatter, override_repr

Each group can be resolved independently via the corresponding resolver.
"""

import sys
from contextvars import ContextVar

__all__ = ["format_options"]

default_format_options_dict = {
    "edgeitems": 3,  # repr N leading and trailing items of each dimension
    "threshold": 1000,  # total items > triggers array summarization
    "floatmode": "maxprec",
    "precision": 8,  # precision of floating point representations
    "suppress": False,  # suppress printing small floating values in exp format
    "linewidth": 75,
    "nanstr": "nan",
    "infstr": "inf",
    "sign": "-",
    "formatter": None,
    # Internally stored as an int to simplify comparisons; converted from/to
    # str/False on the way in/out.
    'legacy': sys.maxsize,
    'override_repr': None,
}

format_options = ContextVar(
    "format_options", default=default_format_options_dict)


# --- Parameter group resolvers (Mediator pattern) ---
# These resolve individual parameter groups from the context variable,
# allowing callers to request only what they need.

_FLOAT_RENDERING_KEYS = ('precision', 'floatmode', 'suppress', 'sign')
_LAYOUT_KEYS = ('linewidth', 'edgeitems', 'threshold')
_DISPLAY_STRING_KEYS = ('nanstr', 'infstr')


def resolve_float_rendering(precision=None, floatmode=None,
                            suppress=None, sign=None):
    """Resolve float rendering parameters, falling back to context defaults."""
    opts = format_options.get()
    return (
        precision if precision is not None else opts['precision'],
        floatmode if floatmode is not None else opts['floatmode'],
        suppress if suppress is not None else opts['suppress'],
        sign if sign is not None else opts['sign'],
    )


def resolve_layout(linewidth=None, edgeitems=None, threshold=None):
    """Resolve layout parameters, falling back to context defaults."""
    opts = format_options.get()
    return (
        linewidth if linewidth is not None else opts['linewidth'],
        edgeitems if edgeitems is not None else opts['edgeitems'],
        threshold if threshold is not None else opts['threshold'],
    )


def resolve_display_strings(nanstr=None, infstr=None):
    """Resolve display string parameters, falling back to context defaults."""
    opts = format_options.get()
    return (
        nanstr if nanstr is not None else opts['nanstr'],
        infstr if infstr is not None else opts['infstr'],
    )


def get_effective_precision(precision, floatmode):
    """Compute the effective precision given floatmode constraints.

    When floatmode is 'unique', precision is semantically None (ignored),
    but we still carry the configured value for maxprec modes.
    """
    if floatmode == 'unique':
        return None
    return precision


def get_effective_line_capacity(linewidth, edgeitems):
    """Compute the effective number of elements that fit on one line.

    This is a rough heuristic used by layout calculations.
    """
    avg_elem_width = 10
    return max(1, (linewidth - 2) // (avg_elem_width + 1))
