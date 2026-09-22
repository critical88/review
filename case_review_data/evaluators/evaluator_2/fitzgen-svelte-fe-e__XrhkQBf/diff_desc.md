# Diff design record: report-building helpers gathered onto the analyses

## Maintenance motivation

The profiler's analyses all answer the same kind of question: "turn some part of the items
graph (or a small record type derived from it) into a printable row, a JSON object, or a CSV
record." That per-item assembly work had grown as free functions and closures sitting next to
each analysis's `emit` implementation, parameterized over the data they read. Three practical
annoyances kept recurring for anyone touching the report code:

1. The same shallow/retained size and percentage computation was written out separately in the
   text, JSON, and CSV paths of an analysis, so every new column or option touched three
   slightly-different copies.
2. Helper parameter lists grew with every addition (`items`, `depth`, `table`, `obj`, `wtr`,
   totals, option flags), because the helpers were free functions that had to be handed every
   piece of context explicitly.
3. The helpers read as detached: nothing on the page tied them to the analysis they served,
   even though they were only ever called from that analysis's emitters.

The natural move a maintainer reaches for in this codebase is to group the helpers with the
type they serve, shortening parameter lists by letting the helpers live in an `impl` block and
read shared context from the analysis struct. This diff models that refactor as it actually
drifts in practice: the helpers become methods on the analysis types, several of them stop
taking the configuration they were handed and start reading it from the struct instead, and
per-item/per-entry data types are introduced so the formats share one assembly step.

## Modeled development evolution

The changes follow the shape of an ordinary cleanup pass over the `analyze` crate after the
report formats stabilized. First, a per-item value type replaces the ad-hoc tuples and
closures that each emitter had built for itself, with one function producing that value from
an item identifier. Then each analysis's emitter helpers move from module-level free functions
into `impl` blocks on the analysis struct, which is where the struct's other behavior already
lived; the recursive walking helpers move as well so that one calling convention is used
throughout the file. Finally, selection logic that had been a free function taking an options
snapshot becomes part of the analysis whose options those are, and the construction of the
analysis is split so selection can run before the entries exist. No report content changes at
any point: every table, JSON array, and CSV file comes out byte-for-byte the same as before the
refactor.

## Overall design

- `analyze/analyses/top.rs`: a `ItemMeasurement` value type (shallow size and share, retained
  size and share as options, and the display name) now describes one item's row content, and a
  single function produces it from an item identifier plus the retained-sizes flag. The three
  emit paths ask for that value instead of recomputing sizes in place; the text path keeps its
  existing fall-back-to-shallow display rule, and the JSON/CSV paths keep their existing
  retained-field-inclusion rules.
- `analyze/analyses/dominators/emit.rs`: the three per-node builders (`add_text_item`,
  `add_json_item`, `add_csv_item`) and the recursive walkers that drive them move from
  module-level free functions into an `impl` block on the dominators tree type, keeping the
  `CsvRecord` module-level type it returns.
- `analyze/analyses/monos/emit.rs`: the JSON and CSV emitters get the same per-entry processing
  step the text path already had: one method builds the JSON object for an entry, one method
  builds the CSV record, and the CSV `Record` type moves to module level (behind the existing
  CSV feature gate) so the record-construction method can name it.
- `analyze/analyses/paths/mod.rs`: the starting-position search becomes part of the analysis it
  initializes: instead of handing an options snapshot to a free function, the analysis struct
  is built first and asked where traversal should start. Inside, the four search arms are
  written as straight-line branches over items and function names rather than nested
  collectors, so the regular-expression arm can propagate a bad-pattern failure directly with
  `?` instead of threading a result through nested closures. The construction of paths entries
  is unchanged apart from where the starting positions come from.

## Per-cluster rationale

### `top.rs` - shared per-item measurement

This site was selected because the top analysis is where the duplication was most visible: the
text, JSON, and CSV paths each computed a size, a percentage of total, and a name for the same
item, with small differences only in which fields they showed and whether retained values
fell back to shallow ones. The implementation shape - one value type plus one
identifier-to-value step - was selected so that the retained-size display semantics live in
exactly one place, which matters because retained sizes are only meaningful when the
computation that produced them was enabled; concentrating that in one step keeps the guard
(that computation must have run) from being re-derived differently in three formats. The value
type is small and local to the file because nothing outside `top` consumes it; the name rides
along in the value because every format needs it and the item's name lookup belongs with its
sizes for one report row. The production role this serves is the content of the top-listing
report: bytes, percent of binary, and item name for each row, plus the retained variants of
the first two when the retained option was requested.

### `dominators/emit.rs` - per-node builders as tree methods

The dominators analysis had the longest-standing free-function chain in the crate: three
builders taking a tree (or an options snapshot) plus a node, its depth, and the appropriate
output destination, plus three recursive walkers. As retained sizes and row-per-node records
were added over time, those parameter lists grew, and the free functions made the file's
structure hard to follow because the walkers and builders sat above the emitters with no
visual tie to the type they served. Housing them in the tree's `impl` block lets the walkers
call the builders and themselves through `self`, keeps the emitters to a handful of lines each,
and groups everything the analysis does with one visual boundary. The production role is the
dominators report itself: for text, an indented list of node sizes and shares; for JSON, one
array of per-node objects with retained sizes; for CSV, one record per node carrying its
size, retained size, immediate dominator reference, and name. The text label formatting (size,
percent, and indentation before the node name) keeps its existing shape because the
expectation-encoded rendering of retained sizes and percentage widths is load-bearing for the
report's readability.

### `monos/emit.rs` - per-entry JSON and CSV assembly

The monomorphizations analysis already had the text path's row iterator doing per-entry work
in place; the JSON and CSV paths still assembled their objects and records inline in the emit
loops. The selected change gives those two formats the same per-entry step, so all three
formats now have a clearly named "turn one entry into rows/objects/records" boundary while the
text iterator keeps its nested-helper shape (its returned iterator captures the entry, which
does not fit a method that returns a borrowed iterator over host data). The CSV `Record` type
moves to module level with `pub(super)` fields because the record-building step returns it and
a nested type definition cannot be named from the `impl` bodies cleanly under the existing
feature gate; the JSON step returns the same error type the JSON writer uses so failures
propagate as before. The production role is the monomorphizations report: one entry per generic
function, its bloat bytes and percent of binary, its total size and percent, and the list of
its monomorphizations (as nested objects for JSON, joined into one CSV column).

### `paths/mod.rs` - starting-position selection as part of the analysis

Traversal starting positions were computed by a free function that received the items graph
and a copy of the options, before the analysis struct existed; the entries that make up the
report were then built from the returned identifiers. The selected change constructs the
analysis first - options snapshot and an empty entry list - and asks it where to start, which
keeps the analysis's options read in one place instead of being cloned into a helper's
arguments alongside a struct that also holds them. The search arms are written as flattened
branches: the regular-expression arm compiles the set and can return an error directly with
`?`, the exact-name arm walks the given names and looks each up, and the argument-less arms
order the traversal frontier largest-first (neighbors below the meta root when descending,
every non-meta-root item when ascending, matching how the report opens with the biggest
offenders). Flattening the arms also keeps each branch's collection visibly local, which is
where bad-pattern failures and empty match lists had been easiest to mis-handle. The
production role is which retaining-paths the report opens with, under every combination of
the names, regular-expression, and direction options.
