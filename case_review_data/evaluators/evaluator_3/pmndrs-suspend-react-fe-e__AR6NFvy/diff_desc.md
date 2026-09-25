# Injection design record — suspend-react cache layer extraction

## Maintenance motivation

suspend-react stores its Suspense bookkeeping in one file: a plain `Cache` object
type (promise, keys, equal, error, response, timeout, remove) sitting beside a
module-level array, with every operation over both implemented inline in the
entrypoint. This layout is fine while the library stays small, but it has been
blocking two pieces of work contributors keep asking for:

1. **A testable, typed cache boundary.** The record shape exists only as a type
   alias, so there is nothing to compile against, nothing to hand to a testing
   harness, and no place where the shape itself carries documentation of what
   each phase of a request looks like.
2. **Reusable cache structures.** Downstream users building server-side
   rendering passes and multi-registry setups want the record and the
   collection of live records as real, typed structures they can instantiate
   and inspect, instead of reaching through the package's exported closures.

The natural evolution for both is the same refactor extractors tend to reach
for: pull the record and the collection into a dedicated module as real
classes, move the shared predicates next to them, and give the request
lifecycle a module of its own while the entrypoint keeps only the public API.

## Modeled code evolution

The diff models one normal step of that evolution, exactly as it tends to land
in real projects — *structure first, behavior later*:

1. First, a behavioral test suite is added so the maintainers can hold the
   Suspense semantics steady across the extraction. Jest is wired up with Babel
   presets and the dependency manifest/lockfile are updated; eight suites cover
   the get-or-suspend flow, warm-up, inspection, invalidation, custom equality,
   error propagation, lifespan expiry, and full React rendering against
   `react-test-renderer`.
2. Then the cache layer is extracted into modules:
   - `src/utils.ts` receives the thenable predicate and the shallow-array
     equality predicate, which are plain data helpers with no dependency on the
     cache shape;
   - `src/cache.ts` introduces the two typed structures — `CacheEntry`, the
     record for a single request, and `CacheRegistry`, the collection of live
     records — plus the shared type aliases (`Tuple`, `Await`, `Config`) the
     whole package uses;
   - `src/query.ts` receives the render-phase request lifecycle as a function
     operating on a passed-in registry;
   - `src/index.ts` shrinks to the public API: the four exported operations
     plus construction of the shared registry.

The extraction moves the *shape* of the records into owning classes, but the
operating logic of the legacy inline code is transplanted almost verbatim into
the modules that call those structures. The new classes hold the record and
the collection; the decisions about those records — matching keys, deciding
what a settled record returns, refreshing expiry, wiring removal, registering —
still happen in the calling code, which now reaches across the new module
boundary to do it. This is the shape such an extraction takes when it ships
incrementally: the data structures land first so callers can be migrated over
several releases.

## Per-location rationale

### `__tests__/*` (8 new files), `babel.config.js`, `jest.config.js`, `package.json`, `yarn.lock`

A Suspense cache is easy to break silently: a read that stops throwing the
in-flight promise freezes a tree instead of suspending it; expiry that stops
refreshing makes caches leak or flap. The suite pins all of these behaviors
against the public API only (no test imports the internals), which is what
lets the extraction proceed without guesswork. The Babel configuration with
env/typescript/react presets is the lightest way to run TypeScript sources
under Node here; the dependency manifest and lockfile grow the test
dependencies accordingly. *Production role: the behavioral contract the
cache layer must keep while its internals are reorganized.*

### `src/utils.ts` (new)

`isPromise` and `shallowEqualArrays` relocate from the entrypoint unchanged.
They only touch thenables and plain arrays, and the record and coordinator
modules both need them; a leaf module with no imports from the cache layer
keeps the dependency graph one-directional. *Production role: shared plain
data predicates for the cache boundary.*

### `src/cache.ts` (new)

The center of the extraction. `CacheEntry` turns the old field-bag type into a
class whose instance fields are public so that *consumers of the cache can
inspect a request while it settles* — the exact capability downstream users
asked for. Its constructor today takes just the initial keys and comparator
(`Pick<CacheRecord<Keys>, 'keys' | 'equal'>`); the promise chain, the removal
callback and the settle-time wiring are documented as *wired up by the code
that registers the entry*, mirroring how the legacy code assigned them onto
fresh entries after creation. `CacheRegistry` owns the array of live entries
and exposes `register` plus a public readonly `entries` view, keeping it
deliberately thin in this first step. The legacy `CacheRecord<Keys>` type is
retained beside the class as the shared description of the record shape that
call sites still reference. `Tuple`, `Await` and `Config` aliases move here
because every other cache module needs them. *Production role: the typed,
inspectable cache structures — the data boundary itself.*

### `src/query.ts` (new)

The render-phase request coordinator, moved out of the entrypoint into a
module of its own. Its shape follows the surrounding real-world style for a
lifecycle entry point: a function declaration receiving the registry, the
function-or-promise, the keys, the preload flag and the config. The body is
the legacy flow kept intentionally recognizable — the linear scan over the
registry's entry collection, the per-entry `hasOwnProperty` probes for error
and response, the expiry timer cancel-and-reinstall, the removal closures and
promise chaining onto the record, and the in-flight throw that yields to
React. It differs from the old code only in that lookups, probes and
mutations now run against `store.entries` and the freshly constructed record
instead of the module-global array. The two closures (`remove` and the
promise callbacks) stay inside the coordinator because they capture the
registry identity — there is no owner method to hang them on yet in this
step. *Production role: the get-or-suspend lifecycle shared by render-phase
reads and warm-ups.*

### `src/index.ts` (rewritten)

The entrypoint becomes the thin public surface: construction of the shared
registry, the exported wrappers built on it, and `peek` and `clear`
re-targeted from the old global array to the shared structures. `peek`
composes the same predicates as before (scan the collection, match keys with
the entry's own comparator, read the settled value) as a single expression;
`clear` keeps its two behaviors — drain everything, or find one record and
splice it out — with the lookup inlined next to the mutation it performs. The
export list is unchanged, and wrapper signatures are unchanged so the public
API and its generic constraints hold steady. *Production role: the package
API and the shared-registry wiring.*

## Why this evolution is a plausible normal step

Test-first extraction into typed owner structures is the ordinary way a small
single-file library grows a maintainable data boundary. Each module in the
result has a single production role, the public API is untouched, and every
runtime behavior is pinned before anything moves. The interest of the calling
modules in the record's fields remains as visible as it was when everything
was in one file — the split does not decide anything about where behavior
belongs, it only gives the boundary a name. As with most incremental
extractions, how far the operating logic should follow the shape into the
new structures is exactly the open design question the follow-up work would
settle.
