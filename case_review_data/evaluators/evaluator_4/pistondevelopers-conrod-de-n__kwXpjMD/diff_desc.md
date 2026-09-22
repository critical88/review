# Injection design record: deferred-bookkeeping experiments in `conrod_core`

## Maintenance motivation

`conrod_core`'s update pipeline has several recurring costs: every frame it
re-derives window bookkeeping, re-aggregates all input events, re-visits the
widget graph to rebuild the depth order, and recalculates every scroll offset
from freshly accumulated scroll amounts. Each of these costs invites the same
kind of experiment: defer the bookkeeping -- reuse what the previous update
already computed, and only pay for the change. Because those experiments must
be evaluable against the supported behavior without forking the crate, they get
introduced behind a small crate-local constant: both the deferring policy and
the supported policy compile, and flipping the constant selects between them.

The maintenance episode this change models is what happens when such an
experiment is *abandoned* rather than landed: the constant gets pinned to the
supported side (usually with a comment explaining why the deferred behavior was
rejected), and the deferred side is left in the tree "until we have time to
remove it". Nothing forces the removal at that point -- the abandoned branch
still compiles, its helpers still type-check, its state fields still get
initialized -- so the experiment survives release after release as code that
every reader must understand and every contributor must keep compiling, while
no configuration of the crate can ever reach it.

## Simulated development evolution

The diff reconstructs the residue four separate abandoned experiments would
leave in the update pipeline, one per pipeline responsibility, plus a shared
helper that outlived its experiment:

1. **Window-dimension tracking via the widget graph** (`conrod_core/src/ui.rs`).
   The UI learns about resizes through `Input::Resize`, but a class of backends
   instead synchronises the window's dimensions straight into the window
   widget's `kid_area` rect. While such a backend was being evaluated, the UI
   could not trust the resize report and re-derived its `win_w`/`win_h` fields
   from the graph; once every backend settled on forwarding resizes through the
   event loop, the graph-derived path became unreachable. The residue is the
   fixed `TRACK_WINDOW_DIMS_VIA_GRAPH` setting (pinned `false`), the
   re-derivation branch it guards inside `Ui::handle_event`, the
   `window_dimensions_from_graph` helper the branch drives, and the dimension
   comparison the branch conditions its store on.

2. **Batched input emission** (`conrod_core/src/input/global.rs`). Buffering
   events between updates so that scroll-driven widgets observe one net change
   per update was trialled to remove per-event jitter; it pushed `event::Ui`s
   one update into the future and visibly desynchronised scrolling, so
   immediate emission remained the rule. The residue is the fixed
   `EMIT_EVENTS_IMMEDIATELY` setting (pinned `true`), the buffering `else` side
   of `Global::push_event`, the private `buffer_event_for_next_update` helper,
   and the `pending_events` buffer field (plus its initialization) that nothing
   drains.

3. **Retained depth ordering** (`conrod_core/src/graph/depth_order.rs`).
   `DepthOrder::update` has a documented FIXME about the cost of re-visiting
   the whole graph on every update; re-using the ordering retained from the
   previous update and merging in only the changed indices was the obvious
   experiment to try against that FIXME. It broke the "last clicked comes
   last" behaviour for floating widgets stacked over other floating widgets,
   so re-sorting on every update stayed. The residue is the fixed
   `SORT_ON_EVERY_UPDATE` setting (pinned `true`), the retained-index `else`
   side of the update branch, and the private `merge_into_retained_index`
   helper carrying that policy.

4. **Retained scroll offsets** (`conrod_core/src/widget/scroll.rs`).
   `widget::scroll::State::update` recalculates the new offset from the fresh
   `additional_offset` on every update; during the jitter investigation of
   multi-update scroll bursts, re-using the whole offset retained from the
   previous update was trialled as the cheaper alternative. It froze the
   offset whenever the calculated bounds changed with the kid area, so
   recalculating each update was kept as the only supported source. The
   residue is the crate-private `OffsetSource` enum whose second variant
   (`RetainPrevious`) is never constructed, the fixed
   `SCROLL_OFFSET_SOURCE` selection constant pinned to
   `RecalculateEachUpdate`, and the unselectable `match` arm in the offset
   computation.

