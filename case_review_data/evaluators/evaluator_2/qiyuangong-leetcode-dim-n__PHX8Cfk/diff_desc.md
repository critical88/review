# Injection design record — merged-body recursive solutions (`python/`)

This record documents the design intent behind `smell.diff`: the maintenance
motivation the change models, the development evolution it imitates, the overall
design, and a per-cluster explanation of what changed, why that site was picked,
and the production role each absorbed fragment serves. It is auditable design
rationale, not a review of the outcome.

## Maintenance motivation

Every file in `python/` is a stand-alone answer to one problem, and the entry
method of each `Solution` class is the artifact contributors actually care
about: it is the method the problem site's answer box requires, the method
shown in the README links, and the surface every later fix has to preserve.
The recursive answers in this collection grew a maintenance habit around that
fact: when a solution misbehaved on an edge case, the contributor did not
debug the decomposed file in place. They reworked the answer in the answer box
itself — a single-method environment where slices, accumulators, helpers and
recursion all have to live inside the one required method — and then synced
the accepted code back into the repository.

That workflow has a characteristic cost. The pasted-back answer is
self-contained by construction: everything the computation needs is in the one
body, the extra recursion state travels on defaulted parameters so the public
entry signature stays callable, and the first call is told apart from later
calls by comparing those parameters against placeholder values. The private
helpers the file used to have — written when the solution was first composed,
for clarity — are dead code after the sync, and they tend to get deleted in a
follow-up commit so the file matches the accepted answer. What remains is the
subject of this change: recursion-based answers whose entry methods run the
entire computation themselves, with defaulted parameters as the only seam.

The same motivation compounds during review: a single-method answer is easier
to paste into a comment thread or a discussion post than a three-method class,
so the sync-back keeps happening, and the merged shape spreads across the
recursion-based answers rather than staying in one file.

## Modeled development evolution

The change models one pass of that evolution over the corpus:

1. a contributor re-derives the solution inside the required entry method,
   re-entering it recursively instead of delegating to the walker method, and
   threading the recursion state through defaulted parameters instead of
   passing it explicitly to a helper;
2. the first call is recognized the cheapest way available — a sentinel
   comparison on one of those defaulted parameters at the top of the body
   (`x is None` for containers, `x == -1` for positional indices, `x == 0`
   when zero is already a legitimate phase value);
3. the support predicate or merge loop the walker used to call is folded into
   the re-entered body because, in the single-method environment, there is no
   other place for it;
4. after the sync, the now-unused original helpers are removed so the file is
   left exactly as the accepted answer.

The end state of that pass is the merged-body shape this change records.

## Overall design

The pass was applied to the recursion-based solution classes that actually
contained the three-level delegation the workflow dissolves: a driver
(prepare the state, kick off the recursion), a walker (re-enter itself over
the work), and a support predicate or merge routine used by the walker. Site
selection and the per-site reasoning are documented below; the key boundaries
applied everywhere were:

- the public entry call forms are untouched — a caller passing the original
  arguments keeps getting the original results and in-place effects;
- the Python-2-era true-division expressions in these files are carried over
  verbatim wherever the moved code needs them, exactly as the original
  helpers computed them;
- nothing outside the four solution files is edited, and each file keeps its
  character (the commented-out earlier approaches in the merge file, and the
  typo'd inline comments in the pattern-counter file, are left byte-for-byte
  so the sync-back remains a minimal edit).

Where a merged body needed a small local rearrangement to be coherent as one
function, the rearrangement is documented in the cluster entry rather than
silently smoothed over.

## Cluster A — `python/023_Merge_k_Sorted_Lists.py` (divide-and-conquer list merge)

- Before: `mergeKLists` (driver) checked the incoming container, derived the
  window bounds and delegated to `mergeK` (recursive window splitting over the
  list index range), which base-cased on singleton and two-list windows and
  delegated pairs to `mergeTwolists` (support: two-list stitching with a dummy
  head).
- After: one `mergeKLists(lists, low=0, high=-1)` runs everything. The `high`
  default is the sentinel: `high == -1` at the top widens the window to the
  full input, which is the driver's entire remaining job; singleton and
  two-list base cases are inline; the window split re-enters the same method
  for both halves; the surviving half is stitched with the same dummy-head
  walk the support routine used.
- Roles absorbed: container screening (driver), window halving (walker),
  two-list stitching (support).
- Site selection: the corpus's only divide-and-conquer merger, and the only
  site where the recursion is binary (two re-entry points per level), which
  exercises the merged-body shape where both recursion phases read state that
  the same scope prepared.
- Local rearrangement, kept honest: in the second stitching pass (the one
  after the window recursion), the two trailing "attach the exhausted rest"
  branches appear in the opposite order from the first inlined copy. This is
  the kind of drift the paste-back workflow produces rather than a semantic
  edit — after the stitching loop exits, at most one of the two lists is
  non-exhausted, so the two branches are alternatives, never a sequence. The
  same dummy-head discipline is kept for both copies.
- The two commented-out earlier approaches at the top of the file are the
  file's own history and were deliberately not touched.

## Cluster B — `python/037_Sudoku_Solver.py` (backtracking board solver)

- Before: `solveSudoku` (driver) collected the blank cells into a stack and
  handed it to `solve` (recursive backtracking with push/pop on the stack and
  write/revert on the board), which consulted `is_safe` (support: digit
  admissibility against row, column, and box).
