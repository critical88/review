# tsar: the per-column tail statistics are getting unwieldy to pass around

## Where I noticed it

While preparing tsar's tail statistics for additional aggregate kinds, I kept
having to hand the same per-column aggregates — the running maximum, the
running mean and the running minimum that tsar renders as the MAX, MEAN and
MIN sections when it replays its stored history — from one helper to the next,
one argument at a time.

Any piece of work that touches a stage of the per-column aggregate upkeep now
lists all three aggregate pointers in its signature, next to whatever other
values it actually needs. Some of them even rename the trio to match the local
function, so the same three aggregates appear under two different parameter
name families. The call sites are no better: each one digs the same three
members out of the module record and passes them along one by one, and one of
them only really uses one of the three. If the next aggregate kind lands, all
these signatures and call sites grow by one more argument, and then another.

The part that worries me most is that this is not one framework function's
private affair: the same trio is re-assembled on both sides of the tool — in
the statistics pipeline that maintains per-column values while collecting, and
in the history output code that later decides which of the three aggregates a
printed section should read. Two unrelated production areas are agreeing on
the same implicit group by spelling it out every single time.

## What I would like done

Investigate how tsar maintains and consumes these per-column tail aggregates
across the statistics framework and the tail/history output path, find every
place where the recurring max/mean/min group is passed around as individual
pointers, and give the group one proper home so that all the work done on it —
seeding, updating, and selecting for output — goes through a single owning
abstraction instead of a hand-rebuilt argument triple. The goal is that adding
aggregates, or changing how the existing ones are maintained, has one obvious
place to happen, and that no two areas of the code need to agree on the
grouping by spelling it out parameter by parameter again.

## Behavior that must stay exactly as it is

The refactor must not change what tsar reports or how it talks to its plugins:

- the seeding and upkeep semantics of the aggregates themselves: on the first
  retained record each column's maximum, mean and minimum are established
  from that record's value, and on later records the running maximum and
  minimum move only under the current margin comparisons (0.1, with the
  non-negative guard on the minimum), while the mean is still the same
  incremental average tied to the retained-record counter, updated after the
  extremes within a record pass;
- the MAX, MEAN and MIN history sections keep rendering exactly as before,
  including which aggregate a section reads and what happens for an
  unrecognized selection;
- everything else in tsar (record parsing, --list, per-module statistics
  collection and output) is untouched;
- the module record's existing field layout and the loaded module interface
  stay intact — the module plugins are compiled against that record and their
  registration and callback signatures must keep working unchanged;
- the standard build (running `make` from the repository root) must stay green
  and the repository's tests must pass without new failures.

This is repository-level refactoring of one recurring design problem, not a
single call-site cleanup: make sure every place where the group currently
travels piecemeal is migrated, in both the statistics-maintenance code and
the history-output code.
