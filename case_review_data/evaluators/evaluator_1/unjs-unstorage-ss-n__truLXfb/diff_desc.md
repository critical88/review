# Injection design record — per-driver value size limits

## Maintenance motivation

unstorage drivers wrap storages with very different physical ceilings: the
in-memory Map is only bounded by RAM, `localStorage` throws an opaque
`QuotaExceededError` at insertion time, IndexedDB storage grows silently,
Cloudflare KV rejects any value above 25 MiB *remotely* during `put`, reverse
proxies cap request bodies, and filesystem-backed drivers eventually fill the
disk. These are different failure modes, but they share one user question:
"can I tell unstorage not to even accept a write above some size?"

The change models the natural answer a project gives to that question: an
opt-in `maxItemSize` guard on the write path of the drivers where the issue
actually hurts. It is a plausible feature — several storage SDKs ship exactly
this knob — and it is the kind of feature that grows driver-by-driver, because
each report arrives through a different issue, from a user of a specific
driver, and lands with whoever knows that driver best.

## Modeled development evolution

The diff is written as the *second stage* of that rollout rather than its
ideal first stage: several guarded drivers had already shipped once the
pattern became visible, but no design pass followed. Concretely, the modeled
history is:

1. The memory cache gained a guard after a report that one large SSR payload
   evicted the whole hot cache (`memory.ts`).
2. The same knob was then requested for the other cache (`lru-cache.ts`) and
   for the browser stores where the quota is a hard failure
   (`localstorage.ts`, `indexedb.ts`, `capacitor-preferences.ts`).
3. A follow-up added the same knob to network- and disk-backed drivers
   (`http.ts`, `fs-lite.ts`), and to the KV binding whose remote rejection was
   discovered in production (`cloudflare-kv-binding.ts`).

Each author solved the problem with the vocabulary already present in *their*
file: the measurement comes from whatever that driver already touches
(character length, `TextEncoder`, `Buffer.byteLength`, `Blob`), the reaction
is decided per store (caches skip silently, committed stores throw), and each
options interface gets its own hand-written doc comment. The resulting drift —
four measurement forms, two reactions, one per-call override, one local
closure — is precisely what survives review when an incremental feature
cross-cuts a driver directory without a shared design.

## Overall design

The feature is opt-in: nothing changes for users who do not pass
`maxItemSize`. Every guarded write path belongs to a driver's `setItem` /
`setItemRaw` implementation, and the guard sits at the top of the write, before
any store-specific work (no eviction, no network call, no disk write happens
for a value above the limit).

The eight owners were selected so that each plays a distinct production role in
the same write-policy decision:

| Owner | Production role | Measurement form | Reaction |
| --- | --- | --- | --- |
| `src/drivers/memory.ts` | default in-process cache | `value.length` (chars) | silent skip |
| `src/drivers/lru-cache.ts` | bounded in-process cache | local `byteLength()` helper | silent skip |
| `src/drivers/localstorage.ts` | browser string store (quota-limited) | `value.length` (chars) | throw |
| `src/drivers/indexedb.ts` | browser structured store | `TextEncoder().encode().length` (bytes) | throw |
| `src/drivers/capacitor-preferences.ts` | native key/value store | `value.length` (chars) | throw |
| `src/drivers/http.ts` | HTTP PUT storage | `new Blob([value]).size` (bytes) | throw |
| `src/drivers/fs-lite.ts` | filesystem storage | `Buffer.byteLength()` (bytes) | throw |
| `src/drivers/cloudflare-kv-binding.ts` | edge KV binding | `value.length` (chars) | throw, per-call override |

The cache-vs-store split in the reaction column is deliberate and realistic:
failing hard on a value that a mount owner never asked to persist would break
caching-as-a-side-effect usage, while a store whose user explicitly committed
a write should hear about the refusal instead of losing data quietly.

## Per-cluster rationale

### `memory.ts` — fastest cache, first guard

The guard is placed in both writers (`setItem` and `setItemRaw`) before the
existing timer/expiry bookkeeping, so an oversized value can never re-arm a TTL
timer for a key that is not actually stored. The silent-skip reaction reflects
that the memory driver is a cache: the value is simply not cached, reads fall
through to the mount layer. Because this driver previously took no options at
all, the author had to introduce an options interface locally
(`MemoryDriverOptions`) rather than reach for a shared type — the change keeps
`memory()` callable with no arguments, only widening the signature
(`DriverFactory<void, ...>` becomes an inline options-taking signature,
requiring the `Driver` type import).

