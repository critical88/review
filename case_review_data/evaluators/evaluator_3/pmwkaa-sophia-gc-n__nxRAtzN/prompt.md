# Scheduler sprawl: maintenance policy no longer lives with its engines

I've been carrying a grudge against our background maintenance scheduler, and
after this week's incident I want it cleaned up. A colleague needed to add a new
maintenance behaviour and, instead of touching the engine that owns the data,
ended up editing the scheduler: reaching into node internals, re-deriving log
file rules, and poking at statistics calculations from the outside.

It didn't get here in one jump. The scheduler already ran the maintenance loop,
so each time debugging wanted fewer hops between "when does work run" and "which
work runs", another engine's decision landed in scheduler code. It's now the
place where maintenance policy gets implemented: procedures that decide what to
do by reading other engines' structures directly, a scheduler-side record that
describes another engine's internals because the engine's own way of computing
them is gone, and home-grown log housekeeping that decides on its own when log
files are finished and sweeps them. The engines still hold the rest of their
logic, so the same rules now exist in two places that have to be kept in
agreement — and anyone changing index internals or log file handling has to
know scheduler code to do it.

Please restore sensible ownership. Maintenance decisions — choosing which
node or file gets worked on, and computing descriptions of an engine's state —
belong to the engine that owns those structures, through its own interface. The
scheduler should go back to scheduling: ordering, triggering, admitting work to
worker limits, dispatching, retrying, and tracing what it ran.

Please investigate the scheduler's absorbed procedures together with the
engines' sides of the same responsibilities — including declarations, headers,
build lists and any reporting path that consumes a record whose home changed —
and finish the job everywhere it reaches, not only at the obvious entry points.

Nothing user-visible may change: the same maintenance choices happen under the
same conditions, the same statistics are reported, log rotation and collection
happen exactly as before, and runtime commands and recovery paths keep
working. The full test suite (`./sophia-test`) must stay green without test
changes.
