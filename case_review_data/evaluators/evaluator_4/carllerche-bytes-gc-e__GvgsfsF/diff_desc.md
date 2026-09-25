# Design record: shared cursor mechanics for the buffer adapter family

## Maintenance motivation

The buffer module ships six small adapters around `Buf`/`BufMut`: a read-side
window adapter, a write-side window adapter, a two-region adapter, an `io`
read bridge, an `io` write bridge, and a byte iterator. Each of them is a
windowed view over one or two byte regions, and each of them re-implements
the same handful of cursor rules in its own private fields:

* which region is currently active and when to move to the next one;
* how a window budget clamps `remaining`/`chunk`/`chunk_mut` and how the
  budget is decremented per transfer;
* how a request splits across two regions (the two-region walk, present
  once per direction);
* how the `io` bridges move bytes and how iteration steps one byte at a
  time.

These rules drifted apart over time: the window adapters assert against
their budget on advance while the two-region walk clamps by arithmetic; the
two-region split logic exists in four near-identical copies (read walk, write
walk, both spanning-copy paths); the read-side and write-side window rules
are mirror images maintained twice. The last several changes to this area
touched the same arithmetic in three or four places at once, and reviews
kept flagging that a rule corrected in one adapter was missed in its sibling.
An internal consolidation pass — the kind that puts a utility layer's cursor
rules in one authoritative place —
was the natural next maintenance step.

The pass was explicitly scoped: **the public API surface of every adapter
stays frozen** (same types, same constructors, same accessors, same trait
impls with the same edge-case behavior, same feature gates), and only the
mechanics move.

## The evolution being modeled

This is a normal internal-utility refactor of the kind that appears in a
buffer library: extract the duplicated traversal/windowing arithmetic into a
private engine type, place it next to the adapters, and turn the adapters
into stable façades. Consolidations like this usually carry three payloads:

1. one authoritative copy of each cursor rule (no more drift between the
   read-side and write-side windows, or between the two-region directions);
2. scaffolding for work that is *anticipated*: a byte-traffic
   counter in the engine ("consumed") is deliberately installed now so a
   later observability pass can report cursor traffic without re-instrumenting
   every adapter;
3. a single place where a future adapter — for example a paired `io` reader
   or a telemetry wrapper — would inherit the same rules instead of copying
   them again.

## Overall design

A new crate-private engine type, `BufCursor<T, U = ()>`, owns the cursor
mechanics for the whole family:

* `front: T` — the region always consulted first;
* `tail: Option<U>` — the second region of a paired cursor, `None` for
  single-region cursors ("paired" vs "single" is the only mode dimension);
* `limit: usize` — the window budget, `usize::MAX` when the cursor is
  unlimited, so window arithmetic keeps one comparison form instead of
  optional-budget plumbing in every walk;
* `consumed: usize` — bidirectional traffic moved through the cursor (the
  bookkeeping with no consumer yet, described above).

Single-region cursors (`Take`, `Limit`, `Reader`, `Writer`, the byte
iterator) pass `()` as the region type; a tiny `Buf`/`BufMut` implementation
for the unit type backs the `U = ()` default so every construction satisfies
the same bounds with one type-parameter set. Paired cursors (the two-region
adapter) construct with `paired(front, tail)` and carry the paired-region
invariant; accessors that expect a paired cursor state that contract with an
`expect` on the selector.

The engine implements every responsibility the family needs from it, and
the adapters keep only their public surface: constructors, accessors, and
trait methods that delegate mechanics one level down. No public item was
added, removed, or re-signatured anywhere.

## Per-location rationale

### `src/buf/cursor.rs` — the shared engine (new file)

One file, one engine, all families:

* **`Buf` walk** (`remaining`/`chunk`/`advance`/`copy_to_bytes`/`chunks_vectored`):
  the authoritative read-side rules. The single-region branch carries the
  window clamp and window decrement of the read-side window adapter; the
  paired branch carries the front-then-tail drain order of the two-region
  adapter, including the spanning copy that assembles exactly `len` bytes
  across both regions and, under `std`, the vectored path with its
  window-limited slice fill (the existing `IoSlice` lifetime technique is
  carried over verbatim from the read-side window adapter it serves).
