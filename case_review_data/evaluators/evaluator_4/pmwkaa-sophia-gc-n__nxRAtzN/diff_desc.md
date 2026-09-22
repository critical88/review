# Injection design record: the maintenance scheduler absorbing its neighbours' policies

Repository: `sophia` at commit `1fb4966b35dc3457cc4bb40ec719a14025a34fd4`.
This record describes the change `smell.diff` makes to the pinned clean checkout,
the maintenance situation it models, and the reason each location has the shape
it has. It is an auditable design document, not a to-do list: it states what a
developer changed and why that change is one a real project could make.

## Component layout this change relies on

sophia is organized as engines behind prefixed interfaces:

- **index** (`sophia/index`, `si_`) owns LSM node and page internals: the node
  tree, per-node indexes (`i0`/`i1`), rank queues used for maintenance ordering,
  and the decision of *which* node a maintenance operation should take
  (checkpoint, compaction, node gc, gc, expire, backup).
- **wal** (`sophia/wal`, `sw_`) owns log files: opening, rotation, and
  garbage-collecting completed files.
- **scheduler** (`sophia/scheduler`, `sc_`, struct `sc` plus per-database
  records `scdb` and per-worker `scworker`) owns *when* maintenance runs:
  per-database periodic scheduling, per-operation worker admission, dispatch,
  retries, and the worker trace.
- **environment** (`sophia/environment`, `se_`) presents engine state as the
  user-visible configuration/status surface.

## Maintenance motivation and the evolution being modeled

Historically the scheduler asked each engine through its public interface: for
each maintenance slot it called the index component's plan-selection entry
point, and the worker loop asked wal whether the log needed attention. The
scenario modeled here is a maintenance-stall investigation that punishes every
component hop: "when does work run" (scheduler) and "which work runs" (engine
policy) lived on opposite sides of a delegation, with locks in between, so
stepping through one decision meant reading two modules in two components.

The change is the settlement a developer reaches when that hop costs more than
the boundary does, applied three times in a row:

1. **Plan selection inlined into the scheduler.** The scheduler stops calling
   the index planning entry point and implements the selection cascade itself,
   reading index runtime state directly. The index-side dispatcher and its
   six specialized peek helpers no longer have callers, so they are deleted.
2. **Statistics folded into the scheduler's snapshot.** Per-database status
   reporting used to combine a scheduler-state snapshot with the index
   component's statistics module. The scheduler exposes one merged snapshot
   that copies its own state and then walks the index tree itself, accumulating
   the same numbers into the *scheduler's* per-database record, whose layout
   grows to describe index internals. The index statistics module, now
   unreferenced, is deleted along with the engine-side record type.
3. **Log maintenance policy taken over by the scheduler.** Instead of asking
   wal whether the active file is rotation-ready, the step loop makes that
   decision itself, sweeps the manager's list, and unlinks finished files
   through a per-file primitive wal now exports; wal keeps the physical
   rotation, its externally driven commands and the wreckage-free recovery
   paths.

Each step is individually defensible in review (fewer hops, one lock sequence,
one place to step through). Taken together they move three engines' decision
logic into one component while each engine keeps the *rest* of its logic at
home — which is exactly the friction a maintainer discovers later: to change a
log-file rule or an index matching condition, the scheduler must be edited, and
two separately-owned halves of one rule have to be kept in agreement.

Every matching condition, comparison, threshold, return value and trace string
is the one the previous owner used, and each operation's external effect
ordering is kept; the change alters where the decisions live, not what they
decide.

## Cluster A — index plan selection

**New `sophia/scheduler/sc_plan.c`.** One new module carries the absorbed
responsibility:

- `sc_plan(scdb *db, siplan *plan)` implements the whole selection cascade: a
  six-way branch on the plan type with, per type, the exact matching rule
  previously used by index — LSN bounds for checkpoint, cache-per-node
  watermark computation and big-in-memory-node search for compaction,
  delayed-gc list walking for node gc, duplicated-key ratio for gc, expiry
  timestamps for expire, backup bounds plus the completion fallback for backup
  — including locked-node retry handling, node locking on selection, and the
  index lock around the queue walk that the engine used to take.
- `sc_plantrace(siplan *p, uint32_t id, sstrace *t)` renders a selected plan
  into the worker trace. It moves with the cascade because every plan the
  scheduler picks is now traced where it was picked; the worker previously
  asked the index component to render this line.

