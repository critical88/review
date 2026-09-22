# Injection design record — storage split preparation in `use-local-storage-state`

This record documents the change introduced into the repository at commit
`799291ab6152fbdca80823bf6b12e87257d98638` (`use-local-storage-state` v20): what a
maintainer was trying to achieve, what development phase the change models, how the
change is laid out, and why each cluster looks the way it does.

## Maintenance motivation

Two long-standing feature requests were on the tracker for this library:

1. **Replaceable storage backends.** The hook is hard-wired to the `localStorage`
   member of `window`. People embedding the library in extensions, workers, React
   Native shells, or tests with custom storage shims keep asking to point the
   persistence at something else. Any operator like `persistDefault`,
   storage-throw recovery, or the `isPersistent` report eventually needs an
   answer to "which storage area is this build talking to", so the plan was to
   isolate the answer in one place.
2. **Shared change notification.** Instances of the hook that observe the same
   key currently coordinate through a module-private callback set. The plan was
   to give that mechanism a home so future integrations (devtools, external
   invalidation, backend-powered sync) can trigger re-reads per key from one
   place.

The modeled development step is the *first mechanical stage* of that
preparation: move the state involved — the resolved storage area and the
in-memory fallback pool, the per-key cached value, and the listener
collection — out of the hook module into dedicated owner modules. The stage is
realistic because this is the stage where the actual refactoring work happens:
a maintainer extracts the holders, re-points the hook at them, keeps the
behavior contract intact, and means to come back for the behavior pass once
the data have a home. The tree recorded here is the moment between the two stages: the data
moved out, the old behavior stayed put.

## Public-compatibility constraints that shaped the change

- The browser storage entry point must stay lazily resolved inside the owner:
  published consumers — including test rigs that install storage doubles
  (`vi.spyOn(window, 'localStorage')`, `Storage.prototype` mocks) after the
  module is imported — rely on the library not touching browser storage at
  import time, and a module-level `localStorage` access would read too early. This is why the shared
  owner stores a *resolver function* in a public function-valued field that
  callers invoke at use time instead of resolving a `Storage` object once.
- `inMemoryData` must remain a module-level `Map<string, unknown>` exported
  from the hook module — callers clear the in-memory pool through that name.
  After the move it is the shared owner's fallback pool, re-exported from the
  hook to keep the import path stable.
- First-read default persistence, storage-throw fallback seeding, per-key
  serializers (JSON-like with the `'undefined'` case), key-change semantics,
  cross-tab `storage` filtering (`e.key`, `e.storageArea === goodTry(() =>
  localStorage)`), the `storageSync` option, and `getServerSnapshot` behavior
  are all part of the library's published contract; the change carries them
  over untouched in semantics, only repointing *who performs them*.

## Overall design

Four production files participate.

- **`src/PersistentStore.ts` (new)** — the shared storage owner: the resolved
  storage-area resolver, the in-memory fallback pool, and the default
  serializer functions, all as public instance fields. One exported singleton
  instance binds the whole library to one storage area. It also carries the
  first extracted operation, `persistDefault`, which performs the
  persist-first-read-default step on behalf of a per-key entry.
- **`src/StorageItem.ts` (new)** — the per-key keeper: the key, its
  house-keeping pair of cached raw string and cached parsed value, the
  effective parse/stringify functions, and the default value provided by the
  caller. One extra `readStored()` operation wraps the guarded raw read.
- **`src/SyncRegistry.ts` (new)** — the listener home: a single public
  listener set, exported as a singleton. It holds no behavior yet — that was
  the honest state of the mechanical step; the notification loop and per-key
  filtering still live in the hook module.
- **`src/useLocalStorageState.ts` (rewired)** — the React binding keeps doing
  every piece of the coordination work it always did, now against the three
  new owners: building a per-key entry in a ref, resolving the default
  serializer functions from the shared owner's fields, keeping the
  snapshot/cache bookkeeping in its `getSnapshot` callback, registering
  listeners in its `subscribe` callback, driving write-through and fallback
  switching in its setter, storage-area teardown in `removeItem`, post-write
  notification through a module dispatcher, and the `isPersistent` probe in
  the memoized tuple.

The design intent of the tree — as opposed to a finished design — is a
half-finished extraction: the owners were created as plain data holders, the
hook was re-based onto them, and no behavior was hoisted into the owners
beyond the two operations the extraction could not avoid pulling (`readStored`,
`persistDefault`). Everything else is exactly where it was before the split —
just expressed against foreign data.

## Per-cluster rationale

### The shared store module

**What it holds.** An `areaResolver` function-valued field defaulting to
`() => localStorage` (kept as a resolver — never a `Storage` value — because of
the late-install-by-tests constraint above), a `fallback: Map<string, unknown>`
pool, and two default serializer fields, `parseDefault` (which understands the
`'undefined'` literal the library writes for `undefined`) and
`stringifyDefault`.

**Why plain public fields.** This is deliberate: in the modeled phase the
maintainer extracted what the *data* are, and left the re-distribution of
*behavior* for the next pass. Public fields are the honest expression of a
holder object that does not yet own operations.

