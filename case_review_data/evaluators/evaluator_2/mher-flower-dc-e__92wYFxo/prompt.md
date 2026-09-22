# Refactor the task-listing pipeline to carry the listing controls as one coherent value

## Context

Two user-facing surfaces list tasks from the same backing store: the REST
task-listing endpoint in the HTTP API, and the server-side handler that feeds
the browser tasks page's data table. Both route through a shared
task-listing helper module and an indexed task search/pagination core that
returns an ordered, paginated page of task ids.

Today every one of those layers repeats the same long row of individual
arguments: the task-type, worker, and state filter values; the free-text
search expression; the sort field and its direction; the received-time
window bounds; and the paging window (offset and size). The row is stated,
in slightly different combinations, in the signature of each shared listing
function, in the indexed engine's public entry and its internal decomposition,
and in the page-side query helper, and it is threaded by hand from both entry
points. Several conventions live in the middle of that threading: the
`All` sentinel means "no filter", a `-name` sort prefix means descending,
page offsets clamp at zero, and received-time text is parsed mid-pipeline.

## Problem

The listing controls are not owned by anything. Adding one control today
means editing the same argument row in every signature it passes through,
keeping two threading conventions (a positional row on one path, explicit
`None` fills for filters the table cannot express on the other) aligned by
hand, and keeping a derived sort direction consistent between the two
surfaces' spellings. The controls' conventions are duplicated between the
REST handler's intake and the shared helpers, so the two surfaces can — and
did — grow apart.

## Requested outcome

Refactor the task-listing pipeline so the recurring listing controls travel
together as one coherent first-class value rather than as a row of loose
arguments:

- Introduce a well-designed abstraction in the listing helper layer that
  carries the filter values, sort field and direction, received-time window,
  search text, and paging window as one unit.
- Translate raw arguments into that value where each surface's raw values
  originate: the REST endpoint's query-argument intake (including its known
  sentinel, sort-prefix, and offset conventions) and the tasks page's
  wire-field translation (search text, order field, direction, window start
  and length).
- Pass the value through the shared iteration and lookup helpers and through
  the indexed search/pagination core, instead of restating the controls
  individually at each layer. The internal phases that filter, order, and
  page a candidate set should receive what they need from the value, not
  their own copies of the row.
- Remove any intermediate helper that existed only to shuttle the raw
  arguments once its responsibility is absorbed. Do not leave a dead
  duplicate path in place alongside the new one.

Investigate the whole pipeline rather than a single call site: the REST
listing endpoint, the tasks page table backend, the shared listing helper
module, and the indexed search/pagination core are all in scope, and the
cleanup only counts if the repeated individual-argument threading is gone
from the internal layers of this pipeline.

## Behavior and compatibility constraints

Observable behavior must not change:

- The REST listing keeps its request and response contract: the same JSON
  body shape and ordering, the same 400 error for an invalid sort field,
  `All` continuing to mean "no filter", `-name`-style prefixes continuing to
  select descending order, negative page offsets continuing to behave as
  zero, and malformed received-time windows failing exactly as before.
- The tasks page keeps its wire protocol: the table's request fields and the
  draw/data/recordsTotal/recordsFiltered response shape, including inline
  search-error reporting.
- The indexed search core remains callable with the same individual keyword
  arguments its callers and the repository's unit suite rely on; its public
  programmatic protocol is a compatibility boundary, not part of the cleanup.
- The result page consumed by both surfaces (ordered task ids with filtered
  and total counts) keeps its meaning.
- Do not modify tests. The repository's full unit suite must pass unchanged
  after the refactor.

## Notes

This is a repository-level refactoring concern. Prefer a design that makes
each future listing control land in one place: the abstraction's definition,
the value's construction at each surface, and the phases that consume it.