* **`BufMut` walk** (`remaining_mut`/`chunk_mut`/`advance_mut`): the
  write-side mirror, same windows, same drain order on the paired branch,
  same over-advance assertion shape.
* **`io::Read` + `io::BufRead`** (std only): reads copy out of the active
  region clamped to usable capacity; the buffered view exposes the current
  chunk without consuming. The bridges live on the engine rather than on the
  adapter so a future paired io consumer gets them for free — that was the
  strongest argument for the engine holding the bridges rather than adapters
  calling helper functions.
* **`io::Write`** (std only): the write-side mirror, filling the active
  region clamped to usable capacity, flush a success no-op.
* **`Iterator`**: yields the front byte of the active (windowed) chunk and
  advances one byte, with exact size hints.

The `advance`/`read`/`write`/`next` paths each keep a single-region fast
path that bypasses the generic mode branch when no tail exists. The window
adapters and io bridges are hot for their callers, and the fast path removes
the selector branch from the common cases while keeping one implementation of
each rule.

Placement and form: the engine sits in the buffer module tree next to what
it serves; it is crate-private (`pub(crate)`); and it is generic over the
region types rather than trait-object based so no adapter pays dynamic
dispatch.

### `src/buf/take.rs` — the read-side window adapter

The adapter that made the drift most visible (its window arithmetic was
mirrored by the write-side window adapter and its walk shared by the
two-region walk). Its fields are replaced by one engine cursor constructed
with `with_limit(inner, limit)`; the window getter, setter, and accessors
forward to the cursor's own budget. Its `Buf` impl becomes a thin delegation
surface. The window assertion itself now lives in the engine's `advance`,
where it also defends paired cursors constructed with a budget.

### `src/buf/limit.rs` — the write-side window adapter

The write-side mirror of the read-side window. Delegating it to the same
engine is what keeps the two window rules from drifting again: the clamp
shape, the budget decrement, and the over-advance assertion are now the
engine's, shared by both directions. Only the budget accessors remain local.

### `src/buf/chain.rs` — the two-region adapter

The adapter with the most copies of the split logic (read walk, write walk,
spanning copy, plus per-region accessors). Its two fields become one paired
cursor (`paired(a, b)`); `first_ref`/`first_mut`/`last_ref`/`last_mut`/`into_inner`
map onto the engine's region accessors and its paired-region contract; all
six trait methods delegate. The engine's paired branch now owns the
front-then-tail drain order for both directions and the spanning copy.

### `src/buf/reader.rs` — the io read bridge

Reads and the buffered view become delegations to the engine's `io` bridge
impls. The adapter keeps its constructor and its three accessors, which now
return the cursor's front region — the same underlying values as before.

### `src/buf/writer.rs` — the io write bridge

Same shape on the write side: `write`/`flush` delegate to the engine's
`io::Write` impl; accessors return the front region.

### `src/buf/iter.rs` — the byte iterator

The iterator's field becomes a single-region cursor; `next`/`size_hint`
delegate to the engine's iterator impl, and the existing accessors map onto
the region accessors. Iteration gains the window clamp for free since it now
walks through the engine's chunk rules.

### `src/buf/mod.rs` — module wiring

Registers the new `cursor` module alongside its adapters, matching the
module tree's existing flat layout.

## Behavior kept stable

The conservatism rules for the pass were:

* drain order is unchanged (front region exhausts before the tail region, in
  both directions, including spanning copies);
* window semantics are unchanged (clamps, the over-advance assertion surface,
  the per-transfer decrement, and differing behavior for unlimited
  cursors);
* io semantics are unchanged (capacity-clamped lengths, the buffered view
  without skipping, flush as a success no-op);
* iteration is unchanged (single-byte yields, exact size hints);
* every adapter keeps deriving `Debug` the way it did; note that the derived
  output follows internal field layout, so a wrapped adapter's `Debug` view
  reflects the engine's shape rather than the adapter's former private
  fields — acceptable for a crate-private representation change;
* `std` feature gates are unchanged: the `io` bridges and the vectored path
  remain `std`-only; everything else keeps building without `std`;
* the two `Buf`/`BufMut` impls for `()` exist only to back the default
  region parameter of the engine; they are crate-internal and are not part
  of any public surface.