5. **A shared comparison helper** (`conrod_core/src/utils.rs`). While the
   graph-derived resize tracking (experiment 1) was being debugged, dimensions
   flowing in from two sources needed a tolerance-aware comparison, so a small
   `dims_differ` predicate with a pixel-fraction epsilon went into the crate's
   shared utils so both consumers could use it. When the deferred tracking side
   lost, the predicate lost its last live caller with it. The residue is
   `pub(crate) fn dims_differ` and `DIMS_DIFFER_EPSILON`.

## Overall design

The five clusters were selected so that one maintenance episode is spread over
the distinct production responsibilities of the frame-update pipeline: event
ingestion (`Ui::handle_event`), event aggregation between updates
(`input::Global`), render order maintenance (`graph::DepthOrder`), scrollable
widget state (`widget::scroll`), and the shared geometry predicates
(`utils`). Each cluster is a place where an immediate-vs-deferred trade-off is
commercially plausible, framed against real neighboring code (the `DepthOrder`
FIXME, the existing double-buffer redraw ceremony, the real per-frame update
cycle), and each was shaped to leave differently-structured unreachable
residue:

- three `bool` settings pinned to the supported side (two pinned `true`, one
  pinned `false`, mirroring how "keep the existing default" usually wins an
  abandoned trial), each leaving one dead `if`/`else` side behind;
- one enum-based selection leaving an unselectable `match` arm and a
  never-constructed variant behind;
- private helper functions and methods that only the dead sides call, a written
  but never drained buffer field, and a shared predicate helper with no live
  callers, i.e. the kind of supporting cast a frozen experiment drags along.

Every live code path computes what it computed before the change: the guarded
settings are all pinned to whichever side reproduces the original statements,
so the added branches are decision ceremony around untouched behavior. The
crates's public surface is not extended to host any of this -- the added
settings, enum, helper and field are all crate-private or `pub(crate)` in
service of the experiments, and no existing `pub` item is altered. No tests
were modified: the experiments are supposed to be observably inert.

## Per-cluster rationale

### `conrod_core/src/ui.rs` -- graph-derived window-dimension tracking

**What changed.** A crate-private `const TRACK_WINDOW_DIMS_VIA_GRAPH = false`
at module scope carries the experiment's decision; inside the resize arm of
`Ui::handle_event`, the original two assignments (`self.win_w = w;
self.win_h = h;`) move into the `else` side of a guard on that constant, and a
`then` side re-derives both fields from the widget graph instead, guarded by a
`utils::dims_differ` comparison that avoids re-writing unchanged values; the
free function `window_dimensions_from_graph` at the end of the module reads
the window widget's `kid_area` rect (falling back to the current dimensions
when the window has no state yet) to serve that side.

**Why this site and shape.** `Ui::handle_event` is the single place the UI
translates `Input::Resize` into its own state, so it is exactly where the two
dimension-ownership models diverge, and the then/else guard is the minimal
shape that lets a backend keep both models selectable at compile time. The
helper is a file-local free function because only this module consumes the
graph view, and it deliberately takes `&Ui` (not the event) because the
graph-owned model by definition ignores the event's report.

**Production role.** Keeps `win_w`/`win_h` consistent with the window widget's
laid-out area in the graph-owned arrangement -- resize handling that works
even for backends that never emit a faithful resize report. With every current
backend forwarding resizes through the event loop, this side can no longer be
selected.

### `conrod_core/src/input/global.rs` -- batched event emission

**What changed.** A crate-private `const EMIT_EVENTS_IMMEDIATELY = true`
carrying the emission policy; `Global::push_event` now guards its original
`self.events.push(event)` behind the constant and adds a buffering `else`
side; private `fn buffer_event_for_next_update` implements the buffering;
`Global` gains a `pending_events: Vec<event::Event>` field (documented as
only-used-by-the-buffering side) initialized alongside the existing `events`
in `Global::new`.

