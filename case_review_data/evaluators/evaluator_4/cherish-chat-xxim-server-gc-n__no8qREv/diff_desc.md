# Injection design record — gateway connection-layer consolidation

## Maintenance motivation

The xxim-server gateway terminates three transports (websocket, WebRTC data
channel, HTTP bridge) behind one universal route table, and its connection
registry (`xConnectionLogic` in the gatewayservice logic package) is the
server's most-touched component in production. Two recent developments in this
repository's history pushed that area into the center of everyday maintenance:

1. **Transport-layer encryption.** The ECDH key-agreement work (commit
   `3369ce1`, "feat: 通讯层加密") made every connection establish a shared
   secret at connect time. Key generation, client-key unmarshaling, and
   shared-secret derivation are *connect-time* concerns, but in the pinned
   tree they live inside a *per-request* logic type (`verifyConnectionLogic`),
   while the registries the verification must update live on the connection
   owner. A single verification flow is therefore split across two types that
   are wired together by an extra transition callback.
2. **Response-consistency fixes across transports.** The commit
   `b1e0a0b` ("fix: 修复http与webrtc通讯协议响应内容不一致") had to be
   propagated through websocket, HTTP and WebRTC paths at once, which showed
   how many files a single behavioral decision in the connect flow touches:
   transport handlers, per-request logics, and the registry owner.

The realistic maintenance story modeled here is the maintainer reaction to
exactly this pressure: *if every connect/verify/login/keepalive/fan-out
decision already has to be coordinated with the connection owner, stop
coordinating and put the whole connection lifecycle on the owner*. The
per-request logic types stay as dispatch stubs, the handlers hand connections
to the owner, and one type becomes the single brain of the connection layer —
the shape an experienced Go maintainer produces when they optimize for "fewer
places to know about a connection" during a busy hardening sprint, without
noticing how much the class has grown until change-requests and on-call
debugging all routable to one file are the norm.

## Modeled development evolution

The injection is written as three plausible engineering increments, each of
which leaves the tree building and behaving like the pinned commit:

1. **Unify transports behind the owner.** The websocket and p2p construction
   code that handlers previously ran inline is replaced with
   owner-provided factory methods, so both transports produce `*Connection`
   values the same way and pull the owner into connection establishment.
2. **Move connect-time security and identity onto the owner.** The ECDH
   session (a stateless `utils.ECDH`), the verification crypto with its
   verified-pool transition, and the whole authentication flow with its
   login transition move in. The per-request logic types shrink to stubs;
   the transient `OnVerified` transition callback folds away.
3. **Move presence and fan-out execution onto the owner.** Keepalive
   bookkeeping with its lazily started sweep timer, the secure fan-out
   writes, and the kick fan-out are delegated wholesale, leaving the RPC
   logics as thin adapters.

Each increment is a decision a maintainer could defend in review in
isolation; their composition is what the case is about.

## Overall design

The owner grows from 7 clean methods to 14 methods, from 3 registry fields to
9 fields spanning three lock-guarded connection pools, a per-user
installation map, an ECDH key-agreement session, a keepalive record map, and
a start-once guard for the keepalive sweeper. The sender of each absorbed
responsibility is rewired at its call site, so the owner is reached from four
layers: transport handlers (websocket upgrade, data-channel open), request
routes (verify/authentication), zrpc service methods (keepalive, writes,
kicks), and a background sweep goroutine. Package initialization gains a
singleton block so all absorbed methods are reachable without plumbing
through `ServiceContext`.

Crucially, the injection is behavior-preserving by construction: every
absorbed body keeps its callback order, pool-transition order, close codes,
response marshaling, request-scoped loggers, lazy timer start, and error
paths byte-for-byte relative to where it came from. What changes is *where
the work happens*, not *what the server does*.

## Location-by-location rationale

### `app/gateway/internal/logic/gatewayservice/x_connection.go` — the hub

**Change.** Eight methods join the owner: `NewWebsocketConnection`,
`NewP2pConnection`, `VerifyConnection`, `AuthenticationConnection`,
`OnKeepAlive`, `checkKeepAliveTimer`, `WriteDataToConnections`,
`KickConnections`. The struct gains the fields those methods need: an `ecdh`
key-agreement session, a `userKeepAliveMap` for keepalive timestamps, a
`keepAliveCheckOnce` start-once guard for the sweeper goroutine. A
package-level singleton makes the extended method set reachable from
handlers and sibling logics. The clean methods (`GetConnectionsByUserIds`,
`GetAllConnections`, `OnConnect`, `OnLogin`, `OnDisconnect`, `loopCheck`)
stay as they were, and the clean `OnVerified` transition callback is replaced
by the transition executed inline inside the absorbed verification flow.

**Why this site and shape.** The owner already held every registry a connect,
login, or disconnect has to touch, so absorbed logic arrives without any new
cross-owner coordination. The ECDH session is stored as an interface field
because `utils.ECDH` session objects are stateless (they wrap a curve), so one
shared instance conveniently serves all connections — the exact kind of
"single owner owns a collaborator" decision centralization encourages. The
keepalive map and start-once guard move in with `OnKeepAlive` because the
sweeper they belong to reads them.

**Production role.** Connection registry and lifecycle owner: who joins which
pool at which moment, who tracks presence, and — after this evolution — who
builds, negotiates, verifies, writes to, and severs every connection.

### `x_connection_ws.go` and `x_connection_p2p.go` — transport adapters

**Change.** Each file becomes a small structural adapter type
(`websocketWrapper` / `p2pWrapper`) implementing the existing
connection-interface `SendMessage`/`CloseConnection` surface for its transport,
plus a field on the `Connection` record pointing at the transport handle.

