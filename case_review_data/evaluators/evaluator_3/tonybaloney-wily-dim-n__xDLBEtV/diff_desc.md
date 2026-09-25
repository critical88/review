# Injection design record — bulk-inlining collapse of the `diff` and `report` command pipelines

This record documents the maintenance scenario the change models, the
evolution it inverts, the overall design decisions behind the rewrite, and
an auditable per-location rationale for every material edit. It describes
the resulting code as an engineering event: what was collapsed, why each
piece sits where it sits, and what was deliberately kept or varied.

## 1. Maintenance scenario and motivation

The change models a well-known species of regression commit: a maintainer,
mid-refactor of the two heaviest CLI commands, decides the fine-grained
helper decomposition introduced in the 2.x rework costs more hopping than it
saves ("I jump five files to trace one run"), and pastes each stage of the
pipeline directly into the entry point so the whole command reads top-down.
Behavior is identical, the commands work, so the collapse survives review —
and only the next maintainer discovers that the stage functions, the shared
renderer calls, and the honest variable names are gone, replaced by stage
banners that carry the original decomposition's shape but none of its
structure.

The scenario was chosen for its realism relative to this repository: wily's
command modules (`src/wily/commands/*.py`) are naturally pipeline-shaped
(resolve targets → open index → compute → render), the renderers are
genuinely shared (`wily.helper.print_table` serves four commands), and the
dead code left behind (unused imports) is exactly what a hasty inlining
commit leaves. Nothing in the change is undoable or exotic; every fragment
reads like code a hurried contributor writes.

## 2. Evolution modeled

The collapse inverts two distinct layers of the module's history:

1. **Shared-utility layer (long-lived).** Since before the 2.x line, table
   rendering went through `wily.helper.print_table` (with `get_box_style`
   and `BOX_STYLES`), and the cache layout was resolved centrally by
   `wily.cache.get_default_metrics_path`. The erasure here is deep: it
   replaces *call sites on shared, still-live utilities* with adapted local
   copies, so the shared implementation and the new copies now coexist as
   rivals rather than one moving to replace the other.
2. **Module-decomposition layer (recent).** The 2.x parquet rework had split
   each command into module-level stage functions — `diff.py` into ten
   helpers (resolution, index reads, delta computation, formatting) and
   `report.py` into a console/HTML presentation split whose HTML leg carried
   its own nested converter. The change reverts that decomposition wholesale
   while preserving every observable behavior.

## 3. Overall design

- **Two production pipelines rewritten, not one.** `diff()` and `report()`
  are the two command entry points whose bodies can absorb an entire
  multi-stage pipeline and remain plausible. The two sites were collapsed
  with different internal shapes so that re-deriving the extraction requires
  genuine per-site analysis (see §5).
- **Depth, not just breadth.** Each collapse flattens a multi-level chain,
  not a set of sibling helpers: `diff` absorbed its stage functions *and*
  the renderer chain those stages called (`_format_table_output` →
  `print_table` → `get_box_style`) *and* the path helper
  (`get_default_metrics_path`); `report` absorbed its presentation helper,
  that helper's *nested* converter function (`text_to_html`, defined inside
  `_render_html_report`), and the same shared renderer chain.
- **Local renames as authored prose.** Absorbed bodies were re-bound to
  renamed locals rather than pasted verbatim (`indexed_data`→`history`,
  `resolved_metrics`→`delta_metrics`, `diffs`→`file_diffs`,
  `metric_diff`→`delta` in `diff.py`; `metric_metas`→`column_specs`,
  `data`→`report_rows`, `report_template`→`page_template`,
  `table_content`→`rows_html`, `last_end`→`span_cursor` in `report.py`).
  This mirrors how a human inlines (they rewrite names as they go) and
  forces structural rather than textual re-identification during any
  future extraction.
- **False boundaries.** The stage banners do not merely label — two of them
  embed plausible-sounding arguments against extracting the very region a
  maintainer would naturally want to lift out first (§4). Misleading as
  they are, both arguments are *technically half-true*: the noted data is
  indeed reused later, which is exactly the kind of note that freezes
  future contributors in place.
- **Collapse debris.** `from wily.helper import print_table` is left
  unused in both modules, and `from wily.cache import
  get_default_metrics_path` unused in `diff.py` — dead imports are the
  fingerprint of a copy-without-delete refactor and a natural cleanup
  obligation.
- **Behavior frozen.** Exit codes (`sys.exit(1)` / `SystemExit(1)`
  placement), error and debug log lines, the `--json` schema with
  `indent=2`, table headers/cell styling, newest-first ordering in both
  console and HTML, granular `file.py:function` handling, and the HTML
  span-class mapping were all preserved to the letter; the change is
  purely structural.