**`persistDefault`.** First-read default persistence was the first operation
to move, because both the snapshot callback and potential future entry-point
code would otherwise re-implement it. The moved operation is the old hook's
guarded write block, carried over almost verbatim — and this is visible in its
shape: it still works in terms of the *entry's* fields. It serializes with a
passed-in stringify, writes under the entry's key, then copies the written
string and the default value into the entry's cache fields to keep them
consistent. That reach-from-the-store-into-the-entry pattern is the natural
byproduct of relocating a block of coordinator code into one class without
redesigning the data flow: the store ends up keeping the entry's caches
current, i.e. doing the entry's job.

### The per-key entry module

**What it holds.** Per-key slice of state: `key`, the cached raw `string`,
the cached `parsed` value, `defaultValue`, and the effective `parse`/
`stringify` pair the hook resolved at creation.

**Why the key is a public mutable field.** The hook keeps the entry in a `useRef`,
so the first-render entry object survives key changes; the modeled maintainer
made `key` re-assignable from the render path rather than rebuilding the ref
value or threading the key through each operation — the smallest change that
kept key-change behavior working on the rerender path, and exactly the kind
of shortcut a realistic first pass leaves behind. `readStored()` is the one behavior on this
class: the guarded raw read plus the fallback-seeding-on-throw step (seed the
shared pool with the default when the read throws), which the snapshot
callback needs on every poll. Its body speaks the owner's language only half:
it resolves the storage area through the shared store's resolver *field* and
seeds the *shared* pool itself, because the extraction copied the read block
as-is and repointed its references.

### The sync registry module

The old module-private callback set became a dedicated owner with one public
listener collection and no operations. **Why an owner at all:** the planned
integrations (external invalidation, devtools, backend sync) need a stable
place to plug in, and the set had to move before its dispatch behavior could.
**Why it is empty of behavior:** the notification loop
(`triggerCallbacks` — fan out to listeners whose key matches) stayed behind in
the hook module next to the `storage`-event path that feeds it, so the hook's
subscribe callback registers itself against the set directly and the module
dispatcher iterates the set on each write. A registry that stores callbacks
without knowing how to serve them is the canonical mid-refactor shape.

### The hook module

The hook keeps every coordinating behavior, re-anchored to the new holders:

- **`inMemoryData` re-export.** One line: alias the shared pool out of the
  store so downstream imports keep resolving the pool they clear.
- **Serializer resolution in the render path.** The effective
  `parse`/`stringify` are picked per instance by reading the store's two
  default function fields, because that is where the defaults now live; the
  hook keeps the choosing logic.
- **The per-key ref plus the key write-back.** The render constructs the entry
  once in a ref and then re-assigns its key every render (the `useRef` value
  survives; the entry's key must not).
- **`subscribe`.** The registration callback adds the change handler to the
  registry's listener set and returns a remover that deletes it. Nothing asks
  the registry to subscribe anything.
- **`getSnapshot`.** Unchanged in spirit from before the split — it must
  return a stable instance per unchanged snapshot for `useSyncExternalStore`,
  so the cached parsed value stays in a mutable holder and the snapshot
  callback refreshes it: raw read through the entry, fallback-pool probe when
  the read path reports the pool, parse-or-default reconciliation on change,
  copy-through of the raw string, default persistence on first read, and the
  updated value back. Every one of those steps reaches across the owner
  boundary to do so.
- **`setState`.** Functional or direct new value computed by the caller's rule
  from the entry's cached value, one guarded write through the store's
  resolver with the resolved stringify, a switch on failure into the pool, a
  delete-from-pool on success, and a dispatch through the module dispatcher.
- **`removeItem`.** A guarded removal through the store's resolver plus a pool
  delete, then dispatch.
- **The synchronization effect and dispatcher.** The `storage`-event handler
  keeps filtering on `e.key` and `e.storageArea === goodTry(() => localStorage)`
  (guaranteed not to trip lazy-resolution constraints) and re-notifies via the
  module dispatcher; the dispatcher fans out over the registry's listener set.
- **The memoized tuple.** `isPersistent` is reported by probing the shared
  pool for the key.

## Structural variation in the change surface

The surface deliberately varies along several axes so that no single shape is
the whole story:

- **Reach kinds:** value reads, cache writes, function-valued field
  invocations, and collection insert/remove/probe — the primitives of the
  storage behavior.
- **Frame kinds:** hook scope (render path), `useCallback`/`useMemo`
  callbacks, inline guarded lambdas (the `goodTry` blocks), module-level
  function, and class methods in two different classes.
- **Direction:** hook→store, hook→entry, hook→registry, entry→store, and
  store→entry, so the misplaced-work pattern is not a one-way street.
- **Object scopes:** two module-level exported singletons and one
  per-instance object living in a ref.

## Production role

The change keeps the library fully usable: the public API of the package is
untouched, the library behaves exactly as its published contract promises,
and the new modules give the
two planned features a place to plug in. The remaining work — redistributing
the behavior among the owners — is what a reviewer would flag as the next
task: with the data now living in three owner classes, the interplay the hook
and the two extracted operations still implement by hand has a natural home
in each owner.
