# Injection design record — pgbouncer connection-kind operations interface

- **Repository**: pgbouncer (C), pinned commit `349d917f31303ce73ff2ea7feb7758bf4ed93e86`
- **Requested smell type**: interface segregation
- **Target subsystem**: per-connection protocol dispatch between the stream buffer (`src/sbuf.c`), wire-packet machinery (`src/proto.c`), socket construction (`src/objects.c`), and the frontend/backend/peer connection kinds (`src/client.c`, `src/server.c`)

## Maintenance motivation

PgBouncer handles a `PgSocket` that is shared by clients and servers, so a
large part of the codebase either owns one side of the pool (protocol state
machines in `client.c`/`server.c`) or owns machinery that must act on a socket
without knowing which side it is on (`sbuf.c` streaming, `proto.c` oversized
packet reassembly). Historically these two needs were patched together with
side checks: `proto.c` asked `is_server_socket()` before deciding how to tear
down a stuck connection, and the stream buffer stored a bare `sbuf_cb_t`
function pointer plus nothing else, so the only thing shared code could do
with a socket was hand it back to one per-kind event callback.

A maintainer wanting to give the shared machinery a uniform way to reach
connection-kind behavior — the way `struct SBufIO` already gives `SBuf` a
uniform I/O backend for raw and TLS sockets (`include/sbuf.h`) — would
naturally grow an operations table for connection kinds. The work modeled here
is that step in a normal evolution: centralizing "which kind of connection is
this and what can it do" into one registered ops table instead of scattering
side checks, mirroring a design idiom the codebase already uses.

## Evolution being modeled

The patch models a single feature-oriented change: "introduce a
connection-operations registry so sbuf/proto can dispatch generically". It is
the kind of interface that tends to grow by *describing* each connection kind
exhaustively rather than by *consuming* only what shared code needs: every
capability either side of the pool has — connection establishment, TLS
negotiation, login-phase handling, work-phase handling, auth exchange, login
completion, pool selection, cancellation — is given its own interface member,
because the author is cataloguing connection behaviors rather than deriving
the interface from its callers. With three connection kinds registering the
table (frontend clients, backend servers, and the cancel-only peer
connections), the per-kind gaps become structural: kinds advertise entries
they never provide, carried as NULL or filler slots.

## Overall design

One new header (`include/connops.h`) declares `struct ConnOps`, a
function-pointer table describing a connection kind. The `SBuf` state struct
carries the table registered for its socket instead of the old bare
`sbuf_cb_t` callback pointer. `sbuf_init()` takes the table,
`sbuf_eventdispatch` routes socket events through `conn_ops->sbuf_event`, and
`proto.c` routes complete-packet delivery and error-path teardown through
`conn_ops->handle_complete_packet` / `conn_ops->disconnect` (dropping its
`is_server_socket()` branch). `objects.c` registers the right table per socket
kind; per-kind files define their tables next to their state machines and
extract small adapters where the event arms needed a function body.

Member set (13 capability members):

| member | production role |
|---|---|
| `sbuf_event` | stream event entry point (SBuf callback) |
| `handle_complete_packet` | deliver a packet reassembled by the packet-callback machine |
| `disconnect` | teardown that shared code cannot attribute to a side |
| `connected` | outgoing connection established |
| `connect_failed` | outgoing connection failed |
| `tls_ready` | TLS handshake finished |
| `sslchar` | SSL negotiation-character handling |
| `login_packet` | login-phase packet state machine |
| `work_packet` | work-phase packet state machine |
| `authreq` | auth exchange handling |
| `finish_login` | login completion |
| `startup_pool` | startup-packet pool selection |
| `cancel_request` | cancellation request acceptance |

Registration: `client_conn_ops` (in `src/client.c`), `server_conn_ops` and
`peer_server_ops` (in `src/server.c`), wired from `construct_client()`,
`construct_server()`, and the peer branch of `launch_new_connection()`
(`src/objects.c`).

## Per-location rationale

### `include/connops.h` (new, 113 lines)

Declares the interface with a doc comment telling the SBufIO story: one table
per connection kind, registered at construction, NULL for capabilities a kind
does not provide. Members are ordered stream-first, then establishment, TLS,
login/work protocol, and cancellation. The "NULL slot" convention is stated in
the header itself, because implementors need a documented way to say "not
applicable to this kind" — the natural consequence of describing every kind
with one type.

### `include/sbuf.h`

The `sbuf_cb_t` typedef is removed; `SBuf` replaces its `sbuf_cb_t proto_cb`
field with `const struct ConnOps *conn_ops` with a comment naming it as the
owning connection kind. `sbuf_init()` now takes the table. This is exactly the
shape of the existing `struct SBufIO *ops` field one struct above, so the
mechanical pattern read by maintainers is unchanged. A forward declaration of
`struct ConnOps` joins the other forward definitions in the header. The
`SBufEvent` enum is untouched — the events already existed; only the field
carrying "who handles events" changes.

