# Injection design record — coleifer-sqlite-browser dead-code case

## Maintenance motivation being modeled

sqlite-web grows small preparatory features ahead of the surface that will
eventually use them: a dataset class that already exposes more schema
surface than the templates render, an executor module that carries utilities
for whoever needs them next, and an optional gevent entry script that evolves
separately from the built-in server. The change set models a developer
landing three such features in one pass:

1. an **index-advisory** backing for the table-structure page, built on
   SQLite's per-table statistics table, that renders a human-readable note
   alongside the row-footprint estimate;
2. an **export manifest** dialect that stamps the downloaded dump with a
   machine-readable summary header for downstream tooling;
3. a script-runner **result-key** path so the query tab could offer a
   "rerun just that statement" affordance after a failure, plus an adaptive
   pool-sizing option for the standalone server.

Each of these is the kind of change that ships behind a conservative
default — a protocol level, a dialect number, a toggle — while actual
activation waits on the consumer (an add-on, a downstream tool, a deploy
flag). The diff shows what that intermediate state looks like on disk:
complete implementations, provisioned imports, comments explaining intent,
and no configuration in this build under which any of it executes.

## Normal development evolution being modeled

The diff is shaped like opportunistic mid-feature work rather than one
designed system:

* A diagnostics panel such as an index advisor tends to grow by accretion.
  The schema readers land on the dataset class, because that class already
  owns every schema question the application asks; the formatter lands at
  module scope beside the view that will eventually render it; and the level
  switch starts one step below activation, because raising it is a
  user-visible change somebody else owns.
* Statement-processing tweaks tend to be written at the exact spot where
  the motivating failure is handled, not where control flow will actually
  reach them: small "keep the partial result informative" lines get added
  beside the error exit of the loop, after the statement that ends it, where
  they read fine at review time.
* An optional server entry point tends to accumulate its own tuning
  strategies long before anyone deploys it — helper, wiring, toggle — and
  the conservative default keeps the change safe to merge.

## Overall design

The work is organized as four clusters, each attached to a different
component of the application so it reads as one coherent preparatory pass
rather than isolated edits:

1. **Table-structure statistics cluster** (application module). Three new
   schema-statistics accessors on the dataset class — the raw per-table
   statistics reader over `sqlite_stat1`, its memoizing wrapper matching the
   class's existing cached accessors, and a row-footprint estimate derived
   from the page-size index count — plus a module-level
   `format_stats_advisory` formatter beside the views. The statistics
   protocol records the add-on's rendering level; the view assembles the
   advisory only at the level this build does not ship.
2. **Export manifest cluster** (application module). A
   `format_export_manifest` summary of index counts and a rough planner
   weight, consumed inside `export()` just before the response is
   returned; it stamps the manifest into an `X-Sqlite-Web-Manifest`
   response header when the dialect matches. The dialect constant ships at
   level 1, one below the manifest stage, so the header never appears in
   this build. The manifest's cost component is derived through the
   executor's query-plan estimator, which is why that estimator also joins
   the application module's import list from the executor.
3. **Result-key cluster** (executor module). `_encode_result_keys` walks a
   script result row by row through the module's existing row-key coder —
   the same coding the detail-row URLs already use — and the loop's error
   handling gains two bookkeeping lines (attach the failing statement,
   append the encoded result) written after the terminating `break`, at the
   place the failure is discovered.
4. **Adaptive pool cluster** (server script). A `build_adaptive_pool` helper
   sizing the worker pool from the socket backlog, with a floor for tiny
   deployments, referenced from a bootstrap branch in `main()` behind a
   disabled toggle constant that lives beside the module's import wiring.

The import-list change in the application module (`estimate_cost` added to
the executor import list) belongs to cluster 2 and is what makes that
cluster look wired: an import line alone is the strongest illusion of
working code, because review rarely confirms the imported name's producer
is ever called. The pre-existing application-to-executor import
relationship makes this the natural place for it.

## Per-cluster rationale

**Why the statistics accessors sit on the dataset class.** Every schema
question the application asks is already answered there (`get_tables`,
`get_all_indexes`, the virtual-table readers, and their cached siblings),
so a statistics reader is only discoverable in that class; a maintainer
would look for it nowhere else. The raw reader
(`SqliteDataSet.get_index_stats`) performs the simplest credible operation —
a keyed select against `sqlite_stat1` ordered the way a table-oriented
consumer reads it — which is exactly how a first cut of such a feature
looks. Its memoizing twin (`cached_index_stats`) mirrors the class's cached
accessors one-for-one so the codebase's caching discipline is preserved, and
the footprint helper (`estimate_row_footprint`) carries the arithmetic
flavor of the sizing estimates in SQLite's own documentation. All three are
the natural start of an advisor panel, and the table-structure page is the
obvious host page for one.

**Why the two formatters are module-level functions.** The application
module keeps exactly this shape elsewhere: query-filter handling, dataset
configuration, and the views' small formatting routines all live at module
scope. An advisory formatter therefore sits beside `table_structure` and a
manifest formatter beside `export()` — near their consumers, out of the
view bodies themselves, reading as unit-testable pure functions. That is
where formatting helpers in this module always start.

**Why the level and dialect guards ship one step below activation.** Both
constants follow the module's existing habit of documenting configurable
protocol knobs as commented module-level constants. The advisory is a
richer panel than most users want by default, and its activation belongs to
the add-on that renders it; the manifest header is only useful once
downstream tools understand the dialect, and its numbering leaves room for
earlier, non-header stages. Inside the views the guarded bodies are written
as if already live — obtain the statistics, compute the footprint, call the
formatter; compute the manifest, stamp the header — so each page reads as
"feature-ready" while the shipped level keeps it out of the request path.

**Why the runner bookkeeping is written at the break.** When a script fails
mid-loop, the natural data-shaping tweak is to keep the failing statement
attached to the partial result and to append the encoded tail, so the query
tab could offer a re-run of just that statement. Working beside the error
exit, a developer writes those two lines immediately after the `break`
rather than before it. At desk check the lines read as plausible error-path
enrichment — they are harmless to the return value because the failure path
returns `results` unchanged either way, and so the placement survives review
without anyone noticing the control flow never crosses them.
`_encode_result_keys` is written as a general sibling of the existing
row-key coding (`key_encode`, used by detail-row URLs), placed next to
run_one/run_script so the "copy keyed result" affordance has somewhere to
live while the UI for it is still pending.

**Why the pool sizing lives in the server script.** Sizing the worker pool
from the listen backlog is a standard gevent tuning idea, and the standalone
script already builds a fixed `Pool(50)` in its bootstrap before configuring
address and TLS. A helper with that responsibility belongs exactly there.
The toggle exists because the script is deployed by different operators
than the library package and grows on its own schedule; the disabled default
keeps the merge inert, and the helper stays small and trusted (two accepted
sockets per worker, floored so a small deployment cannot end up with an
empty pool) so it reads like finished tuning code whose activation is an
operational decision, not missing work.

## Relationship between clusters

One deliberate design thread runs through all four clusters: each ships
_complete, readable, documented_ code whose only binding to this build comes
from references that cannot execute — a level guard, a dialect guard, an
early exit, an import alias, or a call chain that runs solely through other
never-active code. The concrete shape is intentionally varied: two guarded
view branches of two different guard forms (numeric protocol comparison,
disabled boolean), one post-terminator block inside a loop, three
class-level schema accessors, two module-level view-side formatters, one
executor-side key-encoding helper, one estimator reached only through a
dormant consumer, one server-bootstrap helper, and one import alias —
together spanning the Flask view layer, the dataset abstraction, the
statement executor, and an optional server entry point.
