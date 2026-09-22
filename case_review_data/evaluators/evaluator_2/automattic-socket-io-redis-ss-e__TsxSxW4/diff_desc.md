# Injection design record — Redis client compatibility handling scattered across the adapters

Repository: `Automattic/socket.io-redis` @ `16f7cf938d45c758cbc81e22479b903e082afcea`
Smell under study: `shotgun_surgery` — a change that requires making small modifications in many different places, indicating scattered responsibilities.
Behavior requirement: behavior-preserving. The full test suite must produce identical outcomes before and after injection.

## Maintenance motivation

The package supports five Redis client configurations (node-redis v4+ standalone and cluster, ioredis standalone and cluster, and the legacy redis@3 client) against two underlying transports (regular Pub/Sub and Redis 7 sharded Pub/Sub) — eleven test variants in total. Client-flavor compatibility is therefore a pervasive concern:

1. **Detection** — the package must decide, per client, whether it is talking to node-redis v4+ (whose methods are camelCase and accept options such as buffer flags and message handlers) or to ioredis / redis@3 (which only implement lowercase commands and expose incoming sharded messages through a generic `smessageBuffer` event that the package must demultiplex through a channel→handler map).
2. **Dispatch** — every sharded subscribe, unsubscribe and publish must be issued with the spelling, arguments, and bookkeeping that the detected flavor requires.

When the sharded adapter was introduced, this concern was implemented properly: a compatibility layer in the shared util module (a single `isRedisV4Client` detection helper and single `SSUBSCRIBE`/`SUNSUBSCRIBE`/`SPUBLISH` dispatch helpers covering the registry bookkeeping) did the flavor work, and the adapter body simply called it.

Over subsequent maintenance rounds — wiring the room create/delete events of the base adapter into channel subscriptions, extending `close()` to tear down dynamically created channels, and keeping the standard adapter's setup/teardown gate current with newer node-redis capabilities — the easy path at each site was to re-probe the client and branch inline "just this once". After enough rounds, the compatibility layer is consulted by nothing but the server-count path, and every operation in the adapters carries its own copy of the probe and command branches.

This is a realistic way such scattering arises: no single commit introduces it, each step is locally justified as "simpler than refactoring the helper", and each copy drifts a little from the last.

## Overall design of the injected change

The injection scatters one responsibility — *talk to whatever Redis client package the application wired in* — across the two adapter classes, while keeping all observable behavior byte-for-byte identical.

The scattering is performed by:

- **Inlining the dispatch helpers**: the shared subscribe/unsubscribe/publish compat helpers are deleted from the util module and their bodies (probe + per-flavor command branch + registry handling) are pasted into each adapter site that needs them.
- **Drifting the standard adapter's gate**: the standard `RedisAdapter` re-probes the client inline at its subscription setup and teardown gates, using the sharded-capability member rather than consulting the shared detection helper.
- **Extracting room handlers**: the dynamic room create/delete logic is moved from inline arrow functions in the constructor into dedicated private methods (`onRoomCreated`/`onRoomDeleted`) that carry their own copies of the subscribe/unsubscribe fragments — the way the file would look if room-lifecycle handling had grown and been refactored per-site rather than through the shared layer.
- **Keeping one live consumer**: `PUBSUB` (cluster-aware server counting) still calls `isRedisV4Client`, so the shared detection helper survives as an orphaned-but-alive anchor while everything else drifted away from the compat layer; the exported registry symbol lets the adapter manipulate the legacy handler map directly.

After injection, a conceptual compatibility change (a new node-redis major, another client without the callback API, a change of probed capability) requires coordinated edits at ~nine locations across three files — the shotgun-surgery trigger.

## Changed locations

### Cluster 1 — `lib/util.ts` (compatibility layer dissolved)

**What changed**

- Deleted the `SSUBSCRIBE`, `SUNSUBSCRIBE` and `SPUBLISH` helpers and the `RETURN_BUFFERS` constant (54 lines removed): the subscribe helper had contained the node-redis-vs-lowercase branch plus the legacy `kHandlers` registry setup and `smessageBuffer` listener attachment; the unsubscribe helper, the registry cleanup; the publish helper, the dual-spelling publish and promise shaping.
- Kept `isRedisV4Client` (unchanged body: probes the `sSubscribe` capability) and `PUBSUB`/`hasBinary`/`parseNumSubResponse`/`sumValues`.
- Exported the previously private `kHandlers` symbol so adapter sites can manipulate the legacy channel→handler registry directly.

**Why this cluster is plausible maintenance evolution**

This mirrors what happens when a shared layer stops being the only way: after call sites grow their own local handling, the central helpers become dead code that a maintainer deletes in a cleanup pass ("no callers left"), and the registry symbol gets exported for the direct manipulation the call sites now perform.

**Production role of the cluster**

The compatibility module. After the change it only owns the detection used by server counting (`PUBSUB` → `isRedisV4Client`) and message utilities; it no longer owns subscribe/unsubscribe/publish flavor dispatch.

### Cluster 2 — `lib/index.ts` (`RedisAdapter`, the standard pub/sub adapter)

