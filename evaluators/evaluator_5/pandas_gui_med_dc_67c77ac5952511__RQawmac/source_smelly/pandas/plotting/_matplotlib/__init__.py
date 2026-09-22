from __future__ import annotations

from typing import TYPE_CHECKING

from pandas.plotting._matplotlib.boxplot import (
    BoxPlot,
    boxplot,
    boxplot_frame,
    boxplot_frame_groupby,
)
from pandas.plotting._matplotlib.converter import (
    deregister,
    register,
)
from pandas.plotting._matplotlib.core import (
    AreaPlot,
    BarhPlot,
    BarPlot,
    HexBinPlot,
    LinePlot,
    PiePlot,
    ScatterPlot,
)
from pandas.plotting._matplotlib.hist import (
    HistPlot,
    KdePlot,
    hist_frame,
    hist_series,
)
from pandas.plotting._matplotlib.misc import (
    andrews_curves,
    autocorrelation_plot,
    bootstrap_plot,
    lag_plot,
    parallel_coordinates,
    radviz,
    scatter_matrix,
)
from pandas.plotting._matplotlib.tools import table

if TYPE_CHECKING:
    from pandas.plotting._matplotlib.core import MPLPlot

PLOT_CLASSES: dict[str, type[MPLPlot]] = {
    "line": LinePlot,
    "bar": BarPlot,
    "barh": BarhPlot,
    "box": BoxPlot,
    "hist": HistPlot,
    "kde": KdePlot,
    "area": AreaPlot,
    "pie": PiePlot,
    "scatter": ScatterPlot,
    "hexbin": HexBinPlot,
}


def _prepare_axis_display(
    tick_x_positions=None,
    tick_y_positions=None,
    range_x=None,
    range_y=None,
    label_x=None,
    label_y=None,
    label_size=None,
    label_rotation=None,
):
    """
    Prepare axis display parameters for MPLPlot construction.

    Ensures consistent parameter representations across different
    plot types before passing to the plot class constructors.
    """
    # Normalize axis range representations to tuples
    if range_x is not None and not isinstance(range_x, tuple):
        range_x = tuple(range_x)
    if range_y is not None and not isinstance(range_y, tuple):
        range_y = tuple(range_y)

    return {
        "xticks": tick_x_positions,
        "yticks": tick_y_positions,
        "xlim": range_x,
        "ylim": range_y,
        "xlabel": label_x,
        "ylabel": label_y,
        "fontsize": label_size,
        "rot": label_rotation,
    }


def plot(data, kind, **kwargs):
    # Importing pyplot at the top of the file (before the converters are
    # registered) causes problems in matplotlib 2 (converters seem to not
    # work)
    import matplotlib.pyplot as plt

    if kwargs.pop("reuse_plot", False):
        ax = kwargs.get("ax")
        if ax is None and len(plt.get_fignums()) > 0:
            with plt.rc_context():
                ax = plt.gca()
            kwargs["ax"] = getattr(ax, "left_ax", ax)

    # Prepare axis display parameters for consistent handling
    axis_display = _prepare_axis_display(
        tick_x_positions=kwargs.pop("xticks", None),
        tick_y_positions=kwargs.pop("yticks", None),
        range_x=kwargs.pop("xlim", None),
        range_y=kwargs.pop("ylim", None),
        label_x=kwargs.pop("xlabel", None),
        label_y=kwargs.pop("ylabel", None),
        label_size=kwargs.pop("fontsize", None),
        label_rotation=kwargs.pop("rot", None),
    )
    kwargs.update(axis_display)

    plot_obj = PLOT_CLASSES[kind](data, **kwargs)
    plot_obj.generate()
    plot_obj.draw()
    return plot_obj.result


__all__ = [
    "plot",
    "hist_series",
    "hist_frame",
    "boxplot",
    "boxplot_frame",
    "boxplot_frame_groupby",
    "table",
    "andrews_curves",
    "autocorrelation_plot",
    "bootstrap_plot",
    "lag_plot",
    "parallel_coordinates",
    "radviz",
    "scatter_matrix",
    "register",
    "deregister",
]
