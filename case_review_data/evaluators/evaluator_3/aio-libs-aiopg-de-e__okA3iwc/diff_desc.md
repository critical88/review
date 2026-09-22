# Injection design record — aiopg dormant compatibility-path cleanup

## Maintenance motivation

aiopg's 1.x releases consolidated several behaviors: event-loop handling
became running-loop-only, LISTEN/NOTIFY delivery moved onto the
connection's notification queue, the blocking protocols (query cancel,
bulk copy) became unconditional errors in the async-only API, and the
SQLAlchemy integration layer followed SQLAlchemy 1.4 conventions for
execution parameters, server-side defaults and row access. Each of those
transitions shipped behind a small module-level switch so deployments
pinned to the older behavior could keep it for one more release — a
standard pattern in this repository's history, where compatibility
concerns arrive as named module constants with a short comment stating
which release flipped or dropped them.

The motivating maintenance situation is the step that historically happens
next and historically gets forgotten: the release where the switches are
pinned off for everyone, the 1.x series moves on, and the removal ticket
that should have landed early is still open two minor versions later. This
injection models the state such a repository ends up in — the disabled
options still wired into live control flow, the helpers and import names
that served them still defined, and a few methods where the old
implementation was left physically in place after the method had already
been converted to fail unconditionally.

## Evolution being modeled

The injected code reads as the residue of four genuine repository
evolutions, each expressed in the idiom the surrounding project already
uses:

1. **The loop consolidation (1.3 → 1.4).** `connect()` and the pool used
   to adopt a caller-supplied event loop; 1.4 made the running loop
   mandatory. The residue: constructors keep an "adopt the explicit loop"
   option behind a disabled switch, and a loop-resolution helper remains
   in the shared plumbing module for the branches that used it.
2. **The LISTEN/NOTIFY routing change (pre-1.4 → queue proxy).**
   Integrations used to register a container-side router; 1.4 delivered
   notifications directly through the connection's queue. The residue: a
   disabled switch, an empty router registry, a router function defined
   at import time, and a dormant routing decision inside the delivery
   loop.
3. **The async-only hardening.** Sync-style methods (`cancel`, the copy
   family) were converted from half-implemented delegation to clean
   `psycopg2.ProgrammingError` rejections; the nested-transaction commit
   changed from parent-walking re-commit to a hard
   `InvalidRequestError`. The residue: the superseded statements stayed
   behind the unconditional raises instead of being deleted with the
   behavior change.
4. **The SQLAlchemy 1.4 alignment.** The dialect compiler went from a
   second round trip for server-side defaults to prefetch merges,
   `execute()` gained parameter distillation, result rows became a
   `Mapping` with processed access, and recycle auditing was offered and
   then retracted. The residue: one disabled switch plus one helper per
   retired behavior, each with the branch that would invoke it.

Every injected region carries the comment style the real project uses at
its existing compatibility boundaries: a short prose note naming the old
behavior, the release that ended it, and that the switch ships disabled.

## Overall design

The change distributes one family of dormant compatibility remnants across
the two layers that the modeled cleanups touched:

- the **driver layer**: `aiopg/connection.py`, `aiopg/pool.py` and the
  shared `aiopg/utils.py` loop plumbing;
- the **SQLAlchemy integration layer**: `aiopg/sa/connection.py`,
  `aiopg/sa/engine.py`, `aiopg/sa/result.py` and `aiopg/sa/transaction.py`.

The manifestations deliberately vary along four axes so that each location
reflects a real shape a retired option can take:

- *shape*: disabled switches reading as module constants, guarded branches
  inside live methods, statements stranded behind unconditional raises,
  private helper functions, a dictionary registry, and import names added
  to otherwise live import blocks;
- *nesting*: module top level (the router region), constructor bodies, an
  `elif` arm inside the pool's release decision, the first statement of a
  dunder accessor, and the body of a delivery `while` loop;
- *size*: one to three statements per dead region;
- *kind of dead support*: names consumed only by disabled branches
  (helpers, aliases), constants whose only readers are their own guards,
  and an import-time registration block with nothing left to register.

No live behavior is intended to change: the branches are guarded by
constants that evaluate false at import, the stranded statements sit
after raises that always fire first, and the helpers are called from
nowhere reachable. Public names, signatures and exports are untouched.

## Per-location rationale

### `aiopg/connection.py` — module head

Added three module-level entities after the `WSAENOTSOCK` errno doc
block: the loop-API switch, the notify-routing switch, and the empty
router registry `_NOTIFY_ROUTERS`. Under the routing switch sits the
import-time registration region: the deque router function and a registry
assignment, guarded by the constant. This is where such switches
naturally live in a Python driver — module scope, immediately after the
module's other constants, evaluated once at import — and placing the
router definition inside the guarded region models how an integration
era defines its adapters next to the interface they register into. The
`_resolve_explicit_loop` name is added to the existing `.utils` import
block because that is how this module already receives its loop helpers.
Production role: the compatibility entry points for the loop and
notification consolidations.

### `aiopg/connection.py` — `Connection.__init__`

After the running-loop assignment, a guarded branch adopting the
caller-supplied loop through the resolver. Constructors are where the
loop policy is decided, so this is the one location both the 1.3 API
(and its residue) and any later cleanup would touch; the branch reuses
the constructor's existing `**kwargs` plumbing rather than changing any
signature. Production role: the dormant half of the loop policy.