Why here and in this shape: the scheduler's call centers hold the
per-database scheduling record, so `sc_plan` takes that record and reaches the
index engine record from inside it; the plan parameter is the engine's own
selection state carried in-the-open. The cascade is inlined rather than
delegated so that one step-through covers the whole decision. The file gets
the standard component preamble and include set of a scheduler module, so it
reads as first-class scheduling code, with the index internals (`si_lock`,
`sinode`, rank-queue traversal, node header fields) as working details inside.

**New `sophia/scheduler/sc_plan.h`** declares both procedures as the
scheduler-side face of plan selection; `sophia/scheduler/libsc.h` includes it
beside the existing scheduler headers.

**Drain side, index:** `sophia/index/si_planner.c` loses the dispatcher
(`si_plan`), all six peek helpers and the trace renderer;
`sophia/index/si_planner.h` drops their declarations; `sophia/index/si.c` loses
the `si_plan` wrapper and `sophia/index/si.h` its declaration. What stays in
the index component is deliberately the *statekeeping* half of the planner: the
rank-queue used by maintenance ordering and its update/remove calls, which
index-side node and document mutations still drive. The knowledge that moved
is the policy half — which node satisfies which operation — while the bookkeeping
half keeps its index-internal callers.

**Call centers, scheduler:**

- `sophia/scheduler/sc_step.c` — the task builders for checkpoint, compaction
  and node gc previously asked index to choose; they now call the absorbed
  `sc_plan` directly on the current database record. The three queue-driven
  operations (backup, expire, gc) previously funneled through a small static
  wrapper that applied per-queue worker admission first; admission is genuinely
  scheduling business, so it survives as `sc_plan_queue(sc *s, scdb *db, int
  id, siplan *plan)` — a thin scheduler-local gate that then hands over to the
  absorbed cascade. `sc_execute` traces the plan with the re-homed renderer
  instead of the engine's one.
- `sophia/scheduler/sc_ctl.c` — `sc_ctl_compaction` is the operator-driven
  manual compaction command; it selects its plan through the absorbed cascade
  too. This gives the new procedure two distinct caller classes (background
  loop and control command), so it is not a private helper of one path.

**Build wiring:** the scheduler makefile gains the new object; nothing else in
the scheduler build changes. `sophia/index/libsi.h` keeps its included planner
header since the surviving bookkeeping interface still lives there.

## Cluster B — index statistics aggregation

**New `sophia/scheduler/sc_profile.c`.** `sc_profile(sc *s, scprofiler *p, si
*index)` produces the per-database runtime snapshot the environment reports:
under the scheduler's own mutex it copies the live per-database scheduling
state, then under the index lock it walks the engine's node tree and
accumulates the same totals the index statistics module used to compute —
stored and in-memory keys, duplicated keys, index size and origin size, page
counts, memory used, and the engine's disk/cache read counters.

Why here and in this shape: the caller is the status-serialization path, which
the scheduler cannot quit owning (it *is* the state being summarized), so the
path of least resistance for the incident being modeled — "the two snapshots
disagree by the time we serialize them" — is one hole that produces both at
once. The index tree traversal, node header field reads and the size helper
that used to sit behind `si_` interfaces become scheduler-side working
details.

**`sophia/scheduler/sc_profiler.h` grows its record.** `scprofiler` was the
scheduler's small state summary (`scdb state`). It now also carries the
aggregated totals — node counts and sizes, page count, memory used, key and
duplicate counts, read counters — i.e. the record's layout becomes a
description of index internals. The engine-side record type that previously
held exactly these fields was deleted with the module below, so the
scheduler's record is now the only place these statistics have a name. The
header's small inline state-copy helper gives way to a declaration of the new
module's procedure, whose body lives in `sc_profile.c`.

**`sophia/environment/se_db.h`.** The per-database record (`sedb`) no longer
needs a private statistics record for the engine snapshot — the merged
scheduler-side record replaces it, so the member is removed (the existing
scheduler-side member stays).

**`sophia/environment/se_conf.c`.** `se_confdb`, which serializes each
database into the configuration/status tree, swaps the old
begin/walk/end sequence for one call into the merged snapshot, and the nine
status subtree leaves (memory used, node sizes, key counts, duplicate counts,
read counters, node and page counts) read from the scheduler-side record.