**Why this site and shape.** Once the owner builds connections for both
transports, the transport-specific `SendMessage`/`CloseConnection`
implementations the handlers previously achieved with interface tricks need a
concrete home per transport. Keeping them as tiny per-transport structs (each
owning exactly its transport handle) preserves the route table's dependency on a
uniform connection record while hiding transport APIs from the owner's
callers.

**Production role.** Transport-bound connection adapters: everything below
the wire (frame send, compression-aware close, data-channel message push)
stays per-transport; everything above it becomes owner policy.

### `verifyConnectionLogic.go` and `authenticationConnectionLogic.go` — collapsed request logic

**Change.** Both per-request logic types shrink to go-zero RPC stubs that
return the request untouched; their bodies (crypto verification for the
former, identity callback and response shaping for the latter) move to the
owner's `VerifyConnection` / `AuthenticationConnection`.

**Why this site and shape.** The go-zero pattern requires the logic
constructors and types to exist for the service routes to compile, so the
plausible evolution keeps the files as stubs rather than deleting them. In
review, "the real logic now lives with the registries it maintains" reads as
a simplification — each request type is one delegation hop instead of a
third place to understand a connect flow.

**Production role.** RPC entry points: request dispatch scaffolding whose
body is now a pass-through, with the actual per-request behavior executed by
the owner.

### `gatewayKeepAliveLogic.go` — delegated presence handling

**Change.** The whole body — timestamp store, the lazy first-time start of
the sweep timer, the user-after-keepalive callback, the first-seen
user-after-online callback — becomes a single call to the owner's
`OnKeepAlive`, with the request-scoped logger retained for the error path.
The sweep timer construction and offline callbacks move to the owner's
`checkKeepAliveTimer` / `loopCheck` family, driven by
`Websocket.OfflineDeterminationSecond` exactly as before.

**Why this site and shape.** A request-scoped logic type is a poor owner for
a process-lifetime timer; consolidating the timer with the keepalive records
it sweeps is the core motivation of this increment. The stub-plus-delegation
shape avoids changing the zrpc wire type or the service signature.

**Production role.** Presence keeper: per-request keepalive intake plus the
background offline sweep that fires user-after-offline and registry cleanup.

### `gatewayWriteDataToWsLogic.go` and `gatewayKickWsLogic.go` — delegated fan-out

**Change.** The write logic delegates to `WriteDataToConnections` (which
resolves target users' connections, sends, and derives the success list for
the response); the kick logic delegates to `KickConnections`. Request-scoped
error logging stays at the RPC boundary with the same messages.

**Why this site and shape.** Fan-out writes and kicks iterate the owner's
registries under its locks; executing them as owner methods keeps registry
locking in one place while the RPC logics keep marshaling and response
shaping. This is the smallest change that removes the duplicated
lookup-then-iterate pattern from two sibling files.

**Production role.** Outbound fan-out execution: server push and
administrative kick across every device a user has connected.

### `connectionhandler.go` — route rewiring

**Change.** The verify and authentication routes hand their whole flow to the
owner: unmarshal errors still shape INVALID_DATA responses locally, but
business success/failure now comes back from one owner call, which also
performs the pool transitions and the user-after-online callback that the
handler previously triggered via two separate transition callbacks on the
registry owner.

**Why this site and shape.** The handler was the only place that both
unmarshals these request bodies and orchestrates logic-then-transition; with
the flow absorbed, keeping a two-step orchestration here would mean
re-deriving the "did login actually happen" question that belongs to the
absorbed logic. Request/response shaping stays put as the handler's single
job.

**Production role.** Universal-route dispatch for connection verification and
authentication: keeps wire shaping; drops lifecycle orchestration.

### `wshandler.go` and `offerhandler.go` — transport handler rewiring

**Change.** Websocket upgrade and the WebRTC data-channel open handler call
the owner's construction methods instead of assembling connections inline
(header collection, compression-mode Safari special case, per-request
decrypt/route loops all stay byte-identical), and the existing
`OnConnect`/`OnDisconnect` invocations that already crossed the registry
owner remain, entering the same owner that now also builds the connection.

**Why this site and shape.** These handlers are the two production entry
points that observe raw transports; handing raw transports to the owner is
the natural seam once construction is centralized. Keeping the transport
I/O (read loops, ICE callbacks, channel registration timing) at the
transport layer but moving *connection construction* to the owner matches
how the repository previously split transport handlers out of monolithic
gateway code.

**Production role.** Transport ingress: websocket HTTP upgrade, WebRTC
answer/data-channel lifecycle, request decryption and routing — now
producing owner-built connections.

## Structural variation across sites

The injection deliberately does *not* reuse one transformation everywhere:

- **Full absorption** at three RPC logics (verify, authentication,
  keepalive): bodies move, files become stubs — because those files' only
  job was the body.
- **Partial delegation** at two siblings (write, kick): a one-line body call
  that keeps request-scoped response assembly and logging at the RPC
  boundary, because those bodies mix registry iteration with response
  derivation.
- **Method addition with state pairing** at the hub: each absorbed cluster
  brings its private fields (ECDH session, keepalive map, start-once
  guard) onto the struct, reflecting how state and behavior accrete
  together.
- **New small types** for the transport adapters, because per-transport I/O
  must remain callable through one interface without leaking transport APIs.
- **Call-site rewiring only** at the three handler files — no logic moved
  there, because the handlers' own behavior was not the contested part.
- **A singleton initializer block** wiring the extended owner into its four
  consumer layers, in the package style this repository already uses.

This mixture mirrors how a real consolidation lands: each site receives the
smallest change that moves its responsibility onto the owner, in the idiom
its file already speaks, rather than a uniform rewrite gesture.
