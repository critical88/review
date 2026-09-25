# Injection design record — session-facilitated stream diagnostics

## Maintenance motivation

Operating smux in production, the recurring support questions are per-stream:
which stream is stalled on a connection, and why. Is the peer withholding
window? Is our receive side backed up? Is the write scheduler queueing more
than it can drain? Sessions are the handle operators and tooling actually
hold — they hold the connection, but not every stream — so a session-faceted
diagnostics surface is the natural first deliverable: a point-in-time
accounting view of one stream by id, plus the write scheduler's backlog.

That feature is real work, and it touches the transport's most sensitive
state: per-stream receive counters, v2 sliding-window counters, the receive
ring's storage, and the scheduler's per-stream queues. The diff depicts the
landing of that feature together with a small flow-control cleanup the same
change pretended to deliver: hoisting the hot paths' accounting arithmetic
into reusable session-level points.

## Modeled development evolution

The diff models a drift that happens in two believable steps rather than one
fabricated jump.

1. **Flow-control cleanup first.** A reviewer of the v2 read/write paths
   notices the same consumption accounting block inlined twice and the same
   sliding-window computation inlined in the write path, and asks for it to
   be deduplicated. The author — reasoning that the session is "the transport
   policy owner" because it owns token buckets and frame scheduling — places
   the shared helpers on Session, parameterized by the stream they act on.
   The three hot-path call sites are rewired through the new coordinators.
   Behavior at this point is identical; the review feedback is satisfied;
   nothing else changes.

2. **Diagnostics second.** The operator surface lands on Session because
   sessions own the stream-id space and the registry. The per-stream view
   is "just a read", so its author fills it by reaching into the stream's
   fields directly, reusing the ring-size probe already parked next to the
   buffer plumbing; the backlog probe is written where the need is felt,
   reading the scheduler's atomic count and walking its round-robin list
   without taking its lock, because the scheduler and sender loops must not
   block on diagnostics.

Each step is the kind of change real code review accepts: it compiles, it
preserves the protocol exactly, it centralizes something, and it ships.

## Design

One coherent story spans three production areas:

**A. Hot-path session coordinators (session.go, with call-site rewires in
stream.go).** Two helpers on Session — one recording received-consumption
bookkeeping on a stream and deciding when the consumed-amount announcement
is due, one computing the v2 send window from written/consumed/advertised
counters with the early-return check for malformed peer accounting. The
direct-read accounting site, the ReadTo-style accounting site, and the v2
write-path window gate are rewired through them. The parameter-taken
receiver shape is deliberate: it is the ordinary signature of a
"coordinating" method, and it makes the helper's session placement look
justifiable at review time ("the session mediates stream policy") even
though every field it touches belongs to the stream.

**B. Diagnostics surface (new file stats.go).** An exported accounting view
of one stream (identity, cumulative bytes both directions, locally buffered
volume, in-flight and peer-advertised window values, read-closed,
write-closed and closed flags), served by a session-level lookup that
resolves the stream id through the session's registry, then assembles the
view from stream fields; and an exported scheduler backlog pair (queued
frames, queued payload bytes) computed from the scheduler's atomic count
plus a lock-free walk of its round-robin list and per-stream heaps. The
lookup half is correctly session-work; the assembly half reads eight
distinct stream fields from outside. The lock-free walk is intentionally
documented in-code as a sampling tradeoff, so the design reads as a
considered choice rather than negligence.

**C. Ring probe (stream.go).** A small `stream` method reporting the number
of payload bytes currently held in the stream's receive ring, implemented by
taking a local descriptor alias of the ring handle and scanning its slots,
head index, capacity mask and size — parked next to the stream's other
buffer plumbing because "it is about buffers", and consumed by the
accounting view in area B.

## Per-location rationale

Why these sites and these shapes:

- **The two hot-path helpers** sit on Session with stream parameters because
  that is where a "transport policy" review would naturally centralize them:
  Session already holds the token bucket and the write scheduler, so the
  placement masquerades as consistency. Their bodies are the strongest form
  of the misplaced-reach relation: seven accesses across three stream fields
  in the accounting helper, window derivation over three window fields in
  the window helper, with zero own state interest in both.

- **The three rewired call paths** are genuine production paths (both
  receive-accounting branches and the v2 window gate), so the rewire is
  load-bearing rather than staging: any later change to the accounting or
  windowing semantics now has to be negotiated on the session side even
  though the state is per-stream. The window-gate site keeps the
  malformed-consumed early return returning the same error as before, and
  the accounting sites exchange the inlined blocks for coordinator calls
  under the same buffer discipline — precise behavior preservation at the
  hot-path level.

- **The per-stream accounting view** uses a session-level lookup followed by
  field-by-field assembly, with lifecycle state sensed through non-blocking
  channel probes. This is the shape real diagnostics code takes when the
  author treats a snapshot as "just reads": it keeps the session's registry
  work in place while quietly moving the stream's reporting logic outward.
  The result-value construction stays local to the method, which keeps the
  change honest about what is output versus whose state is being read.

- **The backlog probe** reads the atomic count and walks the scheduler's
  containers without its lock. The in-code note about tolerating transient
  inconsistency is the rationalization that would survive review; the
  consequence is that the session side now depends on the scheduler's
  internal container layout, not just its operations.

- **The ring probe** takes a descriptor alias before scanning. The alias is
  idiomatic Go habit (hoist the field, avoid repeated loads) and it makes
  the scan read as if it were operating on local data; tracking it is the
  kind of detail that decides whether ownership analysis is mechanical or
  semantic.

## Structural variation on purpose

The additions deliberately do not share one implementation mold. They cover:

- mutation plus threshold re-arming (accounting), guarded arithmetic
  (window), serialized projection after a registry hit (accounting view),
  lock-free cross-component aggregation (backlog), and an alias-rooted
  layout scan (ring probe);
- three different foreign owners (the stream type, the scheduler queue, the
  receive ring);
- different own-interest levels from zero-access helpers to a method with
  real session duties attached (registry and lock);
- spread across the connection-orchestration file, the stream-internals
  file, and a new interface file, and across both hot paths and
  operator-facing API.

The variation is what makes the change resemble organic growth: a single
uniform pattern repeated five times would read as scaffolding, whereas the
drift being modeled is a set of locally reasonable decisions that happen to
pull the same direction.

## Production role of every materially changed function

- Session receive-consumption coordinator: advances per-stream read
  counters, applies the window-update trigger, returns the announcement
  amount for the read paths.
- Session send-window coordinator: derives per-stream v2 send window and
  the malformed-peer verdict for the write path.
- Session accounting lookup: resolves a stream id through the registry
  (under the registry lock) and assembles the exported per-stream view.
- Session backlog probe: exports the scheduler's pending frame count and
  queued payload byte volume.
- Stream accounting call sites (two): the direct-read and ReadTo-style
  consumption paths now delegate their accounting to the coordinator.
- Stream v2 write site: the window gate now consults the session-side
  window coordinator.
- Stream ring probe: reports locally buffered payload volume for the
  accounting view.
