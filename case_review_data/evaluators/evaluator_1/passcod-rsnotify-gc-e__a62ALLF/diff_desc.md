# Change record — consolidating the notify-debouncer-full pipeline into the coordinator

## Maintenance motivation

For a while the full-feature debouncer has been migrating toward `Debouncer` — the guard type
callers hold — being the one object that owns the pipeline. The crate's own deprecation hints
already point that way: the `watcher()` shim says "`Debouncer` provides all methods from
`Watcher` itself now", and the `cache()` shim says "`Debouncer` now manages root paths
automatically". Root administration used to be the caller's job; it moved onto the guard one
release ago and the crate got simpler to explain.

This change continues that same direction to its conclusion. The motivation is practical: bug
fixes over the last months (a swallowed removal on macOS that never reached consumers, renames
that arrived as two events under heavy load, stale events delivered after a back-end reset)
each required reading several files at once to see how ingestion, identity tracking, expiry and
delivery interact. Keeping those decisions in the guard means every pipeline answer lives
behind one type — no more hopping between a state machine struct, a queue helper, a sorting
free function, a time module and the factory to follow a single event. The trade-off is
accepted deliberately: the coordinator pattern buys one obvious place to look, at the cost of
a much larger surface on the guard itself and thinner helper modules.

The evolution is modeled as it would really happen: a sequence of related maintenance rounds
(one per area of the pipeline), not a single big-bang rewrite, so each absorption keeps the
shape of the code it came from.

## Overall design

`Debouncer<T: Watcher, C: FileIdCache>` becomes the pipeline's single decision-maker:

- `DebounceDataInner` stops being a generic owner of behavior and becomes a plain
  `pub(crate)` state record: pending queues, sorted watch roots, the in-flight rename event,
  the pending rescan event, stacked errors and the timeout. It carries no methods.
- `Queue` keeps its data and its ordering convention but loses the helpers that interpret its
  head; interpreting queues is now coordinator policy.
- All state-machine logic (event intake dispatch, rename stitching, queue surgery, rescan
  serialization, error stacking, expiry) moves into `Debouncer` as associated functions taking
  the state record (and, where identity tracking is needed, the cache).
- The file-ID cache moves out of the state record into a field of the guard (`Arc<Mutex<C>>`),
  so registration, removal and intake — which all run on the guard — share one cache handle in
  the exact lock order the guard uses.
- The old free `new_debouncer_opt` factory body becomes `Debouncer::new_opt`; the export stays
  as a thin wrapper so downstream code is untouched.
- The production clock moves from the `time` module to the coordinator; the module survives
  only as the mockable test clock, and the coordinator exposes the mock branch under the test
  configuration alone.
- `FileIdMap` becomes a data container whose trait implementation forwards to coordinator
  functions — the coordinator owns scanning policy (crawl on registration, forget a subtree on
  removal, rescan every root after a back-end reset, membership queries), the same way it owns
  queue policy. The watcher half of that second impl block is pinned to notify's inert null
  watcher, since these functions operate on the cache alone.

Locking discipline is uniform: the pending-state mutex is always taken first, the cache second,
in every code path that needs both, so the watcher callback (inner → cache), the guard's root
administration (inner → cache) and the delivery thread (inner only) never disagree about order.

## Per-location record

### `notify-debouncer-full/src/lib.rs` — pending-event state machine into `Debouncer`

The ten-method state machine on `DebounceDataInner` (`new`, `debounced_events`, `errors`,
`add_error`, `add_event`, `recursive_mode`, `handle_rename_from`, `handle_rename_to`,
`push_rename_event`, `push_remove_event`, `push_event`) is dissolved; the bodies re-emerge as
associated functions on `Debouncer` (`expire_events`, `take_errors`, `push_error`,
`dispatch_event`, `recursive_mode_for`, `handle_rename_from`, `handle_rename_to`,
`push_rename_event`, `push_remove_event`, `push_event`), each now taking `&mut
DebounceDataInner` (plus `&mut C` where identity tracking or rescan policy needs the cache).
This site was chosen because every one of these operations needs the same three participants —
pending state, watch roots, file-id cache — and the guard is where all three are already in
scope. Keeping the bodies close to their original shape (same order, same fold rules, same
queue surgery) preserves the well-tested behavior exactly: dedup of create/modifications on
fresh queues, tracker/file-identity rename stitching with head-of-queue policy, removal of
child queues with a parent remove, and the extract-per-kind expiry walk. Production role: the
event pipeline itself — the most exercised code path in the crate.

### `notify-debouncer-full/src/lib.rs` — queue policy into the coordinator

`Queue::was_created` and `Queue::was_removed` — the head-of-queue predicates that decide when
a path counts as freshly created or freshly removed, which in turn gate duplicate-create
suppression, post-create modify suppression and rename event placement — move to
`Debouncer::queue_was_created` / `queue_was_removed`. The queue type keeps only its data and
its documented ordering convention. This is a policy site rather than a mechanical one: the two
predicates encode debouncing rules that the coordinator already owns everywhere they are
applied (intake, rename stitching, removal); keeping them next to that consumer is the
consistency argument for the move.

