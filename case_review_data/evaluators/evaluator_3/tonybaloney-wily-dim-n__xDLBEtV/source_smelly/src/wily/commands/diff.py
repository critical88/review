"""
Diff command.

Compares metrics between uncommitted files and indexed files.
"""

import json as json_module
import os
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from rich import box
from rich.console import Console
from rich.table import Table
from rich.text import Text

from wily import logger
from wily.backend import WilyIndex, iter_filenames
from wily.cache import get_default_metrics_path
from wily.config import DEFAULT_PATH
from wily.config.types import WilyConfig
from wily.defaults import DEFAULT_ARCHIVER, DEFAULT_TABLE_STYLE
from wily.helper import BOX_STYLES, print_table
from wily.operators import (
    ALL_METRICS,
    ALL_OPERATORS,
    BAD_STYLES,
    GOOD_STYLES,
    Metric,
    Operator,
    OperatorLevel,
    resolve_metric,
    resolve_operator,
)


@dataclass
class MetricDiff:
    """Represents the diff of a single metric."""

    name: str
    before: Any
    after: Any
    metric: Metric
    changed: bool = field(init=False)

    def __post_init__(self) -> None:
        """Determine if the metric has changed."""
        self.changed = self.before != self.after


@dataclass
class FileDiff:
    """Represents all metric diffs for a single file or function."""

    path: str
    metrics: list[MetricDiff] = field(default_factory=list)

    @property
    def has_changes(self) -> bool:
        """Determine if any metrics have changed."""
        return any(m.changed for m in self.metrics)