- After: one `solveSudoku(board, empty=None)` with the blank-cell collection
  scan in the `empty is None` first-call branch, the depth-first fill
  re-entering the same method (once from the first-call branch to start, then
  once per tentative digit), and the admissibility support inlined as a `safe`
  flag computed by the two scans inside each digit iteration.
- Roles absorbed: blank collection (driver), depth-first fill with undo
  (walker), row/column/box admissibility (support).
- Site selection: the corpus's representative write-and-unwind backtracker —
  the site where the merged body must juggle two undo channels (board
  write-back plus stack push/pop) against the recursion, and where the
  entry's driver role is real work (the 9×9 scan) rather than a one-line
  kickoff.
- Shape decisions: the support's multi-return predicate folds into a flag
  computed by the scans because early returns would escape the digit loop
  the recursion lives in; the first call returns after its branch so the
  recursion propagates the success signal through returns, reproducing how
  the driver and the walker used to divide that convention between them; the
  `first_value / 9` and `3 * (row / 3)` era expressions move with the code that
  uses them, keeping the file's arithmetic character intact.

## Cluster C — `python/131_Palindrome_Partitioning.py` (recursive split builder)

- Before: `partition` (driver) created the result and path accumulators and
  delegated to `recurPartition` (recursive split growth with append/pop), which
  consulted `isPalindrome` (support: two-pointer substring check).
- After: one `partition(s, result=None, path=None, start=0)` materializes both
  accumulators in the `result is None` / `path is None` first-call branches,
  carries the cursor in `start`, re-enters the same method to grow the split,
  and inlines the palindrome check as a flag with the two-pointer walk around
  the recursion.
- Roles absorbed: accumulator seeding (driver), split growth with undo
  (walker), substring admissibility (support).
- Site selection: the corpus's accumulator-shaped backtracker — the site
  where the recursion state is long-lived shared structure rather than a
  bounded cursor, so the merged body has to seed two containers and gate on
  both. It is also the site with the smallest arithmetic surface, which keeps
  the cluster set varied.
- Shape decisions: `start` defaults to a plain cursor value and stays ungated
  (its default is the loop-start, not a phase marker), so this site shows the
  mixed case where a defaulted parameter is legitimate cursor state rather
  than a sentinel — the phase selection is carried by the two `None`
  parameters alone.

## Cluster D — `python/351_Android_Unlock_Patterns.py` (keypad pattern walk)

- Before: `numberOfPatterns` (driver) looped the requested lengths with a
  fresh flag array and delegated to `calc_patterns` (recursive cell walk with
  set/recurse/unset), which consulted `is_valid` (support: step admissibility
  through midpoint arithmetic).
- After: one `numberOfPatterns(m, n, used=None, last=-1, length=0)` seeds the
  flag array in the `used is None` first-call branch, re-enters the same
  method once per candidate length and once per walk step, and inlines the
  admissibility support as an `if`/`elif` chain that assigns an `ok` flag at
  the exact spot where the helper call used to stand.
- Roles absorbed: per-length counting (driver), depth-first walk with undo
  (walker), step admissibility (support).
- Site selection: the corpus's flag-walk recursion and the largest merged
  body, with a support predicate whose branches are plain arithmetic rather
  than list/iteration logic, so this cluster records what the pass does when
  the absorbed support is a flat multi-branch predicate.
- Shape decisions: the three defaults exemplify all three sentinel species —
  `None` for the shared flag array, `-1` for the no-previous-digit marker,
  and `0` for the remaining-length counter whose default doubles as the
  completed-walk base case. The inlined predicate keeps its original comments
  (including the file's typo) in place. The recursion arity (per-length plus
  per-cell re-entry, both returning counts upward) matches the original
  two-method collaboration.

## Deliberate structural variation

The four clusters were shaped to vary the merged-body pattern along the axes a
real corpus would vary it, rather than four copies of one template:

| axis | 023 | 037 | 131 | 351 |
| --- | --- | --- | --- | --- |
| sentinel species | `-1` int | `None` list | `None` containers | `None` + `-1` + `0` |
| ungated defaulted cursor | `low` | — | `start` | — |
| self-recursion arity | 2 (halves) | 2 (kickoff + per-digit) | 1 (growth) | 2 (per-length + per-cell) |
| driver work | window widening | 9×9 scan | two materializations | length loop + seeding |
| absorbed support style | copy loop with tail attach | flag over two scans | two-pointer walk | branch chain |
| undo channels | none (functional) | board + stack | path append/pop | flag set/unset |
| return convention per phase | one channel | `None` top / `bool` recursion | one channel | count / unit |

## Site-selection audit trail

The pass covered the classes whose delegation matched the modeled workflow
end-to-end. The recursion-based answers that fell outside it were excluded
for auditable reasons:

- `python/087_Scramble_String.py`: already one method whose recursion keeps
  memoization on a parameter, so there is no three-level chain for the
  sync-back to dissolve; untouched.
- `python/208_Implement_Trie.py`, `python/305_Number_of_Islands_II.py`:
  three-method chains, but iterative ones — the workflow being modeled
  dissolves a recursion's walker, and an iterative walk absorbed into its
  entry would model a different cleanup entirely.
- the two-method backsolvers (e.g. `python/046_Permutations.py` and its
  shape-mates): an entry plus a single helper dissolves into a two-level
  merged body, which is not the end state this pass models, so including them
  would have mixed two different stories into one diff.

No non-solution file participates in the change; within the touched files,
only the merged method regions were rewritten, and each file's surrounding
commentary and formatting are carried over unchanged.