## 4. Per-location record

### Location A — `src/wily/commands/diff.py` (the `diff` command)

`diff()` is the pipeline for `wily diff`: compare uncommitted files against
indexed values by opening the per-archiver parquet index once, reading
per-file and per-object history, running a fresh analysis of the working
tree, and emitting either JSON or a styled table. Its production role makes
it the narrowest full-stack path in the package: config/target resolution,
index lifecycle, live re-analysis through the index, object-path
bookkeeping, two output formats.

What changed, stage by stage (stage banners mark the absorbed regions):

| Banner | Absorbed units | Production role of the region |
| --- | --- | --- |
| Stage 1 | `_resolve_files_and_targets` | `--path` target resolution and unix-style scan-list normalization |
| Stage 2 | inline `str(Path(config.cache_path) / archiver / "metrics.parquet")` replacing `get_default_metrics_path(config, archiver)` | cache-layout resolution |
| Stage 3 | `_resolve_metrics_and_operators` | operator/metric selection from the CLI `--metrics` or the full set |
| Stage 4a/4c | `_load_indexed_metrics`, `_load_detailed_metrics` | file-level and object-level history reads incl. the revision-lookup failure |
| Stage 4b | the previous direct call body | live analysis (`index.analyze_files`) inside the `WilyIndex` context |
| Stages 5–7 | `_collect_detailed_paths` + delta loop + merged `_get_current_metric_value` / `_compute_file_diff` | object-path completion and per-file/per-metric before-after deltas |
| Stage 8 | `_format_json_output`; `_format_table_output` + `_format_table_cell` + pasted `print_table`/`get_box_style` renderer | output rendering, both formats |

Design choices specific to this location:

- **The misleading note.** Stage 1 carries:
  `NOTE: this branch looks like the natural split point for a "resolve
  targets" step, but 'targets' is reused by Stage 5 below, so it cannot be
  lifted out on its own.` The claim is half-true — `targets` really is
  reused by the live-analysis step — but of course passing values between
  functions is what parameters are for; the note is written in the voice of
  a maintainer defending the collapse. It acts as a false extraction
  boundary exactly where extraction is easiest.
- **Cell formatting fused into the row loop.** `_format_table_cell` was not
  inlined as a separate commented region but folded into the per-row loop
  (the `# format one cell (kept inline: the styling rules ...)` fragment), so
  the render region has mixed granularity rather than tidy one-to-one
  blocks.
- **Uneven absorption.** The `MetricDiff`/`FileDiff` dataclasses and their
  methods were deliberately left as class-level definitions: they are the
  module's data model and are referenced by the pasted fragments. A
  collapse that also dissolved its own data types would stop reading like
  an authored refactor.
- **Dead imports kept** (`print_table`, `get_default_metrics_path`), plus
  the banner inventory described above.

### Location B — `src/wily/commands/report.py` (the `report` command)

`report()` is the pipeline for `wily report`: bind metrics and their
delta-styling rules, walk every archiver that has cached data, sort and
limit revisions, compute per-column deltas against the previous revision,
and render to console (newest first) or to the HTML report template. Its
production role differs from `diff` in every structural way: no live
re-analysis, an archiver loop, per-column delta bookkeeping, and a second
output medium (HTML) with its own file/template/css handling.

What changed:

- The body absorbs the HTML leg wholesale: destination resolution
  (`index.html` vs `output.html`), template read, the style→HTML-class
  mapping, the `<th>` header row build, the row loop over `report_rows[::-1]`,
  the character-encoding-safe write, the css `copytree`, and the final log
  line — everything that lived in `_render_html_report`.
- Depth-3 flattening: that helper's *nested* function — the Rich-`Text`
  span walker that emits `<span class='...'>` fragments — was un-nested and
  re-inlined as a span-cursor walk inside the row loop (`span_cursor` in
  place of the nested function's `last_end`, `plain_text`/`element_html` in
  place of `plain`/`result`), with the original names of the styled/unstyled
  branches preserved as comments.
- The console leg duplicates the same pasted renderer chain as `diff`
  (`Console` construction, `BOX_STYLES` lookup with `box.ROUNDED` fallback,
  `Table(show_header=True, header_style="bold", box=...)`, per-header
  `add_column` with the `wrap`-driven `fold`/`ignore` overflow switch, and
  the `Text`-preserving row conversion over `report_rows[::-1]`), i.e. the
  implementation of `wily.helper.print_table`, whose import remains in the
  header, unused.
