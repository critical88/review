# Injection design record — suspense cache adaptor contract unification

## Maintenance motivation

The next-on-pages runtime templates ship an internal "suspense cache" used to
serve Next.js incremental-cache requests inside the Cloudflare Worker: the
generated `_worker.js` handles an internal request for a cache entry, a store
operation, or the revalidation of tags, backed by whichever storage the
deployment has available — a Workers KV namespace, or the Cache API.

Each storage mechanism lives in its own backend module under
`packages/next-on-pages/templates/cache/`. Historically those backends
inherited from a base class that carried the entry protocol and the tags
manifest machinery, and the backends only genuinely provided raw string
storage (a `retrieve` and an `update`). A maintainer wanting to treat
"anything that can act as the suspense cache" as a single exchangeable value —
for tooling, for tests, for passing the cache straight to the internal request
handler — has a natural temptation: declare one unified TypeScript interface
that covers *every* capability the suspense cache family can have, and have
all adaptors implement it.

The temptation is real because the repository does have one place where a
*composite* object (a full suspense cache) genuinely provides all of those
capabilities at once. Declaring the full contract and pointing every implementor
at it is a believable one-commit cleanup that appears to remove the "we
hardcode concrete adaptors everywhere" complaint.

## Modeled development evolution

This change models that believable maintenance step as a single pass over the
cache subsystem:

1. A unified `SuspenseCacheAdaptor` interface is introduced for the whole family,
   covering three capability clusters that previously had no shared declared
   contract: raw storage (`retrieve`, `update`, `buildCacheKey`), the Next.js
   incremental-cache entry protocol (`get`, `set`, `revalidateTag`), and the
   tags manifest lifecycle (`tagsManifest`, `tagsManifestKey`,
   `tagsManifestPromise`, `loadTagsManifest`, `saveTagsManifest`, `setTags`).
2. The base-class machinery is moved into a `SuspenseCache` class that exists in
   its own right as the reference composite object: it takes a suspended-cache
   adaptor at construction time and serves the entry protocol and the manifest
   lifecycle over it. In the pre-change layout the entry/manifest code lived in
   a base class that storage backends inherited but never meaningfully used;
   giving it an owner that *composes* storage rather than *being* the storage
   is a faithful part of the modeled step, and it keeps every request path
   byte-for-byte equivalent in behavior.
