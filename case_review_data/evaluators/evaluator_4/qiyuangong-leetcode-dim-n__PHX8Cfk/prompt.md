# Recursive solutions whose entry methods became giant single functions

While chasing an edge case in one of the backtracking-style answers under `python/`
last week I ended up reading most of the recursion-based solutions in this
collection, and a few of them keep slowing me down. Their public entry methods are
no longer really entry points. A single method does everything at once: it creates
the state the recursion needs on the first call, recognizes "this is the first
call" by comparing its own defaulted parameters against placeholder values
(`None`, `-1`, `0`), re-enters itself to continue the work, and carries the small
scans and stitching loops each step needs right inside the same body. Three kinds
of work — preparing the problem state, stepping through it recursively, and the
per-step checks — are interleaved in one long method, and the defaulted-
parameter sentinels are the only seam between them.

The history, as far as I can tell from the blame conversations, is that an earlier
sweep over this directory removed what it called needless indirection: the small
private helper methods each recursion used to have were folded into their only
caller, and the state that used to be threaded through those helpers became extra
defaulted parameters on the entry method so it could re-enter itself. The answers
produce correct results, so nobody reverted it, but the structure is now a real
cost: to reason about a backtracking order I have to mentally execute fifteen-plus
lines in which the undo step, the acceptance check and the recursive call sit next
to first-call initialization, and the divide-and-conquer style ones are no better.
New contributors routinely miss that the same function is playing driver, walker
and helper at once.

Please restore a saner method structure across the recursion-based solution
classes in this directory: whatever phases a solution currently crams into its
public entry method should be genuinely separated into well-scoped helper methods
at an appropriate level of abstraction, the way the rest of this collection does
it. Reading an entry method should make the phases obvious; a public entry method
that selects between "first call" and "recursed call" by testing its own defaulted
arguments against sentinel values, while also serving as its own recursive step,
is the shape I want gone.

This is behavior-critical cleanup, and it has to be invisible from outside:

- every solution class keeps its current public interface; the call forms used
  today (including the argument lists callers already pass) must keep working and
  produce the same results and the same in-place effects;
- nothing observable may change for any input you can exercise: no result value,
  no returned structure, no in-place mutation, no exception. Note that some of
  this code is Python-2-era and leans on true division (`/`) in the middle of its
  arithmetic — a few odd-shaped inputs legitimately raise `TypeError` today, and
  those failures are part of the expected behavior, not bugs to fix;
- the decomposition must be real: the state preparation, the recursive stepping
  and the per-step checks genuinely belong in their own methods, not in one
  renamed monolith with most of the body still inline;
- leave the solutions that are already cleanly decomposed alone, and restrict the
  refactor to the problem described — no reformatting of untouched solutions, no
  renamed files, no new abstractions beyond what these solutions need.

I have not enumerated the affected files on purpose; they are not hard to spot
once you know the shape, and I would rather not bias the search. When you are
done, a reader should be able to open any recursion-based solution here and see
the driver, the recursion and the checks as separate, obvious pieces.
