# Gateway connection layer has become one brain — restructure it

Repository: `xxim-server` at commit `1d00a5d`. Scope: the gateway's
long-connection area under `app/gateway` — websocket and WebRTC
data-channel handling, the gatewayservice request pathways, and the logic
they share.

## Where this comes from

Since we shipped transport-layer encryption (`3369ce1`) and then fixed the
cross-protocol response inconsistency (`b1e0a0b`), every connect-side task
lands in the same place. Last sprint I added one "small" feature to the
verify flow and had to hold three registries, two per-request logics and two
handlers in my head at once. Two of us are effectively the only reviewers
for anything in this area because the context you need to review it is "all
of it". On-call debugging is worse: when a user reports being booted, the
answer might be in the upgrade path, the keepalive path, or the kick path —
and today there is no place to look where one of those concerns lives
alone.

Some concrete pain points we keep hitting (not a complete list):

- The connect/verify/login flows are split between per-request logic types
  and transition callbacks on the registry, so "where does login actually
  happen?" has a two-file answer.
- Keepalive presence records and the process-lifetime sweep timer are owned
  by code that only exists per RPC call.
- Both transports construct connections with different halves of the
  knowledge — one knows the wire protocol, the other knows the session
  policy — so touching connect-time behavior means touching both handler
  layers and the shared management type.
- Server-push writes and administrative kicks each re-implement "look up
  this user's connections, then act on every one of them" against the
  shared registries.

## What we want

Rebalance the connection layer so each concern has a focused owner.
Connection *bookkeeping* is a real job and should stay; what we don't want
is one production owner that also performs transport construction, key
negotiation, verification, identity/login handling, presence tracking, and
fan-out — or that every caller in the gateway has to go through for every
one of those. We expect the result to look like the rest of this
repository: small, single-purpose types with clear seams, requests
entering through thin go-zero shells, handlers keeping transport I/O,
business state guarded in one place per concern.

You are free to choose the decomposition you can defend, including new
types and files, moving or replacing package-level wiring, and changing
internal call signatures. You do not need to preserve the current internal
API between the gateway's own packages.

## What must not change

This is a restructure, not a behavior change. From outside, the server must
behave identically:

- Websocket upgrades, data-channel opens, and HTTP-bridged requests behave
  exactly as today, including close codes, the Safari compression case,
  and the pre-connect gate.
- Every client-visible response — status codes, headers, marshaled bodies —
  is unchanged on every route, today's success and error paths included.
- The business callback surface fires as today: same user-after-online /
  user-after-keepalive / user-after-offline moments, same connection
  contexts, and the keepalive sweep still starts lazily the first time one
  is needed rather than at process start.
- Pool transitions keep today's meaning and ordering: a connection that
  fails verification or authentication does not appear in any pool it
  doesn't appear in today; registration on the websocket path still happens
  where it happens today (it never registers during upgrade, the
  data-channel path registers at channel-open).
- Kick and fan-out write semantics — which connections receive bytes, which
  connections get closed with which code, and which ones are reported as
  succeeded — are exactly today's.
- Logging stays request-scoped where it is request-scoped today; background
  guarding behaves as guardedly as today.

## Verify your work

From the repository root, `go build ./common/...` and the test suite
`go test ./common/utils/` must pass (they do on this commit), and the
gateway code must stay buildable with `go build ./app/gateway/...`. There
are no client-visible configuration or protocol changes to document — that
is the point.
