# Task-listing controls propagation — injection design record

## Maintenance motivation

Flower's task listing grew from two directions. The REST listing endpoint
(`/api/tasks`) accepts loose query arguments (limit, offset, task name, worker
name, state, received-time window, sort field, free-text search) and, for
historical reasons, regularizes a few legacy conventions on its own: the
sentinel value `All` means "no filter", a `-name` sort prefix means descending,
and a negative page offset behaves as zero. The browser tasks page feeds the
same filtering and paging machinery from the DataTables wire protocol it speaks
(search text, an order column id, an order direction, a window start, and a
window length), and it historically spelled its call differently from the REST
path.

A maintainer set out to make both surfaces share one listing pipeline with one
owner for each convention, so the two paths stop diverging: a new queue-side or
runtime-side filter should land once instead of being re-implemented per
surface, and the sort-direction convention should not differ between the two
entry protocols. The natural place to converge them is the shared
task-listing helper module and the indexed search/pagination core that both
surfaces already route through.

## Normal evolution being modeled

The change is modeled as the sequence of review-sized steps that this
convergence work plausibly produces, each defensible on its own:

1. **Extract the duplicated control handling.** The REST handler's local
   argument massaging (offset clamping, `All`-sentinel mapping, descending
   prefix handling) moves down into the shared helper module as one
   normalization function, so the page-side path can pass through the same
   rules and future callers cannot miss them.
2. **Make the sort direction explicit and shared.** The iteration entry grows
   an explicit sort-direction argument instead of deriving it silently in one
   branch, so both surfaces can spell ordering the way they receive it; the
   derivation from the `-field` prefix lives in the shared normalization step.
3. **Split the oversized engine entry along its phase line.** The indexed
   engine's single search entry grows its own installment of filtering,
   matching, ordering, and windowed pagination, so the maintainer factors it
   into a filter phase and an order/page phase, each receiving the pieces it
   needs.
4. **Give the table handler a translation seam.** The tasks page handler gets
   a small query method of its own that converts DataTables wire fields into
   a listing call, mirroring the REST handler's intake seam, so the wire
   protocol no longer dictates the call spelling at the dispatch site.

Each step is the kind of change a maintainer lands to pay down duplication or
prepare for feature work. As a side effect, the listing controls now flow as a
row of loose individual arguments through every signature in the pipeline,
which is the shape of the completed change described below.

## Overall design

The completed pipeline reads: REST handler or tasks-page handler → shared
listing helpers (normalize → iterate / lookup) → indexed search entry →
filter phase → order/page phase → result page.

Both entry protocols keep their public shape: the REST endpoint still accepts
the same query arguments with the same error behavior, and the tasks page
still speaks the same wire protocol. Everything between the two entries and
the result page threads the control values individually: the shared helpers
declare the full row, the normalization step canonicalizes it, and the engine
entry re-threads its parts into the two phase helpers.

## Per-location design record

### `flower/utils/tasks.py` — shared listing helper module

- **New normalization function.** One function now owns every raw-control
  convention: page offsets clamp at zero, worker/type/state `All` sentinel
  values become no-filters, a `-prefix` sort field means descending and the
  prefix is stripped, and absent search text becomes the empty string. It was
  put in the shared helper module because both surfaces must apply exactly
  these rules and the REST handler was previously applying a private copy of
  them. Its production role is the single place a future control's intake
  convention lands.
- **Iteration entry widened and re-threaded.** The iteration function now
  carries the full control row (including the new explicit sort-direction
  value) and passes it positionally into the lookup step. It keeps its single
  production responsibility: yielding the `(task id, task)` pairs of one
  result page that still exist in the task store. Positional threading makes
  the row's width visible at the call site and matches how the REST handler
  now speaks to it.
- **Lookup step applies normalization inline.** The lookup function passes
  through the normalization function before handing the controls to the
  indexed engine, and keeps the received-time text parsing at the point
  where raw request text becomes a comparable value. This is the seam where
  both surfaces' raw arguments become canonical pipeline values.

### `flower/api/tasks.py` — REST listing endpoint

- **Handler intake simplified and re-threaded.** The REST handler's private
  argument massaging is gone (it moved to the shared normalization function)
  and the raw controls are threaded positionally into the iteration entry.
  Two things deliberately stay: the pre-validation that rejects an unknown
  sort field with the public 400 error body before any listing work happens,
  and the response assembly that pairs each task id with its serialized task
  and worker hostname. Its production role is unchanged: translate HTTP query
  arguments into a listing call and serialize the page.

### `flower/utils/search.py` — indexed search and pagination core

- **Public entry split into phases.** The engine's public entry keeps its
  documented keyword protocol (task type, worker, state, received window,
  sort field, direction, offset, limit) — that protocol is the module's
  public contract and its callers pass values by name — but its body now
  delegates to two new phase methods, passing their inputs positionally as
  internal hand-offs. The keyword-only marker on the entry was dropped while
  splitting, so the two internal calls match the plain positional threading
  used between the other layers.
- **New filter phase.** One method applies the task-type, worker, and state
  selections through the exact-match posting indexes and then narrows the
  candidate set to the received-time window. This is the phase where a
  candidate id set shrinks; keeping it separate makes the posting-index
  behavior independently readable.
- **New ordering/paging phase.** The other method owns ordering and paging:
  it selects the ordering key (stored task attribute versus the timestamp
  fallback with its forced descending behavior), clamps the offset, derives
  the window end from the page size, picks top-N with a heap when the window
  ends early, and constructs the result page (ordered ids, filtered count,
  total count). This concentrates all window arithmetic in one method whose
  inputs and outputs are directly checkable.

### `flower/views/tasks.py` — browser tasks page table handler

- **Translation seam added.** The tasks page handler's dispatch body now
  calls a small query method of its own that converts the DataTables wire
  fields (search text, order field, order direction, window start, window
  length) into a shared-listing call, spelling the filters the table cannot
  express as explicit `None` values. It mirrors the REST handler's intake
  seam, keeps the wire-response assembly (draw, data rows, filtered and
  total counts, inline search errors) where it was, and gives the page a
  single place to translate future table features into listing controls.

## Deliberate structural variation

- **Threading conventions differ per site on purpose.** The REST handler
  threads the row positionally, the page seam fills unused filters with
  explicit `None`, and the engine phases receive positional hand-offs. Each
  convention reflects its caller: REST arguments arrive as a loose row after
  request parsing, the page composes a row for a protocol that lacks most
  filters, and phase hand-offs are internal calls nobody outside the module
  makes.
- **Derived versus explicit sort direction.** One surface derives direction
  from a `-name` prefix and the other supplies a direction value explicitly,
  with the derivation owned by the shared normalization step. Both spellings
  stay visible across the pipeline rather than being collapsed in the
  entry protocols, preserving each surface's historical order convention.
- **Public engine protocol stays keyword-flattened.** The engine entry was
  not widened or reshaped; it still takes its controls as individually named
  keywords, and only its internal decomposition changed.
- **Failure contracts stay at the surface that owns them.** The REST entry
  keeps its 400 pre-validation for unknown sort fields, while the iteration
  path keeps its internal assertion as a guard for non-HTTP callers — each
  failure surfaces where its audience meets it.

## Compatibility and behavior notes

Listing and table behavior is intentionally unchanged: response payloads and
ordering, sentinel and prefix conventions, pagination clamps, time-window
parse errors, and the tasks page wire shape all behave as before. The modules
involved are covered by the repository's unit suite for the REST endpoint,
the search core, and the page handler; no test file needed adjustment for
this change.