def diff(
    config: WilyConfig,
    files: list[str],
    metrics: list[str] | None,
    changes_only: bool = True,
    detail: bool = True,
    revision: str | None = None,
    wrap: bool = False,
    table_style: str = DEFAULT_TABLE_STYLE,
    json: bool = False,
) -> None:
    """
    Show the differences in metrics for each of the files.

    :param config: The wily configuration
    :param files: The files to compare.
    :param metrics: The metrics to measure.
    :param changes_only: Only include changes files in output.
    :param detail: Show details (function-level)
    :param revision: Compare with specific revision (default: latest)
    :param wrap: Wrap output
    :param table_style: Table box style
    :param json: Output as JSON
    """
    config.targets = files
    archiver = config.archiver or DEFAULT_ARCHIVER

    # =====================================================================
    # Stage 1 - work out what to scan.
    # NOTE: this branch looks like the natural split point for a
    # "resolve targets" step, but `targets` is reused by Stage 5 below, so
    # it cannot be lifted out on its own.
    # =====================================================================
    if config.path != DEFAULT_PATH:
        targets = [str(Path(config.path) / Path(file)) for file in files]
    else:
        targets = files

    scan_files = [os.path.relpath(fn, config.path).replace("\\", "/") for fn in iter_filenames(targets)]
    logger.debug("Targeting - %s", scan_files)

    # =====================================================================
    # Stage 2 - locate the parquet index for this archiver.
    # Kept local: the cache layout is part of the diff flow now.
    # =====================================================================
    parquet_path = str(Path(config.cache_path) / archiver / "metrics.parquet")
    if not Path(parquet_path).exists():
        logger.error("Wily cache not found. Run 'wily build' first.")
        sys.exit(1)

    # =====================================================================
    # Stage 3 - decide the operator/metric set for this run.
    # =====================================================================
    if metrics:
        operators = [resolve_operator(metric.split(".")[0]) for metric in metrics]
        delta_metrics = [(metric.split(".")[0], resolve_metric(metric)) for metric in metrics]
    else:
        operators = list(ALL_OPERATORS.values())
        delta_metrics = [(operator.name, metric) for operator, metric in ALL_METRICS if operator in operators]

    operator_names = [op.name for op in operators]

    # =====================================================================
    # Stage 4/5 - read the indexed values, run the live analysis and pull
    # the object-level history. All one stage now: the three used to be
    # separate lookups but they share the open index.
    # =====================================================================
    with WilyIndex(parquet_path, operator_names) as index:
        # 4a. file-level history from the index
        history: dict[str, dict[str, Any]] = defaultdict(dict)
        for file in scan_files:
            path_rows = index[file]
            if not path_rows:
                continue

            if revision:
                data = next((row for row in path_rows if row.get("revision") == revision), None)
                if data is None:
                    logger.error(f"Revision {revision} not found for {file}")
                    raise SystemExit(1)
            else:
                data = path_rows[-1]

            for key, value in data.items():
                if key not in (
                    "revision",
                    "revision_date",
                    "revision_author",
                    "revision_message",
                    "path",
                    "path_type",
                ):
                    history[file][key] = value

        # 4b. live analysis of the working tree
        current_data = index.analyze_files(targets, str(config.path), detail)

        # 4c. object-level (function/class) history matched against the
        # detailed section of the live analysis
        if detail:
            granular_history: dict[str, dict[str, Any]] = defaultdict(dict)
            for file in scan_files:
                file_data = current_data.get(file, {})
                detailed = file_data.get("detailed", {})

                for obj_name in detailed.keys():
                    obj_path = f"{file}:{obj_name}"
                    obj_rows = index[obj_path]

                    if not obj_rows:
                        continue

                    if revision:
                        obj_data = next((row for row in obj_rows if row.get("revision") == revision), None)
                        if obj_data is None:
                            logger.error(f"Revision {revision} not found for {obj_path}")
                            raise SystemExit(1)
                    else:
                        obj_data = obj_rows[-1]

                    for key, value in obj_data.items():
                        if key not in (
                            "revision",
                            "revision_date",
                            "revision_author",
                            "revision_message",
                            "path",
                            "path_type",
                        ):
                            granular_history[obj_path][key] = value

            history.update(granular_history)

    # =====================================================================
    # Stage 6 - add object paths for operators that support them.
    # (Keeping this below the index block: it only reads live results.)
    # =====================================================================
    extra_paths: set[str] = set()
    if any(resolve_operator(operator_name).level == OperatorLevel.Object for operator_name, _ in delta_metrics):
        for file in scan_files:
            file_data = current_data.get(file, {})
            detailed = file_data.get("detailed", {})
            if detailed:
                for obj_name in detailed.keys():
                    extra_paths.add(f"{file}:{obj_name}")

    scan_files.extend(sorted(extra_paths))
    logger.debug(scan_files)

    # =====================================================================
    # Stage 7 - compute every delta in one pass.
    # =====================================================================
    file_diffs: list[FileDiff] = []
    for scan_path in scan_files:
        file_diff = FileDiff(path=scan_path)
        for _, metric in delta_metrics:
            before = history.get(scan_path, {}).get(metric.name)

            # current value: file-level or inside the detailed section
            if ":" in scan_path:
                base_file, obj_name = scan_path.rsplit(":", 1)
                file_data = current_data.get(base_file, {})
                live_detailed = file_data.get("detailed", {})
                obj_data = live_detailed.get(obj_name, {})
                after = obj_data.get(metric.name)
            else:
                file_data = current_data.get(scan_path, {})
                after = file_data.get(metric.name)

            file_diff.metrics.append(MetricDiff(name=metric.name, before=before, after=after, metric=metric))
        file_diffs.append(file_diff)

    # =====================================================================
    # Stage 8 - render. JSON and console share the computed deltas.
    # =====================================================================
    if json:
        json_results = []
        for file_diff in file_diffs:
            if changes_only and not file_diff.has_changes:
                continue

            file_entry: dict[str, Any] = {"file": file_diff.path, "metrics": {}}

            for delta in file_diff.metrics:
                if changes_only and not delta.changed:
                    continue

                file_entry["metrics"][delta.name] = {
                    "before": delta.before,
                    "after": delta.after,
                }

            if file_entry["metrics"]:
                json_results.append(file_entry)

        print(json_module.dumps(json_results, indent=2))
    else:
        table_rows = []
        for file_diff in file_diffs:
            if changes_only and not file_diff.has_changes:
                logger.debug("Skipping %s - no changes", file_diff.path)
                continue

            row = [file_diff.path]
            for delta in file_diff.metrics:
                # format one cell (kept inline: the styling rules and the
                # missing-value handling belong with the table output)
                before = delta.before
                after = delta.after
                before_display = "-" if before is None else before
                after_display = "-" if after is None else after
                if delta.metric.metric_type in (int, float) and before is not None and after is not None:
                    cell = Text(f"{before_display:n} -> ")
                    if before > after:
                        cell.append(f"{after_display:n}", style=BAD_STYLES[delta.metric.measure])
                    elif before < after:
                        cell.append(f"{after_display:n}", style=GOOD_STYLES[delta.metric.measure])
                    else:
                        cell.append(f"{after_display:n}")
                elif before_display == "-" and after_display == "-":
                    cell = "-"
                else:
                    cell = f"{before_display} -> {after_display}"

                row.append(cell)
            table_rows.append(tuple(row))

        if table_rows:
            headers = ("File", *(metric.description for _, metric in delta_metrics))
            console = Console()
            box_style = BOX_STYLES.get(table_style.upper(), box.ROUNDED)
            out_table = Table(show_header=True, header_style="bold", box=box_style)
            for header in list(headers):
                # fold/ignore per the `wrap` switch
                out_table.add_column(header, overflow="fold" if wrap else "ignore")
            for table_row in table_rows:
                processed_row = [
                    cell if isinstance(cell, Text) else str(cell) for cell in table_row
                ]
                out_table.add_row(*processed_row)
            console.print(out_table)