### `lru-cache.ts` — bounded cache with a pre-existing size helper

This file already computes item sizes for the underlying library's own
`sizeCalculation` via a local `byteLength()` helper, so the guard reuses that
natural local measurement instead of introducing a new one — the same choice a
driver-local author makes in practice. The guard rejects before
`(await getCache()).set(...)` so the write never enters the cache (and never
displaces an entry). The driver's pre-existing library options (`maxSize`,
`maxEntrySize`, `sizeCalculation` pass-through) remain untouched; the new
option is documented separately in `LRUDriverOptions`.

### `localstorage.ts` — browser quota, actionable error

Here the guard exists to convert a hostile platform error into an actionable
one: writes that would exceed the per-driver limit fail with a message naming
the key, the measured length, and the configured limit, instead of the
`QuotaExceededError` the browser throws later. This driver exposes a single
string write path (`setItem`; there is no raw variant in this file), so there
is one guard site. The doc comment frames the limit as pre-quota protection
("before hitting the browser storage quota"), the vocabulary that matches this
file.

### `indexedb.ts` — structured store, byte estimates

IndexedDB accepts structured data, so the author estimated the stored payload
in UTF-8 bytes via a local `const size = new TextEncoder().encode(value)
.length` line. Both write paths (`setItem`, `setItemRaw`) carry the guard
before the idb-keyval `lib.set` call, and both throw a driver-annotated error
carrying the computed size. The `size` local is the file's first byte-accurate
measurement — a different unit than `memory.ts` and `localstorage.ts` measure
the same conceptual quantity with.

### `capacitor-preferences.ts` — closure form, shared by two callers

The native store rejects oversize entries late and with a platform-specific
error, so the author added a local `checkLength` closure inside the factory
and called it from both write paths. The share-through-a-closure shape is what
a contributor normally reaches for when two methods need the same check but a
shared utility module feels like overkill for one file; it keeps the check
private to the driver while giving both of its callers the same behavior. The
bounds check is written in early-return form (`value.length <= opts?.maxItemSize`
returns), the inversion natural to a validate-then-throw helper.

### `http.ts` — request budget measured as a request body

For the HTTP driver the limit is a request-body budget, so the guard measures
`const bodySize = new Blob([value]).size` — the browser-accurate size of what
will be sent — and rejects before issuing the `fetch`. Both `setItem` and
`setItemRaw` (one plain, one octet-stream) guard, with slightly different
error wording ("Payload" vs "Raw payload"). The check fails fast before a
cross-network `PUT`, which is the whole point on this driver: rejecting
locally is cheaper than a proxy timeout.

### `fs-lite.ts` — disk writes measured via Node

The Node-flavored measurement of the same policy: `Buffer.byteLength(value)`
against the configured maximum, in both file-writing paths, throwing before
`writeFile(...)` so nothing half-lands on disk. The guard sits after the
existing `readOnly` early-return and follows the file's current style of
inline option checks. The option doc is file-centric ("larger writes fail
instead of landing on disk").

### `cloudflare-kv-binding.ts` — remote KV ceiling, and a per-call override

Cloudflare KV itself refuses values above 25 MiB at `put` time, remotely and
opaquely; the guard turns that into a local, driver-annotated failure. This is
also the only owner that resolves the limit twice: the author honored both
the driver option and a transaction-level override
(`const maxItemSize = topts?.maxItemSize ?? opts.maxItemSize`), because
`setItem` already threads `topts` into the `put` options for `expirationTtl`
— a plausible per-set override once the author noticed the transaction options
are an open record. Only `setItem` carries the guard because this driver has a
single write method.

## Sites intentionally not touched

The `sessionStorage`-flavored storage is provided by composing the
`localstorage` driver, so it inherits that guard rather than adding a second
one. Drivers whose writes are full-database or network-object operations
(mongo, redis, deno-kv, azure, s3, github, db0, fs, overlay, null) were left
without the knob in this stage of the rollout; their owners would each repeat
an already-covered role, and the modeled issue trail above stops where the
reported drivers end. No driver options were re-exported or re-typed anywhere
else in the package, and no test files or build tooling participate in the
change.
