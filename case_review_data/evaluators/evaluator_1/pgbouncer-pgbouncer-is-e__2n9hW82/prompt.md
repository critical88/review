# Refactor task: slim PgBouncer's connection-operations interface

## Situation

In this codebase, a `PgSocket` can be one of several connection kinds: a
frontend client connection, a backend server connection to PostgreSQL, or a
server connection opened only to forward a cancellation request to a peered
PgBouncer. Shared machinery — the stream buffer that reads and writes each
socket, and the packet-reassembly machinery that buffers oversized packets —
must act on these sockets without always knowing which kind it is holding.

To support that, this revision of the codebase carries a
connection-operations registry: each connection kind registers a table of
function pointers describing itself, and the shared machinery dispatches
socket events, fully buffered packets, and teardown through it.

## Problem

The connection-operations interface lists every capability any connection kind
could ever have — connection establishment and failure handling, TLS
negotiation, the login-phase and work-phase protocol state machines, the
authentication exchange, login completion, startup-packet pool selection,
cancellation acceptance — instead of the small set of operations the shared
machinery genuinely consumes. Because every registered kind must account for
every member, the three registered implementations are forced to carry
capability slots they do not provide: a kind of connection that can never
perform a capability either leaves the slot empty or points it at trivial
adapter code that exists only so the table has an entry, and one of the
registered kinds carries almost the entire protocol state machine that its
connections never run.

This is an interface-segregation problem: implementors of the connection-ops
interface depend on capabilities irrelevant to them, and small protocol
helpers had to be produced or reshaped just to have something to put in a
table slot.

## Task

Restore a clean boundary between what the shared machinery consumes and what
each connection kind provides:

1. Re-examine which connection-operations members are actually dispatched
   from shared code, and which members are only carried along by registered
   kinds.
2. Redesign the interface so registered kinds provide the operations they
   genuinely support and are not made to declare capability slots that are
   unused or filled with empty placeholder implementations. A kind that
   cannot perform an operation should not have to say so in a shared table;
   operations that are only ever called on one connection kind belong to
   that kind, not to the shared interface.
3. Remove scaffolding that exists only to fill table slots: placeholder or
   wrapper entries with trivial bodies, per-kind declarations that exist only
   to be listed in a table, empty slots, and registration special-cases whose
   only purpose was to select a table that is now unnecessary. Keep small
   helpers that are genuinely dispatched.
4. Keep the kinds that remain genuinely distinct. Do not merge connection
   kinds that behave differently; only remove distinctions that exist solely
   to populate the table.

## Requirements

- **Behavior must be preserved exactly.** Clients keep connecting,
  negotiating TLS, authenticating (including SCRAM/PAM/LDAP where
  configured), exchanging queries, prepared statements, and cancellations
  exactly as before. Server connections keep negotiating SSL, authenticating,
  and being released and lifetime-managed as before. Cancellation requests
  keep being forwarded to peered instances exactly as before. No wire-level
  packet, log message, or state transition may change.
- The generic dispatch that shared machinery performs on a socket must keep
  working for every connection kind that exists after your change; it must
  not regress to per-side `is this a server socket?` branching in shared
  code.
- The full project test suite must continue to pass with the same outcomes
  as before your change.
- The final layout should read like something a PgBouncer maintainer would
  have written: the mechanism a connection kind uses to expose generic
  operations should stay idiomatic for this codebase (a focused table, if a
  table remains, is fine; so is a smaller mechanism if that is all shared
  code needs).
- This is a refactoring task, not new functionality: end state is the
  behavior above with a sound, minimal interface for connection-kind
  operations.
