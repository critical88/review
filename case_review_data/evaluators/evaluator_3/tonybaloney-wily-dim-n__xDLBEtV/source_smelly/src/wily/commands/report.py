"""
Report command.

The report command gives a table of metrics for a specified list of files.
Will compare the values between revisions and highlight changes in green/red.
"""

import pathlib
from collections.abc import Iterable
from pathlib import Path
from shutil import copytree
from string import Template

from rich import box
from rich.console import Console
from rich.table import Table
from rich.text import Text

from wily import MAX_MESSAGE_WIDTH, format_date, format_revision, logger
from wily.backend import WilyIndex
from wily.cache import list_archivers
from wily.config.types import WilyConfig
from wily.defaults import DEFAULT_TABLE_STYLE
from wily.helper import BOX_STYLES, print_table
from wily.helper.custom_enums import ReportFormat
from wily.lang import _
from wily.operators import ALL_METRICS, MetricType, resolve_metric_as_tuple

# Rich style names for metric changes
STYLE_RED = "red"
STYLE_GREEN = "green"
STYLE_YELLOW = "yellow"


def report(
    config: WilyConfig,
    path: str,
    metrics: Iterable[str] | None,
    n: int | None,
    output: Path,
    include_message: bool = False,
    format: ReportFormat = ReportFormat.CONSOLE,
    changes_only: bool = False,  # ignore in v2
    wrap: bool = False,
    table_style: str = DEFAULT_TABLE_STYLE,
) -> None:
    """
    Show metrics for a given file.

    :param config: The configuration
    :param path: The path to the file
    :param metrics: List of metrics to report on
    :param n: Number of items to list
    :param output: Output path
    :param include_message: Include revision messages
    :param format: Output format
    :param changes_only: Only report revisions where delta != 0 (ignored - parquet only stores changes)
    :param wrap: Wrap output
    :param table_style: Table box style
    """
    # =====================================================================
    # Stage 1 - bind the requested metrics to their display rules.
    # NOTE: the style mapping below owns the delta colours for the whole
    # command (console cells AND the HTML path), so it cannot be moved
    # under the console branch alone.
    # =====================================================================
    if metrics is None:
        resolved_metrics = ALL_METRICS
    else:
        metrics = sorted(set(metrics))
        resolved_metrics = [resolve_metric_as_tuple(metric_name) for metric_name in metrics]

    logger.debug("Running report command")

    column_specs = []
    for operator, metric in resolved_metrics:
        key = metric.name
        # Set the delta styles depending on the metric type
        if metric.measure == MetricType.AimHigh:
            increase_style = STYLE_GREEN
            decrease_style = STYLE_RED
        elif metric.measure == MetricType.AimLow:
            increase_style = STYLE_RED
            decrease_style = STYLE_GREEN
        elif metric.measure == MetricType.Informational:
            increase_style = STYLE_YELLOW
            decrease_style = STYLE_YELLOW
        else:
            increase_style = STYLE_YELLOW
            decrease_style = STYLE_YELLOW
        column_specs.append(
            {
                "key": key,
                "operator": operator.name,
                "increase_style": increase_style,
                "decrease_style": decrease_style,
                "title": metric.description,
                "type": metric.metric_type,
            }
        )

    # =====================================================================
    # Stage 2 - find every archiver with cached data.
    # =====================================================================
    archivers = list_archivers(config)

    if not archivers:
        logger.error("No wily cache found. Run 'wily build' first.")
        return

    # =====================================================================
    # Stage 3 - scan the index and build the display rows in one pass.
    # The delta book-keeping (`last` below) is shared per metric column,
    # which is why this loop cannot simply be cut in half.
    # =====================================================================
    report_rows: list[tuple[str | Text, ...]] = []

    for archiver in archivers:
        parquet_path = pathlib.Path(config.cache_path) / archiver / "metrics.parquet"
        if not parquet_path.exists():
            logger.debug("No parquet file for archiver %s", archiver)
            continue

        operator_names = [meta["operator"] for meta in column_specs]
        with WilyIndex(str(parquet_path), operator_names) as index:
            # Check if this is a granular query (e.g., "file.py:function_name")
            is_granular = ":" in path

            # Get all rows for this path
            # For granular paths (file.py:function), accept any path_type
            # For file paths, filter to file-level entries only
            if is_granular:
                rows = list(index[path])
            else:
                rows = [row for row in index[path] if row.get("path_type") == "file"]
            # Sort by date (oldest first) and limit to n
            rows = sorted(rows, key=lambda r: r.get("revision_date", 0))[-(n or len(rows)) :]

            last: dict = {}
            for row in rows:
                deltas = []
                vals: list[str | Text] = []

                for meta in column_specs:
                    try:
                        val = row.get(meta["key"])
                        if val is None:
                            k: str | Text = "N/A"
                            delta = 0
                        elif meta["type"] in (int, float):
                            # Ensure val is numeric
                            if not isinstance(val, (int, float)):
                                k = f"{val}"
                                delta = 0
                            else:
                                last_val = last.get(meta["key"])
                                if last_val is not None and isinstance(last_val, (int, float)):
                                    delta = val - last_val
                                else:
                                    delta = 0
                                last[meta["key"]] = val

                                # Build a Rich Text object with styled delta
                                cell = Text(f"{val:n} (")
                                if delta == 0:
                                    cell.append(str(delta))
                                elif delta < 0:
                                    cell.append(f"{delta:n}", style=meta["decrease_style"])
                                else:
                                    cell.append(f"+{delta:n}", style=meta["increase_style"])
                                cell.append(")")
                                k = cell
                        else:
                            k = f"{val}"
                            delta = 0
                    except (KeyError, TypeError) as e:
                        k = f"Not found {e}"
                        delta = 0
                    deltas.append(delta)
                    vals.append(k)

                # Build row data
                revision_key = row.get("revision", "")
                author = row.get("revision_author", "")
                date = row.get("revision_date", 0)
                message = row.get("revision_message", "")

                if include_message:
                    report_rows.append(
                        (
                            format_revision(revision_key),
                            (message or "")[:MAX_MESSAGE_WIDTH],
                            str(author or ""),
                            format_date(date),
                            *vals,
                        )
                    )
                else:
                    report_rows.append(
                        (
                            format_revision(revision_key),
                            str(author or ""),
                            format_date(date),
                            *vals,
                        )
                    )

    if not report_rows:
        logger.error("No data found for %s.", path)
        return

    # =====================================================================
    # Stage 4 - pick the header set for the chosen output format.
    # =====================================================================
    descriptions = [meta["title"] for meta in column_specs]
    if include_message:
        headers = (_("Revision"), _("Message"), _("Author"), _("Date"), *descriptions)
    else:
        headers = (_("Revision"), _("Author"), _("Date"), *descriptions)

    # =====================================================================
    # Stage 5 - publish. Console and HTML are both handled right here so
    # the row order (newest first) stays in one place.
    # =====================================================================
    if format == ReportFormat.HTML:
        # -- resolve the destination -------------------------------------
        if output.suffix == ".html":
            report_path = output.parents[0]
            report_output = output
        else:
            report_path = output
            report_output = output.joinpath("index.html")

        report_path.mkdir(exist_ok=True, parents=True)

        templates_dir = (Path(__file__).parents[1] / "templates").resolve()
        page_template = Template((templates_dir / "report_template.html").read_text())

        # Style to HTML class mapping
        style_to_html = {
            "green": "green-color",
            "red": "red-color",
            "yellow": "orange-color",
        }

        # -- build the <th> row --------------------------------------------
        table_headers = "".join([f"<th>{header}</th>" for header in headers])

        # -- build the <td> body, converting Rich Text spans inline -----
        rows_html = ""
        for line in report_rows[::-1]:
            rows_html += "<tr>"
            for element in line:
                if isinstance(element, Text):
                    # convert one Text cell (span conversion kept inline:
                    # class mapping and the span cursor walk together)
                    element_html = element.plain
                    if element._spans:  # noqa: SLF001
                        element_html = ""
                        plain_text = element.plain
                        span_cursor = 0
                        for start, end, span_style in element._spans:  # noqa: SLF001
                            # Add unstyled text before this span
                            if start > span_cursor:
                                element_html += plain_text[span_cursor:start]
                            # Add styled text
                            span_text = plain_text[start:end]
                            if span_style:
                                html_class = style_to_html.get(str(span_style), "")
                                if html_class:
                                    element_html += f"<span class='{html_class}'>{span_text}</span>"
                                else:
                                    element_html += span_text
                            else:
                                element_html += span_text
                            span_cursor = end
                        # Add any remaining text
                        if span_cursor < len(plain_text):
                            element_html += plain_text[span_cursor:]
                    rows_html += f"<td>{element_html}</td>"
                else:
                    rows_html += f"<td>{element}</td>"
            rows_html += "</tr>"

        rendered_report = page_template.safe_substitute(headers=table_headers, content=rows_html)

        with report_output.open("w", errors="xmlcharrefreplace") as output_f:
            output_f.write(rendered_report)

        try:
            copytree(str(templates_dir / "css"), str(report_path / "css"))
        except FileExistsError:
            pass

        logger.info("wily report was saved to %s", report_path)
    else:
        # -- console path: newest first, rendered straight to the table --
        console = Console()
        box_style = BOX_STYLES.get(table_style.upper(), box.ROUNDED)
        out_table = Table(show_header=True, header_style="bold", box=box_style)

        for header in list(headers):
            # fold/ignore per the `wrap` switch
            out_table.add_column(header, overflow="fold" if wrap else "ignore")

        for row in report_rows[::-1]:
            processed_row = [cell if isinstance(cell, Text) else str(cell) for cell in row]
            out_table.add_row(*processed_row)

        console.print(out_table)