3. Each storage backend is declared to implement the unified interface. The
   parts the backend does not genuinely provide are given placeholder bodies
   that throw a descriptive error, in the same style the package already uses
   for not-implemented primitives ("Method not supported by the KV adaptor -
   ..."). Manifest-related state fields that the interface requires are added to
   the backends even though nothing on a KV/Cache-API deployment reads them.
4. The worker-side cache utilities are re-typed against the unified contract:
   the internal handler receives a `SuspenseCacheAdaptor`, and the helper that
   dynamically imports the built backend modules returns `SuspenseCacheAdaptor`
   instances which are then wrapped in the composite `SuspenseCache`.

The runtime wiring ends up where it must be for behavior preservation: requests
are still served through the composite suspense cache, which still delegates raw
storage to the KV or Cache API backend; backend selection and dynamic imports
are untouched. The change is entirely about the declared contract and the
obligations it imposes — which is why tests, types, and lint stay green
throughout the step.

## Overall design

Four files participate:

- `packages/next-on-pages/templates/cache/adaptor.ts` — the contract, the
  composite cache, the shared types, and helpers;
- `packages/next-on-pages/templates/cache/kv.ts` — the Workers KV backend;
- `packages/next-on-pages/templates/cache/cache-api.ts` — the Cache API backend;
- `packages/next-on-pages/templates/_worker.js/utils/cache.ts` — the internal
  request handler and the backend selection/wiring.

The solvent design decision is to give the unified interface every member of
all three capability clusters, so a backend that can only store strings is
declared to also serve entries and manage a tags manifest. At that point the
interface speaks for three roles at once, and each role's real provider is a
different participant: raw storage is real on the backends, while the entry
protocol and manifest lifecycle are real only on the composite. The
placeholder members and dormant manifest fields exist on the backends purely to
satisfy the declared contract.

## Per-cluster rationale

### `adaptor.ts` — contract and composite (132 insertions, 20 deletions)

The interface declaration sits at the top of the adaptor module because that
file is the subsystem's vocabulary: it already exports the cache constants,
the serialization types of cached fetch values, and the manifest types. The
interface documents each capability cluster in its own block, in the order a
maintainer would think about them (store, serve entries, keep the manifest).

The entry/manifest machinery is relocated verbatim from the previous base
class into the new composite class rather than rewritten, for two reasons:

- behavior preservation is mechanical — the serialized entry shape, the stale
  check against manifest timestamps and the request-scoped revalidated-tag set
  are untouched, and the manifest set stays module-scoped exactly as before;
- the composite is the honest owner of the entry protocol and manifest
  lifecycle in the new design: it is the only participant for which those
  members describe real work, and keeping them in sync with the pre-change
  behavior is easiest while they are still one cohesive unit.

The composite's constructor takes an adaptor value typed by the new interface;
its `retrieve`/`update`/`buildCacheKey` forward to that adaptor, mirroring the
old inheritance relationship through composition, which is the standard way
this kind of base class is consolidated in the modeled evolution.

### `kv.ts` and `cache-api.ts` — storage backends (97/101 insertions, 6/6 deletions)

The two backends previously inherited their entry/manifest obligations; now
they declare conformance to the unified interface directly and must spell out
what conformance means for a backend that only stores strings. The modeled
maintainer keeps it simple and consistent:

- the storage primitives keep their existing implementations, retargeted from
  inherited `buildCacheKey` calls to their own copy of the same method, which
  the interface now requires of every adaptor;
- the entry and manifest members required by the interface become throwing
  placeholders whose messages follow the repository's existing
  not-implemented error style, including the parameters in the message so
  nothing in the files silently stops being referenced;
- the manifest state the interface demands (`tagsManifest`,
  `tagsManifestKey`, `tagsManifestPromise`) is declared on both backends even
  though only the composite ever reads or writes it.

The KV and Cache API backends are selected on different deployment paths
(KV binding present vs absent), so keeping the two modules shaped identically
is what a maintainer would do to keep the comparison fair and the deployment
story simple. The `CacheApiAdaptor` alone keeps its `cacheName` constructor
state, because opening a named cache is part of its genuine storage role.

### `_worker.js/utils/cache.ts` — request serving and wiring (24 insertions, 8 deletions)

The internal request handler keeps serving GET/POST/revalidate exactly as
before; only its types change. The surfaces touched:

- `getSuspenseCacheAdaptor` — the helper the handler calls — now returns a
  value of the unified interface type, and internally wraps the dynamically
  imported backend in the composite `SuspenseCache`, which is the piece that
  actually serves the entry protocol;
- `getInternalCacheAdaptor` — the dynamic-import helper for the built cache
  modules (`__next-on-pages-dist__/cache/{kv,cache-api}.js`) — keeps loading
  and instantiating defaults exactly as before, typed against the unified
  interface.

This cluster exists so the contract change is observable where production
consumes the cache: the internal handler now depends on a contract that
covers everything any adaptor could do, even though the handler only ever
serves entries through the composite. Keeping the instantiation and import
path identical preserves the built-image layout and worker behavior while the
declared obligations widen.

## What this design buys the benchmark

Every participant in the cache subsystem now describes itself in terms of one
contract, and the contract has all three capability clusters; the composite
implements all of them for real, while both storage backends are declared to
implement members they cannot use, backed by throwing placeholders and
manifest fields nothing reads. The backends sit on two deployment-selected
execution paths (KV binding present or absent), and the consumer depends on
the same widened contract. Localization therefore requires distinguishing
"role member genuinely provided" from "role member that merely satisfies the
declared interface", and repair requires drawing role boundaries and re-pointing
realization, wiring, and consumers at them — not editing any single site.
