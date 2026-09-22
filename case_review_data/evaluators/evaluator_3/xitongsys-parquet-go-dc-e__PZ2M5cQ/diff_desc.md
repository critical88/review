# Injection design record — page-statistics collation on the parquet write path

Repository: xitongsys/parquet-go (commit `03e949366fbc9a92fad4c3a233fffee729863ba2`).
Scope of this record: the `smell.diff` change set only. It documents the maintenance
motivation modeled for the change, the development story it is meant to sit inside, the
overall design of the resulting code, and a per-cluster explanation of every materially
touched location, including the production role each site serves. It is an auditable
design rationale for the diff, not a grading, verification, or follow-up-work record.

## Maintenance motivation

parquet-go computes a small, settled record for every column page while slicing column
tables into pages: the page's maximum value, minimum value, and null count. Those
numbers are serialized into the page header's thrift `Statistics`, folded into column
chunk metadata, and surfaced per page through the parquet `ColumnIndex` null counts.
The production path for that record is long: pages are carved out by parallel
marshallers, parked in per-column writer buffers for the entire row group, and only
turned into chunks and page headers when the row group completes.

In the pinned starting state, the in-flight record lived on the `Page` struct itself
(`MaxVal interface{}`, `MinVal interface{}`, `NullCount *int64`). That had two costs that
come up naturally in maintenance on this path. First, the buffered `Page` objects
outlive their usefulness: after compression the raw bytes and the serialized header are
already on the page, so the three extra reference fields only add resident footprint for
every buffered page of every column while a row group accumulates — which for wide
schemas and large row-group sizes is the dominant live object population in the writer.
Second, the record's members were mutable struct fields populated by one function and
read by another two or three, which made the v2-header compatibility path awkward: it
depended on fields being set by a caller it never sees.

The change moves the in-flight record off the buffered page object and into the seams
that actually move it: the table→page slicing step publishes the per-page numbers at
the moment it builds the page, the page-serialization step receives them as explicit
inputs, the pages→chunk aggregation consumes them as per-page traces, and the writer —
the party that needs them until row-group completion — keeps its own per-column
statistics buffers next to the page buffers it already owns. Nothing in the written file
format changes; pages are byte-for-byte what they were, because serialization still
writes exactly the same header `Statistics`.

## Normal development evolution being modeled

This models the ordinary, incremental way such a change slips into a repository:

1. A small semantics-preserving step: the two data-page serialization helpers
   (`DataPageCompress`, `DataPageV2Compress`) are aligned so both take the page's
   statistics as explicit inputs instead of one of them relying on struct mutation by
   an upstream call. Each edit is locally reviewable in isolation.
2. A memory/thread-through step: the record leaves the buffered `Page` type entirely;
   `TableToDataPages` — the producer that used to populate the fields — returns the
   per-page aggregates directly to its caller instead.
3. A uniformity step: `TableToDictDataPages` grows the same return shape, because the
   writer calls both slicing functions in sibling branches and keeping their calling
   conventions identical is the path of least resistance.
4. Follow-through edits: the chunk assembly and the writer buffers continue to compile
   and work, so the remaining changes — parallel per-page traces in
   `PagesToChunk`/`PagesToDictChunk`, per-column statistics collections on
   `ParquetWriter`, and the two writer constructors that build its state — are exactly
   the code the previous steps force into alignment.

Each step is what a maintainer would plausibly land to keep the build green; the
aggregate result, arrived at one seam at a time, is that the record's three members now
recur as a group at every interface they cross, with no single owner of the grouping.

## Overall design of the resulting code

- `layout`: both table→page conversion functions return the pages plus three parallel
  per-page aggregates (`[]interface{}` for maxima and minima, `[]*int64` for null
  counts, one entry per produced page, `nil` entries where the omit-statistics tag
  suppresses the numbers). The three page-serialization helpers take the page's
  maximum, minimum, and null count as explicit parameters and write them into the v1
  header, the v2 header (the compatibility path), and the dictionary-encoded data page
  header respectively. The two pages→chunk functions take the per-page aggregates as
  parallel slices and fold them into chunk metadata.
- `writer`: `ParquetWriter` keeps its existing per-column page buffer and, alongside
  it, three parallel per-column collections for the buffered statistics. The parallel
  flush keeps per-worker maps of all four and merges them name-by-name; row-group
  assembly passes each column's collections to the chunk functions; when a column
  chunk begins with its dictionary page, callers prepend a `nil` member for that
  position, since dictionary pages carry no statistics. Both writer constructors
  allocate the new collections.
- Behavioral invariants preserved everywhere: page headers, chunk metadata statistics,
  and `ColumnIndex` null counts are written from the same numbers as before (the
  omit-statistics branch keeps suppressing min/max/null-count and keeps computing only
  the element size); page splitting, encodings, row-group sizing, ordering and offsets
  are untouched; the null-count evaluation quirks of the two slicing functions are
  carried over verbatim; the parallel dictionary branch keeps its mutex scope and
  defer-release pattern unchanged.

