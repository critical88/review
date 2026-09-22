# Injection design record — `git_classes` case `firebaseextended-reactfire-gc-e`

Repository: FirebaseExtended/reactfire @ `ae18abc6b55dbe08905d6748c33a63c792ebd3cf` (TypeScript React hooks library for Firebase).
Requested smell type: **god classes** (`class that centralizes too much functionality, violating single responsibility and becoming hard to maintain`).

## Maintenance motivation and modeled evolution

ReactFire is an almost entirely functional hooks library. Its deliberate design rule is stated in the source comments themselves: the hooks are side-effect-free, so what little cross-component runtime state exists has to live in `globalThis` bags, and each module keeps its own small private helper next to the hooks that use it. Before this change, that produced three tiny module-scoped registries with near-identical shapes:

- `useObservable.ts` kept the preloaded-observable cache (a `Map` mirrored onto `globalThis._reactFirePreloadedObservables`) plus the `preloadObservable` entry point and the `DEFAULT_TIMEOUT` policy constant;
- `database.tsx` kept a `DatabaseQuery` cache (an array mirrored onto `globalThis._reactFireDatabaseCachedQueries`) plus `getUniqueIdForDatabaseQuery`;
- `firestore.tsx` kept the same pattern for Firestore queries plus its doc-id helper and the public `preloadFirestoreDoc` warm-up entry point.

Alongside those, `index.ts` owned three tiny shared `ReactFireOptions` field checks (`checkOptions`, `checkinitialData`, `checkIdField`) used by the database and firestore hooks, and `auth.tsx` owned a local claims-validator factory used inside the `useSigninCheck` observable pipeline.

The evolution modeled here is the classic "centralize the runtime state" refactor that later grows into a god class: a maintainer tired of three scattered `globalThis` bags and assorted shared helpers decides the one stateful class the library already has — `SuspenseSubject`, the per-observable caching subject behind `useObservable` — should become "the runtime object", so every shared cache and cross-module helper migrates onto it as class-level state and class-level helpers. The registry migration looks harmless on its own, then the query caches follow because "they're caches too", then the option checks follow because "the hooks get them from the same place now", and finally an auth change ("reuse the claims validator from the preload path") parks the last unrelated helper there as well, since by then the class is already the recognized dumping ground for shared things. Each individual step is a defensible-looking maintenance decision; the sum is a class that owns six unrelated concerns and forces every subsystem edit through one file.

No behavior changes are part of this story. Every moved piece keeps its exact observable semantics, observable-id formats, `globalThis` keys, and public API shape; only the *ownership* of the logic changes.

## Overall injection design

`SuspenseSubject` grows from a 138-line, 20-member, single-responsibility caching subject into a ~330-line, 34-member type whose member set decomposes into **seven disconnected cohesion components** (member groups that share no state through any code path): two instance-level components that are the class's original subject behavior, and five class-level (static) components that are responsibilities absorbed from five other modules. The absorbed logic was moved verbatim in semantics: initialization still mirrors the same `globalThis` keys at module-load time, lookups still create-or-reuse the same shared state, ids are byte-identical, and public exports keep their names and signatures. The original modules were left in two deliberate shapes (see "structural variation"): thin compatible re-exports where the moved symbol is public API, and fully removed helpers with their hook callers rewired to call the class directly where the helper was module-private.

Changed production files (6): `src/SuspenseSubject.ts` (the absorbing class), `src/useObservable.ts`, `src/database.tsx`, `src/firestore.tsx`, `src/index.ts`, `src/auth.tsx` (absorption sources).

One note on the shape of the improvement this state calls for: the size of the pile is not the problem. Gathering the absorbed logic out of the class into one new shared module would still leave five concerns that change for different reasons — database query identity, firestore query identity, shared options validation, auth claims, generic observable caching — owned by a single generic unit, and every service ticket would still route through one file. The meaningful work is per cluster: deciding where each responsibility naturally belongs (the observables layer for the generic registry, the service modules for their own identity resolution and preload entry points, the shared options surface for the field checks, the auth code for claims), and rewiring its consumers to that owner so the old central point stops being the access point. The interlock in the cache-preload cluster (the doc preload consults both the registry and the doc-id helper) is deliberately placed where a split must decide which side composes which.

## Per-cluster rationale

### 1. `src/SuspenseSubject.ts` — the god class that receives everything