**What changed**

Both flavor gates — in the constructor (subscription setup) and in `close()` (teardown) — were changed from consulting the shared probe (`typeof this.pubClient.pSubscribe === "function"`) to a locally-duplicated capability probe (`typeof this.pubClient.sSubscribe === "function"`), so the standard adapter no longer shares the detection decision with anything else. Each site keeps its own copy.

**Why this cluster is plausible maintenance evolution**

Keeping the gates "current with what sharded support looks like" by re-probing inline instead of calling the helper is exactly the kind of small, locally-reasonable drift that accumulates when compatibility handling has no single owner.

**Behavior rationale**

For every client configuration the package supports, `sSubscribe` is present exactly when `pSubscribe` is present: node-redis v4+ exposes both (standalone and `Cluster`); ioredis and redis@3 expose neither (lowercase-only API). The probe swap therefore selects the same branch on every tested client.

**Production role of the cluster**

The standard pub/sub adapter used by default: regular Pub/Sub subscribe/publish, request/response protocol, server counting via `NUMSUB`. The changed gates select the node-redis message-listener path (`pmessageBuffer`) on subscription setup, and select the matching unsubscribe/publish spellings at teardown and broadcast.

### Cluster 3 — `lib/sharded-adapter.ts` (`ShardedRedisAdapter`, sharded pub/sub adapter)

**What changed**

- **Constructor**: the main-channel and response-channel subscriptions now each embed a full inlined copy of the old `SSUBSCRIBE` body — capability probe, `sSubscribe(channel, handler, true)` branch, or the idempotent legacy setup (`kHandlers` registry creation, `smessageBuffer` listener attachment, map entry, lowercase `ssubscribe`). The room create/delete arrow handlers registered in dynamic subscription modes are extracted into dedicated private methods.
- **`onRoomCreated`** (new private method): carries its own copy of the subscribe fragment (probe + dispatch + registry setup) for dynamically created room channels.
- **`onRoomDeleted`** (new private method): carries its own copy of the unsubscribe fragment (probe + `sUnsubscribe` branch / lowercase `sunsubscribe` + registry cleanup).
- **`close()`**: the teardown loop over all subscribed channels (main, response, dynamic rooms) inlines the probe and both unsubscribe spellings with registry cleanup per channel.
- **`doPublish` / `doPublishResponse`**: each publish path inlines the probe and branches between `sPublish` and lowercase `spublish` (the old `SPUBLISH` helper body), keeping the original `.then(() => "")` / `.then()` promise shaping.

**Why this cluster is plausible maintenance evolution**

Each site received the minimal per-site edit that made it work with the client at hand: subscribe sites inlined the probe-and-branch "temporarily"; room lifecycle handling was refactored into dedicated methods as it grew, each carrying its fragment; publish paths got the probe inline because the shared helper no longer felt worth the indirection. Every fragment is copied slightly differently (registry setup in subscribe sites, registry cleanup in unsubscribe sites, promise shaping differences between the two publish sites), the way independently-maintained copies actually diverge.

**Behavior rationale**

Every fragment reproduces the semantics of the helper it replaces: `sSubscribe(channel, handler, true)` (buffers requested) on node-redis; on the legacy path the registry is created once per client with the `smessageBuffer` demultiplexer, entries are set on subscribe and deleted on unsubscribe; unsubscribe sites map the channels array imperatively just as the helper did; the publish sites preserve the exact channel construction and promise results returned to the base adapter. The full suite (all 11 client/mode variants) passes identically on clean, injected and repaired states.

**Production role of the cluster**

The sharded pub/sub adapter: channel naming for the namespace and per-room dynamic channels, subscription lifecycle across the three subscription modes, teardown on adapter close, broadcast and response publishing over sharded Pub/Sub, and server counting via `SHARDNUMSUB`.

## Responsibility graph explored

Sites considered while scoping the scattering (all in the production `lib` tree):

- Detection: shared helper `isRedisV4Client`; standard adapter setup/teardown gates; every sharded subscribe/unsubscribe/publish site.
- Dispatch: per-flavor sharded subscribe / unsubscribe / publish; standard pub/sub subscribe/unsubscribe/publish (flavor-agnostic already — lowercase spellings are implemented by every supported client — so there is no per-flavor decision to scatter there).
- Registry bookkeeping: `kHandlers` map setup/attachment/teardown and the `smessageBuffer` demultiplexer.
- Consumers: `PUBSUB` server counting for both adapters (left centralized — it remains the one live consumer of the shared detection helper).
- Non-responsibilities: message encode/decode (0x7b peek / notepack vs JSON), request/response correlation maps and timeouts, namespace/room bookkeeping in the base class, cluster node fan-out in counting.

Included: every production site that must detect the client flavor or issue a sharded command per flavor — 9 functions/methods across 3 files. Excluded: sites whose commands do not vary per flavor (standard pub/sub issuance) and concerns unrelated to client-flavor dispatch. Saturation: no further production site performs either operation; expanding the diff would have to touch behavior, tests, or unrelated responsibilities, which the injection must not do.
