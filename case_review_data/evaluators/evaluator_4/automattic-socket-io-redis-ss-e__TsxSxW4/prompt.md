# Consolidate Redis client compatibility handling in the adapter library

## Background

`@socket.io/redis-adapter` lets a Socket.IO server farm broadcast events between nodes through Redis, and it must work with several Redis client packages: node-redis v4 and above (standalone and cluster), ioredis (standalone and cluster), and the legacy redis@3 client. It also ships two transports: the classic adapter built on regular Pub/Sub and the sharded adapter built on Redis 7 sharded Pub/Sub.

Supporting all of that requires, on every client:

1. **client flavor detection** — deciding whether the connected client is node-redis v4+ (camelCase methods, per-subscription message handlers, buffer options) or one of the lowercase-command-only clients (ioredis / redis@3), where incoming sharded messages arrive on a generic `smessageBuffer` event and must be demultiplexed through a channel→handler map; and
2. **per-flavor command dispatch** — issuing each sharded subscribe, unsubscribe and publish in the spelling, argument shape and bookkeeping the detected flavor needs.

That compatibility responsibility used to be owned by the small shared helper module of the adapter library: one detection helper plus one subscribe, one unsubscribe and one publish helper that encapsulated the per-flavor spelling, the buffer flag, and the legacy handler-map bookkeeping.

## What we observed in the current code

The shared compatibility layer has been bypassed at every site that needs it, so the responsibility is scattered:

- The sharded adapter's channel subscriptions (main channel, response channel, and dynamically created room channels) each embed their own copy of the client-flavor test plus the matching command branch, including their own copy of the legacy handler-map setup and `smessageBuffer` listener attachment.
- Adapter teardown (on close, and on deletion of a dynamically created room) re-implements the flavor test and the unsubscribe dispatch again, per site, with the handler-map cleanup.
- The sharded broadcast publish and the response publish each re-run the flavor test and branch between the two publish spellings inline.
- The classic adapter's subscription setup and teardown no longer consult the shared detection helper either; they test the client inline at each site, and not even against the same capability as the shared helper does, so the copies have already drifted.
- The shared helpers have consequently lost nearly all their callers: the detection helper is now consulted only by the server-count polling path. The subscribe/unsubscribe/publish helpers are gone entirely, their bodies duplicated into each call site; what remains exported of the legacy mechanism is the raw handler-map symbol, which the adapter now manipulates directly.

The practical effect: adding support for another client package — or changing how a flavor is detected and dispatched, for example when a new node-redis major renames methods — now requires hunting down every scattered copy across both adapter classes and editing them all in agreement. The copies already differ in small ways (some set up the listener registry, some clean it up, some promise-shape differently), so keeping them consistent by hand is fragile.

## What to do

Restore the compatibility concern to a single shared owner in the adapter library, and route every affected site through it:

- There must be exactly one place in the library that answers "which Redis client flavor is this", and every flavor-dependent site must ask it rather than testing capabilities inline.
- There must be exactly one place per sharded command (subscribe, unsubscribe, publish) that knows how to issue that command on each flavor — command spelling, buffer/handler arguments, and the legacy handler-map setup, dispatch and cleanup — and every subscription, teardown and publish site must call it instead of carrying a local copy.
- The classic adapter's subscription setup and teardown flavor gate should consult the same shared detection as everything else.
- Remove the local copies, the direct manipulation of the legacy handler-map symbol from the adapters, and any now-dead fragments the duplication introduced. Keep the server-count path working exactly as it is.

After the change, a conceptual change to Redis client compatibility (detection or dispatch) must be one edit in one place, and the shared compatibility module should again be the only code that talks to client-flavor specifics.

## Acceptance boundary (must hold after your change)

Behavior must be fully preserved — this is a restructuring, not a redesign:

- All observable adapter behavior stays identical: channel naming (namespace channel, per-server response channel, dynamic room channels, per-requester response channels), wire formats and the msgpack/JSON encoding decision, the three subscription modes (`static`, `dynamic`, `dynamic-private`) with their per-mode channel sets and room-channel subscription rules, server counting, and request/response behavior with the documented options and their defaults.
- Every supported client package and deployment combination keeps working exactly as before: node-redis v4+ standalone and cluster (the per-subscription message-handler path with buffers), ioredis standalone and cluster, and redis@3 — including the legacy clients' `smessageBuffer` demultiplexing, with the channel→handler map attached at most once per client and entries managed on subscribe/unsubscribe.
- The complete test suite must pass in the end.

## Out of scope

Do not redesign the socket.io-adapter integration, the message encoding, the request/response correlation protocol, the broadcast pipeline, or the cluster node coordination in server counting. Do not change public API or options. Do not modify tests.