**What changed.** The class gained a `static DEFAULT_TIMEOUT` constant, two `private static` registry fields (`_preloadedObservables`, plus `_databaseQueries` and `_firestoreQueries` for the two services), and ten public/class-level helper methods: `preloadObservable`, `preloadUser`, `preloadFirestoreDoc`, `getDocObservableId`, `getDatabaseQueryId`, `getFirestoreQueryId`, `checkOptions`, `checkinitialData`, `checkIdField`, `getClaimsObjectValidator`. Its imports now span rxfire auth/firestore, `firebase/auth`, `firebase/database`, `firebase/firestore`, the `ReactFireError` value, the auth `ClaimsValidator` type, and the `ReactFireGlobals`/`ReactFireOptions` types — i.e. the class compilation unit now depends on every subsystem it coordinates. A long doc comment presents the class as "ReactFire's runtime state object", which is exactly the self-justifying narrative that grows around such a class.

**Why this location and shape.** `SuspenseSubject` is the only class in the library with real mutable state and the only class the whole cache flow already funnels through (`useObservable` → `preloadObservable` → `new SuspenseSubject`), so it is the one place a "consolidate the runtime" change would naturally converge on, and the one where the consolidation looks most like coordination rather than accident. Absorbing as *static* members models the realistic mechanism: shared registries and helpers cannot become per-observable instance state without changing behavior, so maintainers pile them onto the class side instead. The verbatim-move-in-semantics approach is what makes the resulting state a plausible refactor rather than a rewrite: every lookup keeps its create-or-reuse identity, and the field initializers reproduce the original module-load-time `globalThis` mirroring exactly, so all cache-identity behavior (sharing one `Map`/array between preload and hook consumers) survives.

**Production role.** The class performs: (a) per-observable caching subject (value/status/timeout/subscription — unchanged); (b) global preload registry + library-wide preload entry points; (c) two per-service query-id caches; (d) shared option-field validation; (e) auth claims validation. Roles (b)–(e) each change for a different reason and share no state with (a) or with each other.

### 2. `src/useObservable.ts` — registry source becomes a re-export

**What changed.** The module-level `DEFAULT_TIMEOUT`, the `preloadedObservables` mirroring block, and the body of `preloadObservable` were removed; `preloadObservable` remains exported as a one-line forwarder into the class, and the `useObservable` hook itself now registers observables by calling the class directly. The module-to-module import shrank accordingly.

**Why this location and shape.** This module is the registry's original owner and the hub of any consolidation work, so acting here first is what the modeled evolution would do. A forwarder had to stay because `preloadObservable` is public API (re-exported through the package index and consumed as the documented writable warm-up path — the auth tests drive preloading through this import), so a realistic refactor keeps the symbol and moves its body. Rewiring `useObservable` itself to call the class directly models the "new code goes straight to the runtime object" style that follows once the class is established.

**Production role.** The observable-consumption half of the cache contract: the hook must resolve and re-use the exact same cached subjects that preloading produces, and must keep its `ObservableStatus`/suspense/initialData semantics unchanged.

### 3. `src/database.tsx` — full absorption with no residue

**What changed.** The module-level `cachedQueries` mirroring block and the private `getUniqueIdForDatabaseQuery` helper were deleted; `useDatabaseList` and `useDatabaseListData` now build their observable ids from the class-level query-id helper. The `ReactFireGlobals` import became unnecessary and was removed.

**Why this location and shape.** `getUniqueIdForDatabaseQuery` was module-private, so there is no public surface to preserve and nothing left behind: this is the "just move it and rewire callers" shape. It demonstrates the dependency inversion from the other direction — a data-access module that used to own its cache now reaches into the runtime class for a service-specific concern.

**Production role.** Realtime Database list hooks: stable per-query ids (`database:list:<n>`, `database:listVal:<n>:idField=...`) that keep the observable cache collision-free across renders and query objects.

### 4. `src/firestore.tsx` — deeper absorption across three distinct concerns

**What changed.** Three separable things moved at once: the module query cache + `getUniqueIdForFirestoreQuery` (used by both collection hooks), the private `getDocObservableId` template helper, and the body of the public `preloadFirestoreDoc`. As with database, the query-cache helper was private and its hook callers (`useFirestoreCollection`, `useFirestoreCollectionData`) were rewired to the class; the doc-id helper was private and its caller `useFirestoreDoc` was rewired; the public `preloadFirestoreDoc` remains exported as an async forwarder because it is documented public API.

