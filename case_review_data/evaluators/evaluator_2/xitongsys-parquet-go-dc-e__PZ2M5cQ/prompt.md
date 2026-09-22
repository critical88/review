# Refactoring request: give the per-page statistics record a single owner on the write path

**Repository:** xitongsys/parquet-go (Go)
**Area:** the column-file write path — table→page assembly and page serialization in the `layout` package, per-column buffering and row-group flush in the `writer` package.

## Maintainer observation

Every page this pipeline writes carries the same small record: the page's maximum
value, minimum value, and null count. Those three numbers are computed while a column
table is sliced into pages, serialized into the page header's `Statistics`, folded into
column chunk metadata when pages become a chunk, and exposed per page through the
`ColumnIndex`. Lately, whenever we touch anything around page statistics — adding a
statistic, reviewing the omit-statistics handling, following the parallel flush — we
end up editing the same handful of producer and consumer signatures in lockstep, and
each edit re-reads several call sites to keep the values paired correctly. The write
path does not have one place that owns this record. Instead the three members travel
together but ungrouped: slicing returns them as separate parallel results next to the
pages, page serialization receives them as loose individual values, chunk aggregation
consumes parallel per-page traces, and the writer buffers them in side collections the
writer must keep aligned with its page buffers by hand (through the parallel-flush merge
and into row-group assembly, where the dictionary-page position needs a fabricated
empty member).

## Diagnosis

The page's maximum, minimum, and null count are one cohesive data record — they are
produced together, interpreted together, and stored together in the file format. The
current write path handles that record as a recurring group of separated members across
four interfaces: the two table→page conversion paths (plain and dictionary-encoded)
publish the members separately; the three page-serialization forms (v1 header, v2
header, dictionary-encoded data page) take them as independent scalars; the two
pages→chunk folds consume them as index-aligned parallel slices; and the writer's
buffering keeps parallel per-column collections beside its page buffers, spanning the
flush merge and row-group assembly. No abstraction owns the grouping, and the pairing
between a page and its statistics is maintained by convention at each seam. The
consequences are concrete: a new per-page member widens every signature at once, the
per-column collections and the page buffers can drift in length, and the
dictionary-page exception has to be faked by hand wherever a dictionary chunk begins.

## Requested outcome

Redesign the handling of the page statistics record so the record is carried by one
well-designed grouping: introduce or restore one cohesive abstraction that owns the
three members, and use it consistently at each participation point of the write-path
flow above — production and serialization of plain data pages, of dictionary-encoded
data pages (the dictionary page itself keeps carrying no statistics), the v2-header
compatibility form, the pages→chunk aggregation for both encodings, and the writer's
buffering between object flush and row-group completion. Apply the same grouping on
both sides of the `layout` ↔ `writer` interface, and follow every participant on the
write path, including second-construction-path wiring, so no production path keeps
handling the members as separated values. Clean up rather than leave behind any
collection, parameter, or field the new owner supersedes; do not leave dead carriers in
place for compatibility.

## Behavior that must not change

- Written parquet files stay byte for byte compatible: page-header `Statistics` (both
  header forms), chunk metadata statistics, and per-page `ColumnIndex` null counts
  contain the same bytes as before; per-chunk null-count totals and
  omit-statistics-visible behavior are identical.
- The `omitstats` tag keeps suppressing statistics at exactly the same points (no
  min/max/null-count in page headers and chunk metadata for omitted columns).
- Dictionary pages continue to carry no statistics; only their data pages do; page
  splitting, sizes, encodings, page ordering, row-group sizing, and offsets are
  unchanged.
- The public API and the observable behavior of the writer and reader entry points,
  including the arrow writer, are unchanged; internal helper signatures may change,
  but no exported constructor call, option, or invocation shape may require changes
  from callers.
- The repository's full Go test suite passes with the same results as on the current
  revision; do not modify, weaken, or extend the tests to fit the refactor.
- Keep unrelated code, formatting, and files out of the change; the refactor stays
  within the write-path statistics flow.