**Drain and deletion, index:** `sophia/index/si_profiler.c` and
`sophia/index/si_profiler.h` are deleted outright — begin/end/walk and the
`siprofiler` record type have no remaining callers after the move.
`sophia/index/libsi.h` drops the header include and the index makefile drops
the object.

## Cluster C — wal log maintenance policy

**New `sophia/scheduler/sc_log.c`.** `sc_logmaint(swmanager *wm, scworker *w,
int gc)` is the step loop's log housekeeping in one procedure with a mode
switch:

- rotation side: trace marker, enable check, manager spinlock, read the
  active file's garbage-completion state, compare with the rotation
  watermark, and if satisfied call the manager's rotation;
- sweep side: trace marker, enable check, a spinlock-guarded scan of the
  manager's file list for a completed file, unlink it through the per-file
  primitive and repeat until no garbage or disabled.

Why here and in this shape: the step loop is the only actor that needs the
readiness decision in the fast path, and a single `swmanager`-parameter
procedure with a flag keeps one working-variable set in one place. The worker
trace marker moves with the calls so the maintenance trace stays in order.
The decision, the sweep policy and the file list traversal knowledge now read
as scheduler business.

**`sophia/wal/sw.c` / `sophia/wal/sw.h`.** The engine loses its readiness
procedure (`sw_managerrotate_ready`): the only caller was the scheduler
wrapper, which the new procedure replaces. The per-file wreckage routine —
unlink the log file, close and free it, report malfunction — was a
file-local helper used by the manager's own sweep; it becomes the manager's
exported per-file `sw_remove`, because the scheduler's sweep calls it too,
and the manager's externally callable garbage-collect command uses the same
primitive. What deliberately *stays* in wal: the physical rotation, the
externally drivable rotate/gc commands (the environment's configuration
interface dispatches to them, and bring-up/recovery uses the same entry
points), and file copy-out for backup. The consequence of the split — the
readiness rule now living in the scheduler while the rotate act lives in wal
— is exactly the kind of thing a change like this leaves behind: one rule,
two components, agreement enforced by convention.

**`sophia/scheduler/sc_step.c`.** The two thin statics that previously asked
wal from the step loop (`log rotation` and `log gc` wrappers around the
engine's calls) are gone; `sc_step` calls the absorbed procedure at the same
two points with the same trace outcome, since both call sites already sat
side by side in the worker's step.

## Interface and build wiring summary

- scheduler: new `sc_plan.c`/`sc_log.c` objects and the `sc_profile.c` object
  join the component build; `libsc.h` includes the two new headers.
- index: `makefile` drops the deleted statistics object; `libsi.h` drops its
  header include.
- No other component, header or test file is touched; component makefiles
  remain as they are since the amalgamation build picks up new objects by
  list.

## Exploration of the responsibility graph (what the change considered)

Edges the change *includes*, with the scheduler entry that made each
absorption possible: plan selection (task loop per-database dispatch and the
manual compaction command), plan trace rendering (worker trace), statistics
aggregation (status serialization path), rotation readiness (step rotation),
log sweep policy (step gc).

Edges considered and left in place, with reasons recorded as design decisions:
wal's rotate/gc *acts* (their non-scheduler callers keep them engine-owned);
wal's per-file wreckage (an act, re-exported rather than moved); index's
rank-queue bookkeeping (index-internal callers); the scheduler's per-queue
admission gates (genuinely scheduling, kept as thin wrappers at the call
centers); the environment's reporting itself (the change rewires its source,
not its home).

Roles the change exercises through the three clusters: multi-way policy
branching over engine state; derived-calculation aggregation from engine
internals; lifecycle mutation of another component's files; a foreign engine's
lock used inside scheduler code; a scheduler-side record type redefined to
describe engine internals; and one maintenance rule split across two
components that must stay agreed.

The load-bearing saturation argument: the transaction component, the page
format and the repository components expose no scheduler-visible policy entry
point, so absorbing from them would require inventing wiring rather than
removing friction; the remaining oversized scheduler procedures are its own
task composition and backup procedure steps rather than engine policy; and any
further pickup from index or wal would repeat an already-covered role (a third
reader of node internals) instead of adding a distinct responsibility. The
change therefore stops where the evolution story stops being believable.