**Why this location and shape.** This module carries the widest absorption surface, and the public/private split forced exactly the deliberate variation a real refactor shows: private helpers vanish and hooks go through the class, public warm-up API keeps its name and clings to the class by a forwarder. Rendering the doc preload *on the class* (rather than leaving it composed in the module) also cements the interlock — the class-level doc-preload helper now consults other class-level helpers, so the absorbed clusters reference each other and cannot each be understood, or relocated, in isolation.

**Production role.** Firestore document and collection data paths: doc observable ids (`firestore:doc:<app>:<path>`), collection ids from the query cache, snapshot-unwrapping hooks, and the doc preloading story that `useFirestoreDoc`'s docs advertise.

### 5. `src/index.ts` — option checks become pass-through re-exports

**What changed.** The bodies of `checkOptions`, `checkinitialData`, `checkIdField` moved onto the class; the index keeps exporting all three as one-line delegating functions pointing into it. No import changes were needed — the index already imports the class for its `ReactFireGlobals` typing.

**Why this location and shape.** These checks are the published surface of the options-object contract (documented, imported by both service modules), so a realistic consolidation preserves the exported names and turns them into pass-throughs. Keeping the *entire delegation chain* in the re-export module — rather than updating `database.tsx`/`firestore.tsx` to call the class instead — models the note "topology preservation": the service hooks keep importing `checkIdField` from the index exactly as before, which is what makes the change look API-neutral while silently making every options read depend on the runtime class.

**Production role.** Shared validation of `ReactFireOptions` fields (`idField`, `initialData`, `suspense`) and their documented error behavior for invalid field names.

### 6. `src/auth.tsx` — the unrelated helper that proves the pattern

**What changed.** The last cluster: the private `getClaimsObjectValidator` factory (and its inner validator closure) moved onto the class; `useSigninCheck` now obtains its validator from there mid-pipeline. The public `preloadUser` became a one-line async forwarder into the class-level preload helper the registry cluster received. The now-unused `preloadObservable`/`ReactFireError` imports were dropped.

**Why this location and shape.** Claims validation is the piece with no plausible coupling to observable caching at all, which is the point: by the end of the modeled evolution the class has become "where shared helpers go", and auth parks its validator there because that is where everything else already lives. Absorbing `preloadUser` alongside the registry is realistic for the same reason — preload entry points were consolidated with the registry in cluster 1, and the auth user warm-up is a preload entry point. Keeping `useSigninCheck`'s call to the class mid-observable-pipeline (rather than at the hook's top) leaves the dependency embedded in rxfire operator chains, which is how such calls survive review.

**Production role.** Sign-in state gating: the validator produces the `hasRequiredClaims`/`errors` result (with `ReactFireError('auth/missing-claim', …)` entries) that `useSigninCheck` exposes; `preloadUser` warms the auth-user subject that `useUser` consumes.

## Structural variation (deliberate)

The diff intentionally varies how absorbed logic presents at its former home, because real consolidations do:

- **registry (useObservable):** module state + entry point reduced to a public forwarder, hook rewired to the class;
- **database query cache:** fully removed, callers rewired, no residue;
- **firestore:** mixed — two private helpers fully removed with rewired hooks, one public helper retained as a forwarder;
- **options checks (index):** all three retained as public pass-throughs, import topology of consumers untouched;
- **auth:** validator fully removed with the pipeline call rewired; public preload helper retained as a forwarder.

## Scope decisions and stopping point

Explored and excluded as scope: the `firebase-sdk:` init-id logic in `sdk.tsx` and the inline observable-id templates in `functions.tsx`, `storage.tsx`, and `remote-config.tsx` (they are template strings, not shared state — repeating them would have restated the already-covered cache-key role without widening the structural relation); `performance.tsx` (`SuspenseWithPerf` has no interaction with the caches or the class); `firebaseApp.tsx` (app/provider context is a genuinely different concern with no shared state to absorb); the small `ReactFireError` type in `index.ts` (absorbing it would duplicate the type's role without any further responsibility and would read as padding). Both deprecated components in `auth.tsx` (`AuthCheck`, `ClaimsCheck`) were left untouched since they are compatibility surface, not shared-state owners. Saturation was reached when every module that owns shared mutable runtime state or shared cross-module helpers in this library — and every hook that consumes them — had been included; the remaining candidates would have either restated an absorbed role or moved code that has no ownership relationship to the caches.
