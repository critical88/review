# Injection design record — rsnotify diagnostics batch

## Maintenance motivation

This batch models one file-watching repository's response to four independent
support threads that happened to land in the same release window:

1. **Missed-events report (notify-debouncer-full).** A user filed a bug where
   events stopped arriving for a still-watched tree. To triage from a distance,
   the maintainer wanted two things attached to bug reports: a plain-text dump of
   everything the debouncer is currently holding back, and a gauge for the
   suspected failure mode where a queued path outlives the root that registered
   it (the stale-queue hypothesis).
2. **Delayed-burst repro (notify-debouncer-mini).** A downstream user of the
   mini debouncer reported bursts arriving seconds late. Walking raw events by
   hand from a debug-formatted one-liner was painful, so the intake trace was
   upgraded to a readable, one-observation-per-line account of every event the
   reduce loop receives.
3. **Dereference symptoms (inotify backend).** A user enabling `show` plus
   symlink-following watches saw duplicate events. The maintainer asked the
   reporter to run with trace logging and paste "the watch table": descriptor,
   mask, directory/dereference flags and per-watch request metadata at the
   moment each watch is installed.
4. **Size-discrepancy repro (poll backend).** An editor that writes via atomic
   replace produced change events with stalling sizes on a polling watcher. The
   maintainer suspected two stored path entries describing the same content,
   so rescans now emit a pairing report of entries that hold the same stored
   content hash, including their stored mtimes and last-check instants, with a
   note about the whole-second clock granularity watchers use for directories.