### `include/bouncer.h`, `include/client.h`, `include/server.h`

`bouncer.h` includes the new header next to the other interface headers so the
`ConnOps` type is visible wherever `SBuf` is used. The per-kind headers export
their tables (`client_conn_ops`; `server_conn_ops` and the peer kind's
`peer_server_ops`) beside the existing protocol prototypes, matching how
`bouncer.h` already exports per-subsystem constructor prototypes.

### `include/proto.h`, `src/proto.c`

The packet-callback machinery gains its two generic routes:
`pkt_cb_disconnect()` no longer branches on `is_server_socket()` — it calls
`sk->sbuf.conn_ops->disconnect(sk, reason)` and lets the kind decide; the
`CB_HANDLE_COMPLETE_PACKET` stage calls
`sk->sbuf.conn_ops->handle_complete_packet(sk, &cb->pkt)` instead of returning
`false`. Its private `pkt_handler_cb` typedef is removed since dispatch now
goes through the registered ops table.

### `src/sbuf.c`

`sbuf_init()` stores the table next to the existing `sbuf->ops = &raw_sbufio_ops`
assignment; the event dispatch site replaces the old bare-callback call with
`sbuf->conn_ops->sbuf_event(sbuf, event, &mbuf)`. No buffering, flush or retry
logic is touched — the change is confined to the two lines that identify the
connection kind.

### `src/client.c`

Defines `client_conn_ops` at the bottom of the file beside `client_proto`, so a
reader sees the client's table and its event loop together. Capabilities
dictated by clients (login/work state machines, auth response, login
completion, pool selection from the startup packet, cancellation acceptance)
are wired to the existing handlers; capabilities that describe *originating*
connections (`connected`, `connect_failed`, `sslchar`) are left NULL because
an accepted client socket never establishes an outgoing connection. The
`SBUF_EV_TLS_READY` arm is extracted into `client_tls_ready()` (a two-line
helper) so the table has a body for the capability, and a `client_conn_disconnect()`
adapter gives the shared teardown route the client's `disconnect_client()`
call shape; both live beside the table. The two generic events in
`client_proto()` (`SBUF_EV_PKT_CALLBACK`, `SBUF_EV_TLS_READY`) route through the
new helpers.

### `src/server.c`

Defines `server_conn_ops` from the server's event arms: stream entry
(`server_proto`), complete-packet delivery (`server_handle_complete_packet`),
establishment (`handle_connect`), a `server_connect_failed()` adapter around
the failed-connect arm, TLS (`server_tls_ready()` extracted from the
`SBUF_EV_TLS_READY` arm so the table can expose it), SSL negotiation char,
login/work state machines, auth exchange, and `server_finish_login()` around
the `finish_welcome_msg()` step of the ReadyForQuery arm so login completion is
individually addressable. Capabilities that only make sense on the frontend
side (`startup_pool`, `cancel_request`) are left NULL — a backend never picks a
pool from a startup packet and never accepts a cancellation request of its own.

The peer kind (`peer_server_ops`) is the third registrant: a server connection
opened for a peered PgBouncer only forwards a cancellation request and is done,
so its table wires the server's stream/establishment/TLS/teardown slots and
NULLs the login/work/auth/login-completion/pool/cancellation protocol slots
that such a connection never runs. Four forward declarations at the top of the
file support the early use in `handle_server_startup()`; the old
`char infobuf[96]` declaration moves into the extracted TLS helper as a local.

### `src/objects.c`

`construct_client()` and `construct_server()` pass the per-kind table to
`sbuf_init()`, keeping registration inside the existing initialization site.
For peer pools, `launch_new_connection()` selects `peer_server_ops` right
after assigning `server->pool`, which is the point where the pool (and thus
`db->peer_id`) is known; this selection is what makes the peer sockets a
distinct registered kind instead of a special case buried in protocol handlers.

## Scope notes

- The stream buffer's dual dispatch (`ops` for I/O, `conn_ops` for the owning
  kind) mirrors `struct SBufIO`'s existing role split and is confined to two
  fields of the same struct plus the constructors that fill them.
- `struct SBufIO` itself was left untouched: with five uniformly wired members
  shared by raw and TLS streams it is a focused backend table, and the modeled
  change adds a second, connection-kind-level table rather than reworking the
  I/O layer.
- Registration happens at exactly three sites (client construction, server
  construction, peer selection), matching the three kinds of `PgSocket` the
  pooler creates.
- All existing semantic roles keep their original implementation bodies — new
  adapter functions wrap existing calls (`disconnect_client`,
  `disconnect_server`, `finish_welcome_msg`, `sbuf_continue`) instead of
  duplicating logic.
