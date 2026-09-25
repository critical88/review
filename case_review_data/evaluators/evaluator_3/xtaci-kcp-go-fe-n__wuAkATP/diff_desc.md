# Injection design record — kcp-go session/protocol-engine boundary

## Maintenance motivation being modeled

kcp-go's `UDPSession` is the user-facing connection object; beneath it sit the
`KCP` ARQ engine, the Reed-Solomon FEC encoder/decoder, and the `autoTune`
shard-period detector. In a mature codebase this boundary erodes in a
characteristic way: a hot path gets inlined "temporarily", a debug hook wants
to live in the one file under review, a tuner grows "just one more
responsibility" during the feature it was built for. Nobody plans it; each step
is locally defensible, and the component that owns the data ends up watching
some other component manipulate it.

This change models that erosion as a single maintenance evolution: a
performance-and-observability pass over the session layer that pulled protocol
mechanics up into the connection object, followed by the auto-detect refactor
that gave the period detector a reconfiguration duty. Both are realistic.

The evolution we model, in the order a team would actually have produced it:

1. **Datapath absorption** — while instrumenting the read and write paths to
   chase a latency report, an engineer copies the engine's receive-reassembly
   and send-segmentation rules into session-local helpers so the logging and
   buffer accounting can observe every step without touching the engine. The
   call sites in `Read` and `WriteBuffers` are repointed at the new helpers.
2. **Dropping the layer indirection** — once the session holds the only
   working copy of those rules (the review queue moved slowly, the engine's
   originals stopped being called on any path), a cleanup commit deletes the
   now-unreferenced engine operations. This is the step that makes the
   erosion hard to undo by accident: the rules now exist in exactly one
   place, and it is the wrong one.
3. **Configuration inline** — a separate micro-optimization replaces the
   one-call delegation in the delay/pace setter with direct field assignment
   on the embedded engine, copying the argument validation along with it. The
   engine's own method survives this step (it is referenced in docs), it is
   simply no longer used by the session.
4. **FEC parity absorption** — the outgoing packet pipeline gets the same
   treatment as the datapaths: the FEC encoder's encode rule — shard
   sealing, quorum tracking, continuity window, parity sealing — is lifted
   into a session-level helper so the pipeline stage can time and log around
   it.