The follow-on maintenance notes the author would attach to this change: the record's
members are now repeated as a group in every signature they cross, every new per-page
statistic widens all of those signatures at once, the per-column collection lengths and
the page-buffer length are kept equal by convention instead of by construction, and
the dictionary-page position needs a fabricated `nil` member at exactly one call site.
Those observations describe the debt this design carries, and are recorded here as the
rationale for why the change set looks the way it does.

## Per-cluster explanations

### Cluster 1 — plain data-page production and header serialization (`layout/page.go`)

`TableToDataPages` slices a column table into plain-encoded data pages. While doing so
it evaluates the page maximum, minimum, and null count (respecting the omit-statistics
tag, which still keeps the element-size computation the only remaining work). It now
appends each page's numbers to three local aggregates and passes them straight into the
serialization call, returning them alongside the pages. *Why this shape:* the values
are available exactly inside this loop and are consumed immediately by the page's
compression call; returning them keeps the caller in control of what stays alive.
*Production role:* sole plain-path producer of per-page statistics, and the only place
that knows the page boundaries the statistics belong to.

`Page.DataPageCompress` and `Page.DataPageV2Compress` serialize a data page's raw bytes
and its header. Both now receive the page's maximum, minimum, and null count as
explicit inputs and write them into the header `Statistics` (`DataPageHeader` and the
`DataPageHeaderV2` compatibility form respectively). *Why this shape:* explicit inputs
let both header forms share one calling convention instead of silently depending on
struct fields being pre-populated; the v2 variant is a compatibility path with no
in-tree callers and is treated as a first-class sibling so the two forms cannot drift.
*Production role:* the boundary where in-flight numbers become file-format bytes.
The `Page` struct itself loses `MaxVal`/`MinVal`/`NullCount`, leaving it a pure
container of schema, path, table slice, and serialized bytes.

### Cluster 2 — dictionary-encoded page production (`layout/dictpage.go`)

`TableToDictDataPages` performs the same slicing for dictionary-encoded columns and now
returns the same shape. The upstream quirk in its null-count evaluation is preserved
exactly as written (it evaluates nullness differently from the plain-path slicer);
uniformity with the plain path is the reason its return list grew, since the writer
calls the two slicers in sibling branches. `Page.DictDataPageCompress` gains the same
explicit inputs for the data pages that reference the dictionary. *Production role:*
the dictionary slice of the same statistics record — the pages that carry the column's
values are still data pages and still get statistics, while the dictionary page proper
is untouched and keeps carrying none.

### Cluster 3 — chunk aggregation (`layout/chunk.go`)

`PagesToChunk` and `PagesToDictChunk` fold per-page numbers into column chunk metadata
(max, min, accumulated null count, under the same omit-statistics conditions as
before). They now consume the per-page aggregates as parallel slices instead of
visiting each page object again, skipping the leading dictionary-page position the same
way they always skipped it in the page list. *Why this shape:* the writer already
holds the aggregates it received from the slicers, so passing them on is cheaper than
re-deriving them, and the fold logic itself is unchanged. *Production role:* the page
level of the `ColumnChunk` metadata that readers use for predicate pushdown; also the
place where the dictionary-page convention (no statistics for that position) is
honored on the consumption side.

### Cluster 4 — writer buffering and row-group flush (`writer/writer.go`)

The writer owns everything between object arrival and row-group completion: marshal
buffers, the parallel object flush, and the pages→chunk→footer stage. A row group can
span several flushes, so the writer must retain per-page statistics as long as it
retains the pages. `ParquetWriter` gains three per-column collections mirroring the
existing page-buffer map; `NewParquetWriter` allocates them; the object flush keeps
per-worker maps for all four collections, merges them name-by-name after the parallel
marshal completes, and appends each worker's slices in order; row-group assembly
passes every column's collections into the chunk functions, prepending the `nil`
member for a dictionary-page position when that column is dictionary-encoded; the
collections are reset together with the page buffer after a row group completes.
The parallel dictionary branch keeps its existing lock scope and deferred release,
and per-worker slicing results are captured inside that critical section as before.
*Why this shape:* the writer is the only participant that has to hold the record
across page production and chunk consumption, so the change set lands its new state
here; the merge and append order keeps production order (and therefore the
page-to-statistics pairing) intact. *Production role:* the buffering and sequencing
stage of the write path, where per-page statistics acquire their real lifetime.

### Cluster 5 — the second writer construction path (`writer/arrow.go`)

`NewArrowWriter` constructs the arrow-schema writer. It builds the base `ParquetWriter`
state directly rather than calling the shared constructor, which is why it already
initializes the page-buffer map and dictionary map by hand; the new per-column
statistics collections are added to that setup for the same reason. *Why this shape:*
this is the one other place in the package wiring up writer state from scratch, so any
new state slot must appear here too. *Production role:* independent writer entry point
for arrow ingestion; reachable on every arrow write, invisible to the layout packages.