**Why this site and shape.** `Global` aggregates everything that happens
between two consecutive updates, so "aggregate differently" is a policy that
belongs to it; a keyed push_event is the natural seam at which the immediate
and deferred emission models diverge. A struct field plus a one-call helper is
the honest shape of the deferred model: the buffer has to live somewhere, and
routing the push through a named method keeps the batching intent readable.

**Production role.** Collecting events into a pending buffer that the next
update would flush in one batch, so scroll-driven widgets observe a single net
change per update rather than every intermediate event. Because the flush side
was abandoned, the buffer is written by nothing that ever drains it.

### `conrod_core/src/graph/depth_order.rs` -- retained index merging

**What changed.** A crate-private `const SORT_ON_EVERY_UPDATE = true` at
module scope, with a doc comment tying it to the cost FIXME in
`DepthOrder::update`; the body of `update` moves into the constant's `then`
side, a new `else` side delegates to a new private
`DepthOrder::merge_into_retained_index`, and that helper implements the
retained policy: visit only the updated widgets, splice unseen indices into
the ordering retained from the previous update (preserving its relative
depth), and append the floating widgets popped off the stash in order.

**Why this site and shape.** `DepthOrder::update` already owns the
visit-and-sort machinery that both policies start from, and the FIXME
documenting the cost of re-sorting makes the deferral experiment the natural
neighboring decision. The retained policy needs its own helper rather than a
variant of the established "last clicked comes last" comparison because it
must preserve the retained sequence rather than agree with it, so a separate
`fn` with its own splice semantics is the shape a maintainer would write.

**Production role.** Re-using the previous update's depth order and merging
only changed widget indices into it, avoiding a full graph re-visit per
update -- and specifically preserving the retained click order of floating
widgets, which is precisely the behavior that turned out to depend on the full
re-sort and got the experiment rejected.

### `conrod_core/src/widget/scroll.rs` -- retained offsets

**What changed.** A crate-private two-variant `OffsetSource` enum
(`RecalculateEachUpdate`, `RetainPrevious`) documenting both policies, a
crate-private `const SCROLL_OFFSET_SOURCE` pinned to `RecalculateEachUpdate`,
and the un-bounded offset computation in `State::update` refactored from one
`let` bind into a two-arm `match` on that constant: the recalculating arm
performs the original accumulate-and-clamp-to-previous-bounds sequence, and
the retaining arm re-uses the whole offset stored in the previous state
(falling back to the current offset with no previous state).

**Why this site and shape.** `State::update` is where a scrollable widget's
offset policy lives, and unlike the three boolean experiments, this policy
choice reads most naturally as selecting between two named strategies,
because the retained variant carries its own semantics rather than being a
negated default. Enum + constant (rather than a third `bool`) models how a
maintainer names that choice; the never-constructed second variant is what the
frozen decision implies. The subsequent NaN-clamping block is left shared and
unchanged since both policies must still produce a bounded offset.

**Production role.** Re-using the previous update's whole offset to avoid the
per-update recalculation cost during rapid scroll bursts, with the
recalculation side carrying the load-bearing bound clamping that keeps
scrolling stable as bounds change while scrolling.

### `conrod_core/src/utils.rs` -- shared dimension comparison

**What changed.** A `DIMS_DIFFER_EPSILON` tolerance constant and a
`pub(crate)`-visible `dims_differ(a, b)` predicate comparing two dimension
pairs element-wise against that epsilon, added to the module's helper section
alongside the crate's other small numeric utilities, plus the corresponding
`Dimensions`/`Scalar` imports.

**Why this site and shape.** The deferred window tracking needed a
tolerance-aware comparison between two sources of the same dimensions (resize
report vs. laid-out rect), and single-purpose float predicates are exactly
what this module exists to share across the crate, so placing it here is the
habitual home; `pub(crate)` (rather than a private re-export) mirrors how
the module's other helpers are exposed for cross-module reuse.

**Production role.** Suppresses redundant re-writes of the UI's window
dimensions when the two sources disagree only by float dust. Once the
graph-derived side of the resize handling became unreachable, the predicate's
only remaining caller disappeared with it, leaving a still-exposed shared
helper with no live consumers.