During the same investigations the maintainer also refreshed the field
documentation of one small bookkeeping record that kept coming up in these
conversations (`notify-debouncer-mini`'s `EventData`), so that documentation
traveled with the batch.

## Normal development evolution being modeled

Under triage pressure, each helper was written where the triggering call path
lives: the report helpers went onto the `Debouncer` facade users talk to and
into the unwatch cleanup that was already on the lock; the event walk went next
to the reduce loop that consumes it; the watch-table dump went into the inotify
loop that installs watches; the alias report went onto the rescan that would
already have the data loaded. None of this planning was done up front — it is
the shape four one-off investigations naturally leave behind when they each
stop at "make the log line say what the reporter needs to paste back".

## Overall design

Every helper added in this batch is a read-only string renderer:

* Its only output is a `String` (or formatted trace argument) assembled from
  data structures it reads but did not author.
* It is consumed by an existing or new `log::trace!` statement at a call site
  that already runs, so nothing new executes unless the application enables
  trace logging for the relevant target.
* It does not mutate, retry, drop or reorder anything; event delivery,
  debounce timing, watch registration and unregistration behave exactly as
  before. Tests were not touched.
* One helper is a public convenience (`Debouncer::backlog_report`), documented
  for attaching to bug reports; the rest are module-private surfaces for the
  trace paths.

The batch touches four production files across three crates and four
subsystems: the full-fledged debouncer facade, the mini debouncer's reduce
loop, the inotify backend's watch installation, and the poll backend's rescan.
The five renderers all read records owned by other types than the ones holding
the renderer, because each renderer was written from the point of view of the
operation that needed the log line (request a report, clean up a root, take in
an event, install a watch, rescan a tree) rather than from the point of view of
the record being described.

## Per-location account

### `notify-debouncer-full/src/lib.rs` — `Debouncer::backlog_report` (new public method)

Adds a `#[must_use]` method that renders "what is the debouncer holding back
right now": every queued path with its pending event count, the pending rename
and rescan bookkeeping, buffered errors and the registered watch roots, plus
the debounce timeout they are held against. Production role: the paste-into-a-
bug-report answer to "why did my events stop". It was placed on the `Debouncer`
facade because that is the object the user holds; to produce its text it reads
the shared debounce state (`DebounceDataInner`'s queue table, rename/rescan
events, error buffer and roots, plus each queue's pending count and head
event) through the lock the debouncer already owns. Head-style classification of the pending events
(`EventKind::Create`/`Remove`/name-modification) rides along the same reads.

### `notify-debouncer-full/src/lib.rs` — `Debouncer::remove_root` (clean-up path extended)

`Debouncer::unwatch` calls `remove_root` to drop a root and its cache entry. The
stale-queue triage added a gauge to this method: after the retain/cache
bookkeeping, the method now checks which queued paths would no longer be
reachable from any remaining root and describes what would be stranded — the
orphaned queues with their pending event counts and remaining-watch-root counts,
whether a pending rename or rescan event references the removed path, and how
many buffered errors are still awaiting delivery. Production role: a
consistency gauge on the exact path that could leave the suspected failure
state behind. The method holds the write lock on the shared debounce state, so
the added loops are for/while loops over the same state the retain just
touched (`queues` against `roots`, the cache, pending rename/rescan events, and
the error buffer).

### `notify-debouncer-mini/src/lib.rs` — `explain_raw_event` + trace rewiring in `AddEvent` intake

Replaces the intake's single `raw event: {event:?}` debug dump with a module
level `explain_raw_event(&event)` renderer consumed by the same trace statement
in the reduce loop. The renderer narrates the record the way a maintainer
reads it: kind, attached paths one per line, informational attributes, process
id, whether several paths share the event, which path comes first/last, and
which kinds can start or end a tracked burst. Production role: delayed-burst
walkthrough support. It is a free helper next to its only caller and reads the
`notify::Event` record it is handed; the mini debouncer stores parts of that
record in the `EventData` map but the rendering happens before any of that
bookkeeping, on the raw event itself. As part of the same burst investigation,
`EventData`'s field documentation was expanded to spell out insert-vs-update
semantics observed during the walk — documentation only.

### `notify/src/inotify.rs` — `EventLoop::trace_watches` + watch table dump at install time

The inotify event loop now renders its whole `watches` table to the trace log
every time a single watch is added. The report lists every watched path with
the descriptor the kernel knows, the installed mask, whether it covers a
directory, whether it dereferences links, whether it was requested directly by
the user, its recursion mode, and the path events are reported as — then a
summary count of directory/dereference/user/recursive watches, plus notices for
any path that no longer has a watch. Production role: the paste-back evidence
for the dereference symptom while it reproduces. The renderer hangs on the
`EventLoop` because that is where `add_single_watch` runs the creation; the
records it lists are the `Watch` entries (descriptor/mask/directory/dereference
flags) carrying `WatchMetadata` (user request mode, reported path, recursion)
that the loop installs into the table.

### `notify/src/poll.rs` — `WatchData::trace_content_aliases` + rescan wiring

The poll backend's rescan now also renders a stored-content alias report from
its per-path table. For every stored path entry it pairs against every other
stored entry and lists the pairs holding the same stored content hash: the two
paths, their stored mtimes and the gap between them, which of the two was
checked no earlier, and counts of alias pairs, same-clock pairs (the watcher
coarsens directory mtimes to whole seconds, which makes aliases inside the
same second pairwise confusing) and entries without a hash. Production role:
the size-discrepancy repro's paste-back. The renderer sits on `WatchData`
because `rescan` is the operation that would already be walking the tree; the
records it compares are `PathData` entries (mtime in nanoseconds, optional
content hash, last-check instant) held in the `all_path_data` table, reusing
the module's `NANOS_PER_SECOND` granularity constant for the clock-granularity
branching.

## Shape-selection rationale

* **Site selection**: each renderer lives on the type whose operation triggers
  it — the facade users hold, the clean-up that holds the lock, the intake
  that receives the event, the loop that installs the watch, the rescan that
  walks the tree. This is the layout interviews with maintainers produce when
  the investigation outruns the refactor.
* **Implementation shape**: pure string assembly over records read by field
  access, minimal branching, no mutation, plus straightforward loops (for
  maps small enough that the log can name every entry). String building was
  chosen over incremental `log::trace!` statements so the paste-back is one
  entity and so the rendering is testable as text.
* **Production role at each site**: requestable backlog summary, unwatch
  consistency gauge, event-walk narration, install-time watch-table evidence,
  rescan-time alias pairing. The roles are the triage workflows the four
  threads actually used; the wiring points are the operations a maintainer can
  name from a bug report.
* **Related-but-quiet materials traveling with the batch**: the `EventData`
  field documentation expansion, and the documentation blocks on the public
  report method, describing semantics learned during the investigations without
  wiring anything new in.