5. **Tuner responsibility creep** — during the shard auto-detect work, the
   decoder's parameter-adaptation block is extracted into a method on the
   detector and the decoder is passed in as a parameter ("the detector
   already understands the detected periods, let it apply them"), leaving
   `decode` as a one-line branch.

## Overall shape

No new dependencies appear (`reedsolomon` and `container/heap` are already
package imports), no exported symbol changes, and every moved block keeps its
exact operational semantics — the same computations, in the same order, under
the same locks — so the package remains behaviorally identical from the
outside. The touched areas:

- `sess.go` — the connection layer: two new datapath helpers, one inlined
  setter, one new FEC-parity helper, plus the call-site rewires in the read
  paths, the write-vector splitting loop, and the outgoing pipeline stage.
- `kcp.go` — the engine loses the two operations the session absorbed (the
  reassembly drain and the send segmentation) to step 2 of the history above.
- `fec.go` — the decoder's tuning branch collapses to a single call into the
  detector.
- `autotune.go` — the detector gains the reconfiguration method and the
  codec-construction import it needs.

## Per-location design rationale

### Session read path (`sess.go`): reassembly drain

The receive-reassembly rule moves out of the engine into a session method
that walks the embedded engine's state directly: peek-gated size checks,
drain of the receive queue into the caller's buffer, promotion of segments
from the engine's receive buffer with expected-sequence advance, and arming
of the engine's window tell-probe after a fast recovery.

- **Why this site:** the read path is the hottest, most-instrumented path in
  the library and the one a latency report would put under the microscope —
  the most believable place for an engineer to stop trusting the layer
  boundary.
- **Why this shape:** the code is copied with only the receiver spelling
  changed (`kcp.*` chains rewritten as `s.kcp.*` chains), which is precisely
  how such a copy happens in practice (and the only textual transformation
  that guarantees behavior preservation).
- **Production role after the change:** the *session* now defines when a
  message is complete, how fragments merge, and when the remote peer is told
  about window changes — protocol invariants that previously had one owner
  now depend on connection-layer code.

### Session write path (`sess.go`): segmentation

The send rule gets the same treatment one function lower in the file:
stream-mode append-to-tail growth bounded by the engine's maximum segment
size, fragment-count computation with the >255 rejection, and segment
creation/countdown insertion into the engine's send queue, all now reading
the engine's size/stream fields from session code.

- **Why this site:** stream-mode tail growth needs the segment-size field on
  every write, so it is the path where "one extra indirection" is most often
  suspected; it shares the review queue with the read-path change.
- **Why this shape:** same one-pass copy; the splitting loop in the
  write-vector path repoints at the session helper.
- **Production role:** segment-sizing rules — what the wire gets, and how
  big writes fail — now live at the session layer.

### Engine (`kcp.go`): the originals are deleted

Step 2 of the modeled history: the two absorbed engine operations are
removed, since after rewiring no path references them. Deleting rather than
keeping them is the realistic outcome of the same cleanup pass that produced
the helpers (self-consistency: a tree that kept both would have the same code
twice, which no maintainer would commit knowingly). This deletion is what
raises the cost of un-doing the erosion casually and anchors the
session-level copies as the only carriers of the protocol rule.

### Delay/pace setter (`sess.go`): inlined configuration

The setter stops delegating to the engine's tuning method and assigns the
engine's pacing fields directly: argument checks, interval clamping,
retransmission-floor selection by nodelay profile, fast-resend and
no-congestion stores — all spelled out in the session under its own mutex.

- **Why this site:** the setter is the natural first target of a
  "remove needless indirection" pass: a public method whose body is a single
  delegation looks redundant to anyone profiling configuration.
- **Why this shape:** here the owner-side method is kept (it is part of the
  engine's documented surface and referenced from protocol docs), so this
  location intentionally differs from the datapaths: an inlined twin with
  the original still present, which is how configuration inlines usually
  look in real repositories.
- **Production role:** protocol configuration semantics (bounds, profiles)
  are now defined at the session layer; the engine keeps an unreachable
  duplicate.

### Outgoing FEC stage (`sess.go`): parity helper

The pipeline stage no longer calls the encoder's encode; a session method
seals the data shard into the encoder's cache, tracks quorum and maximum
size, applies the continuity-window decision around the latest-packet
timestamp, runs the codec over the shard caches, seals and truncates parity
shards, and handles skip/error paths — chaining through the encoder's fields
throughout.

- **Why this site:** the outgoing pipeline is the newest, most-team-touched
  subsystem (parities, latency windows, sequence numbering), i.e., exactly
  where a "make the stage self-contained" refactor is pitched.
- **Why this shape:** a copy of the whole encode rule, again with receiver
  spelling adapted; the encoder-side original survives, mirroring the
  setter's shape deliberately: having every location either keep or drop the
  owner original would look artificial, and in real repositories the
  datapath cleanups and the encode-stage copy came from different people in
  different months.
- **Production role:** FEC sequence-numbering and parity-emission policy for
  the entire outgoing stream now hang off session code.

### Decoder tuning branch (`fec.go` → `autotune.go`): extraction inversion

The decoder's decode branch that previously applied a detected parameter
change in place — rewriting shard geometry, recycling pooled shards,
reallocating caches, recomputing the wrap guard, rebuilding the codec,
clearing the flag — becomes a call into the detector, with the decoder passed
in as a parameter.

- **Why this site:** during the auto-detect feature the adaptation block sat
  in the middle of the decoder's hottest input path next to the detector's
  samples; "the detector understands the periods, let it apply them" is the
  extract-method suggestion a reviewer would actually make.
- **Why this shape:** unlike every other location, the extraction moves the
  logic to a *different* struct's method with the data owner arriving as an
  argument, passing through the receiver inversion that real extract-method
  refactors of this kind produce. The detector's file gains the codec
  construction import; the decoder keeps only the branch and its early-out.
- **Production role:** incoming-shard adaptation — when a peer changes
  parameters, when caches drop, when decode silently discards packets — is
  now driven from the detection helper.

## Deliberate structural variation

The locations intentionally do not share one shape, because uniform
duplication does not occur in real erosion history:

- two displacements **delete the owner original** (read path, write path),
  one displacement retains the owner original but stops calling it (the
  pacing setter and the outgoing FEC stage), and one displacement **moves the
  behavior to a third struct** via a foreign-object parameter (the tuning
  branch);
- the receivers differ in how much of their own state they still touch —
  the datapath helpers and the FEC-parity helper touch none of the session's
  own fields, the setter keeps its two lock/unlock operations on its own
  state, and the detector keeps two calls into its own detection;
- the touched components differ in kind (connection wrapper, engine, FEC
  encoder/decoder, tuner) so the evolution spans the whole session/engine/FEC
  interaction zone rather than clustering in one of them.

## Behavior-preservation discipline

Every moved block is the same token stream as its original, modulo the
receiver-spelling rewrite required by its new attachment point (`kcp` →
`s.kcp`, `dec` → `tune`-via-parameter, `enc` → `s.fecEncoder`); argument
validation, clamping bounds, error and early-return values, buffer-pool
recycling, sequence accounting, lock placement at every call site, and the
FEC continuity-window timing are carried verbatim. No public type, method
signature, or wire format is altered by any step of the evolution.