- **The misleading note.** The Stage 3 banner carries: "the delta
  book-keeping (`last` below) is shared per metric column, which is why
  this loop cannot simply be cut in half." Also half-true — `last` is
  genuinely per-column state threaded across rows — and also irrelevant to
  whether the loops can be lifted; it reads as an instruction not to touch
  the biggest region in the file.
- Local renames applied as in §3, plus the `report_path`/`report_output`
  pair preserved from the absorbed helper to keep the write path honest.
- `from wily.helper import print_table` left unused.

Why this location pairs with `diff.py`: it is the only other command whose
entry point sits at the top of a multi-level chain (entry → presentation
helper → nested converter, and entry → shared renderer), the only command
with two output media, and the largest function in the package at rest
(89 statements before the change). Re-deriving its extraction requires
decisions `diff` never forces (where the HTML split belongs, how to keep
the span conversion with its `style_to_html` mapping, what to do with the
delta-state thread) — the two sites do not share a template.

## 5. Deliberate structural variation across the two sites

| Dimension | `diff.py` | `report.py` |
| --- | --- | --- |
| Output media fused into one body | JSON + console table | console table + HTML page |
| Renderer copy | table tail (rows computed forward) | table tail over `report_rows[::-1]` (reversed) |
| Nested function absorbed | none (no nested defs existed) | `text_to_html` (nested helper helper) |
| Cache path handling | re-derived by hand (shared helper existed) | pre-existing inline construction kept as-is |
| Banner style | numbered stages 1–8, one anti-extraction note on Stage 1 | grouped stages with the `last`-reuse note in Stage 3 |
| Dead imports left | two (`print_table`, `get_default_metrics_path`) | one (`print_table`) |
| Data-model boundary kept | `MetricDiff`/`FileDiff` dataclasses | `STYLE_*` constants (kept as the tying seam between console and HTML colors) |
| Loop direction of the render input | forward (`table_rows`) | reversed (`report_rows[::-1]`) |

The variation is deliberate: two same-shaped collapses would make the
extraction a mechanical text exercise, and would read as manufactured to
any reviewer of the change.

## 6. What was deliberately left intact

- `wily/helper/__init__.py::print_table`, `get_box_style`, and
  `BOX_STYLES`: untouched and still the live renderer for `wily rank`,
  `wily index`, and `wily list-metrics`. Deleting them would have turned the
  pasted copies into the only implementation and pre-empted the divergence
  problem the collapse is meant to create.
- `wily/cache.py::get_default_metrics_path` and friends: untouched; the
  command-side hand-derivation is the only shadowed path.
- `src/wily/__main__.py`: lazy per-command imports unchanged
  (`from wily.commands.diff import diff`, `from wily.commands.report
  import report`), as are both command signatures — the CLI surface is
  identical.
- Templates, css, i18n strings (`wily.lang._`), the archivers, the index
  internals, and every log/error message.
- `MetricDiff`/`FileDiff` and the `STYLE_*` constants (§5): kept as the
  remaining type and seam surfaces.

## 7. Locations considered and not changed (audit trail)

- `src/wily/commands/graph.py::graph` — already the package's largest
  function (88 statements, complexity 37) with no helper layer of its own
  to absorb; a collapse there would only stretch an existing monolith.
- `src/wily/commands/build.py::build` — its pipeline interacts with the
  revision-analysis seam by calling out to symbols that outside code swaps
  from under the module (`wily.commands.build.analyze_revision_with_index`
  is replaced from outside during crash-path simulation), so its helper
  boundary is externally pinned; the module stays as-is.
- `src/wily/commands/rank.py`, `src/wily/commands/index.py`,
  `src/wily/commands/list_metrics.py` — single-prominence command bodies
  well below the two chosen sites' mass, and healthy consumers of the
  shared renderer; collapsing them would add noise without adding depth.
- `wily/cache.py`, `wily/config/*`, `wily/archivers/*`, `wily/operators.py`
  — thin shared API surfaces consumed across the package; changes there
  would alter cross-module contracts rather than internal command
  structure.

## 8. Behavioral contract the change preserves

The collapse is purely structural. Preserved exactly: the parquet-missing
error path (`Wily cache not found. Run 'wily build' first.` → exit 1), the
revision-lookup failure (`Revision ... not found for ...` → `SystemExit(1)`
inside the index context), the report-side cache/data errors, the `--json`
payload schema with `indent=2`, metric-key exclusion lists, the
`[-(n or len(rows)) :]` limit, `data[::-1]` ordering in both output paths,
per-column delta bookkeeping, granular path compatibility, the
`errors="xmlcharrefreplace"` write, the css `copytree` behavior, the
green/red/yellow → HTML class mapping, `include_message` column handling,
translated headers, and the full CLI option surface of both commands
(including `changes_only` and `wrap`/`table_style` behavior).