### `aiopg/connection.py` — `Connection._ready`

Inside the notification delivery loop, a guarded branch that consults
the registry and routes each payload to the integration's container
instead of the queue proxy, followed by the original queue delivery. The
routing decision has to sit where notifications arrive, so the delivery
loop is the natural site; two statements keep it at the scale of the
surrounding per-payload work. Production role: the dormant half of the
notify delivery policy.

### `aiopg/connection.py` — `Connection.cancel` and `Cursor.copy_expert`

Each method keeps its unconditional `psycopg2.ProgrammingError` and gains
one stranded statement behind it: the pre-cancellation notification drain
close in `cancel`, and the blocking delegation call in `copy_expert`.
These are the two sync-style operations aiopg rejects, and they are the
places where a conversion from delegation to rejection most credibly
leaves the old statement behind — the guard explains what the method used
to do, and the corpse is the very call that had to stop working. The
stranded statement is preceded by a comment naming the dropped behavior,
matching the surrounding code's habit of documenting why an async-only
method refuses. Production role: the retired blocking implementations,
kept as unreachable text.

### `aiopg/pool.py` — module head and `Pool.__init__`

Module head: the pool's own loop switch and the recycle-audit switch,
plus the audit helper — a private function emitting the `ResourceWarning`
operators were offered before the recycle policy was settled, with the
repository's usual `warnings.warn` docstring style. The pool gets its own
switches rather than sharing the connection's because pool construction
is a separate release-visible surface (its `__init__` is documented API
and forwards `**kwargs` to connections) and a real switch rollout keeps
per-surface control. The loop-adoption branch mirrors the connection
constructor's for the same reason it existed in the real API: pools
adopted loops for all their timing work. The `.utils` import block is
rewritten to the multi-line form so the resolver alias sits beside the
other loop helpers it ships with. Production role: pool-side loop and
recycle retention.

### `aiopg/pool.py` — `Pool._fill_free_pool`

In the `elif` arm that fires when a connection has idled past the
recycle interval, a guarded audit call before the close. The recycle
decision is the only place with the timing context the audit message
consumes (`loop.time()`, `last_usage`), which is exactly why the retired
feature was located there originally; keeping it inside the arm rather
than extracting anything models the residue honestly — the cleanup that
never happened would have removed, not refactored. Production role: the
dormant audit half of the recycle decision.

### `aiopg/utils.py` — `_resolve_explicit_loop`

The resolver helper itself: a small private coroutine-free function
validating that an explicit loop was supplied. It lives in the shared
plumbing module next to `create_completed_future` because both the
connection and the pool constructor branches call it — the same
deduplicated helper placement the project uses for loop utilities.
Production role: cross-module support for the dormant loop policy.

### `aiopg/sa/connection.py` — module head and `_distill_params`

Module head: the star-params switch and the star-args forwarding helper
(raising for the empty case), written at module level because the retired
calling convention is a module-level concern of `execute()`. In
`_distill_params`, the final `else` arm — which handles the raw
positional case the distillation replaced — gains the guarded forward
before its live classification. Putting the dormant branch in the same
arm that handles the surviving positional callers models where the
distillation actually changed behavior; a comment documents the pre-1.4
forwarding expectation. Production role: the dormant raw-parameter
execution style.

### `aiopg/sa/engine.py` — module head and `APGCompiler_psycopg2.construct_params`

Module head: the server-defaults switch and the second-pass finder
helper, placed immediately before the dialect compiler class they serve.
The compiler's `construct_params` gains a guarded early return replacing
the built prefetch merge with the helper's per-column second pass. This
is precisely the code SQLAlchemy 1.3 executed on this class, so the
residue reads as the old implementation that the prefetch merge
short-circuited; a comment states which version changed it. Production
role: the dormant second-pass default resolution.

### `aiopg/sa/result.py` — module head and `RowProxy.__getitem__`

Module head: the sequence-access switch, placed after the import block
with the same one-line provenance comment as its siblings. The row
accessor gains a guarded first statement returning raw positions from
the stored row tuple, bypassing processors exactly as pre-mapping access
did. Placing it first models what the retired access mode was: a
precedence rule, not an addendum; one statement keeps the accessor's
liveness at dunder scale. Production role: the dormant raw-positional
row access in an otherwise mapping-processed type.

### `aiopg/sa/transaction.py` — `Transaction.commit`

After the existing inactive-transaction `raise`, the ancestor walk and
recursive re-commit that the pre-0.16 hierarchy performed: three stranded
statements with a comment naming the era and why the walk died. Nested
transaction commit is where the semantic change (inactive child commits
propagated to the active ancestor → hard error) left a body behind, and
the walk is only meaningful as code stranded behind the raise that
displaced it. Production role: the retired re-commit semantics of the
transaction hierarchy.

## Boundary of the modeled work

Everything injected stays inside the two modeled layers and their
private surface: no export, signature, default, or test is modified, no
live statement is reordered, and the only touched import line (pool's
`.utils` block) gains a name rather than changing one. The repo's
documented behavior — loop policy, delivery, rejections, recycling,
compilation, row semantics, transaction errors — is what the regions
describe, never what the regions change.
