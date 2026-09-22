# These new trace helpers keep reaching into state they do not own

During the last release I added a handful of observability aids while running
four support threads, and now my own review of that work keeps snagging on the
same thing. Before I open a proper PR I want a second pair of hands to untangle
it — carefully, because the reports and log lines are exactly what users paste
back to us in bug reports and I do not want a single one of them to change.

The four threads were:

* **Missed events on the full debouncer.** Users get a text summary they can
  attach to a bug report of everything the debouncer is holding back, and
  inside unwatch there is a gauge that tells us when a queued path outlives
  the watch root that registered it (still my prime suspect for the missed
  events).
* **Delayed bursts on the mini debouncer.** The intake trace got a
  one-observation-per-line narration of each raw event so a repro walk is
  readable instead of a single debug dump.
* **Duplicate events on inotify with dereference.** Each time a watch is
  installed, the loop now traces the whole watch table — descriptors, masks,
  whether watches dereference, what was requested directly — so the reporter
  can send us the picture.
* **Stalled sizes on the poll watcher.** Rescans now emit a pairing report of
  stored path entries that hold the same stored content hash, with their
  stored mtimes, check times and the whole-second clock note, since I suspect
  two entries describing the same file.

All of these ended up next to the code that happens to trigger them — the
object the user holds, the cleanup that already had the lock, the intake
function, the watch-install loop, the rescan. And every one of them, to produce
its text, walks somebody else's records one field at a time: the shared
debounce state, the raw event record from the shared event crate, each
installed watch entry with its request metadata, the per-path entries in the
polling tree. Putting it plainly, as a friend from the review put it: most of
these read like the state's own behavior glued onto whatever happened to be
holding the controller at the time. The behavior works, but it lives in the
wrong place, and I do not want the next contribution to copy that shape.

Please give each piece of behavior from that batch a home on the data it
actually describes, whatever shape that takes for each one, and reduce the
triggering spots to just asking for the account they used to assemble
themselves. I listed four threads, but treat the whole batch as the scope:
anything else from that investigations pass built the same way — including
formatting or gauging helpers — should get the same treatment wherever you
find it. If the natural owner is a type in the shared event crate, then the
shared event crate is where the behavior should be.

Hard requirements, because users and bug reports depend on them:

* The exact same trace lines and public report text, in the same situations,
  worded the same — I will diff the output before and after.
* The public summary stays available on the debouncer users hold, callable the
  same way, with the same result and documentation.
* All behavior outside this batch (event delivery, debounce timing, watch
  registration, unregistration) is untouched, and nothing the batch observes
  gets dropped or simplified because it was in the way.
* The workspace test suite keeps passing with no tests edited or skipped.
* The triage notes stay with the code that needs them — for instance the
  whole-second clock explanation the poll pairing relies on.

If you find something from the batch that already sits with the data it reads,
leave it be; I care about the ones that reach out.