### `notify-debouncer-full/src/lib.rs` — flushing and merge ordering into the coordinator

The private free function `sort_events` (chronological interleave of expired buckets, with
path order breaking timestamp ties and same-path events keeping their relative order) becomes
`Debouncer::sorted_events`, used by `expire_events`. The original was a pure free function
with exactly two callers inside the state machine; as the last step of expiry it is delivery
policy, and the coordinator is where expiry lives now. The grouping heap, the path-tail keying
and the per-group drain order are preserved verbatim. Production role: the actual ordering
guarantee consumed by every downstream event handler.

### `notify-debouncer-full/src/lib.rs` — construction and thread wiring

The free `new_debouncer_opt` factory body (tick validation, delivery-thread spawn, watcher
construction and guard assembly) becomes `Debouncer::new_opt`; the exported free function
remains as a wrapper so the public surface (`new_debouncer_opt`, `new_debouncer`) is
byte-for-byte API-compatible. The delivery thread calls the coordinator's
`expire_events`/`take_errors` under the state mutex; the watcher callback calls
`dispatch_event`/`push_error` with the state mutex taken before the cache mutex, matching the
guard's own lock order. The factory is where the three absorbed participants (state record,
cache handle, stop flag) are created — the natural site to demonstrate the completed take-over,
and the only place the wiring could remain single-screen readable. Production role: the wiring
point used by every downstream consumer.

### `notify-debouncer-full/src/lib.rs` — the production clock on the coordinator

`Debouncer::now()` replaces the free `time::now` in the production path. Test builds route to
the module's mock clock through a test-configuration branch on the coordinator; release builds
read the system clock directly. This mirrors what the time module already was (a production
now plus a mock for the fixtures) but places the clock with the type whose timestamps they
are. The intake, rename and expiry call sites now read `Self::now()` — one consistent clock
for every timestamp the pipeline records. Production role: timestamps recorded on each
debounced event and stashed rename event.

### `notify-debouncer-full/src/file_id_map.rs` — the cache as a container

The implementation of `FileIdCache for FileIdMap` reduces to one-line forwards into the
coordinator's file-ID bookkeeping (`cached_file_id_of`, `scan_file_ids`, `forget_file_ids`,
`rescan_file_ids`, with `dir_scan_depth` as folder-scan policy). The crawl body (WalkDir with
link following, recursion via `max_depth`), the subtree-forgetting `retain`, and the root
rescan loop live with the rest of the pipeline policy on `Debouncer`; the map itself keeps the
`paths` inventory reachable to the coordinator (`pub(crate)`). A `rescan` override joins the
trait impl so the identity rescan — previously inherited default behavior — is now explicitly
part of the same coordinator-owned policy set. Production role: the identity-tracking support
that stitches renames on platforms without rename cookies; on Linux the same functions serve
any configured `FileIdMap` cache without behavior change.

### `notify-debouncer-full/src/time.rs` — the mock clock stays for fixtures

With production timestamps owned by the coordinator, the module keeps only the mock clock:
`now()` reading the thread-local and `MockTime` for setting/advancing it, compiled only for the
test configuration. What used to be a dual build/test pair collapses to one test-support
file. Production role: none anymore — it now exists purely so the fixture-driven state-machine
tests can fast-forward time deterministically; production reads its clock from the
coordinator.

### `notify-debouncer-full/src/testing.rs` — fixtures speak the coordinator

`State::into_debounce_data_inner` (which built the state record with its cache baked in)
becomes `State::into_debounce_parts`, returning the record and the fixture cache separately —
mirroring the split where the state record no longer carries the cache. The fixture types and
the whole hjson schema are unchanged; only the construction hand-off and the narrowed import
list adapt. Production role: none — this is test support paired one-to-one with the state
machine's new call shape.

### `notify-debouncer-full/src/lib.rs` — keeping the fixture tests meaningful

The `rstest` state-machine cases keep their hjson fixtures untouched (event streams, expected
queues, expected rename/rescan/error state), but now drive the pipeline through a `TestDebouncer`
alias — the coordinator instantiated with notify's inert null watcher and the fixture cache —
calling `dispatch_event`, `push_error` and `expire_events` with the state record and cache
directly, so no thread is involved in the fixture tests at all. The ordering tie test uses the
coordinator's `sorted_events`, and the recursive-root resolution test uses
`recursive_mode_for` with a plain record. The macOS regression tests and all
public-interface tests (construction, update-paths error accounting, watched-path
forwarding, channel handler tests, the doctests) are unchanged in intent and assertion; the
two that constructed the state machine directly now build the record and cache pair through the
same seam.

## What the crate looks like afterward

One impl block on `Debouncer` carries its original forwarding/lifecycle methods together with
the re-homed pipeline functions; a second, inference-friendly impl block carries the file-ID
bookkeeping for the built-in cache. The state record, the queue, the cache container and the
mock clock are inert data or test support. Downstream-facing functions and types — the free
constructors, the public methods, the trait, the exported caches — are untouched in name,
signature and shape, so no caller in the workspace or in downstream code needs to adapt.
