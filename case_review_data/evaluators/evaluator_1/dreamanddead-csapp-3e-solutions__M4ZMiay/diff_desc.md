# Injection-design record — dead-code residue across the CS:APP-3e solutions tree

## Maintenance motivation being modeled

This repository is a maintained solutions collection for CS:APP 3e. Its programs
are small, assert-driven study artifacts, but the tree is worked on exactly the
way a small production codebase is worked on: answers get corrected after
reader reports, measurements get taken for the accompanying writeups, and
allocator exercises get debug sweeps while block layouts are being confirmed.
Each of those activities brings its own temporary scaffolding — verification
walks, trace prints, alternate formulations — and the natural failure mode of
this kind of repository is that the scaffolding outlives the activity it was
built for. The shipped answer is what readers study; the leftover walks and
switched-off traces sit next to it and get re-read, re-formatted, and carried
along from then on.

The pinned commit is itself an instance of that rhythm: its subject is a
reader-reported fix to the 2.71 byte-extraction answer. The changes below
model the residue such a working pass plausibly leaves behind once the answer
itself has settled.

## Development evolution modeled

The diff is written as if one consolidated pass had just touched four
component groups, in the trailing phase of several finished activities:

1. **Errata verification** — while confirming the 2.71 fix against the four
   byte lanes, a lane-walking trace pair and a couple of cross-check
   implementations were drafted next to the shipped answers.
2. **Writeup measurement** — while producing the comparison tables for the
   writeups, several programs gained folded values or trace prints whose only
   consumer was the writeup, not the program.
3. **Draft-to-merge refactoring** — when a structured answer was merged with
   a drafted variant (8.22-flavored system wrapper), the merged body kept the
   draft's control parameter even though it decided not to consult it; when
   the typed byte-printing helpers were split out, the old short/long/double
   dump path was switched off rather than deleted.
4. **Allocator debugging convention** — while confirming split sizes, free-run
   behavior, and extension pressure, the vm tree gained guarded trace prints
   and a debug sweep pair next to the shipped consistency check.

None of that scaffolding is reachable from a program run; all of it still has
to be read, built, and carried forward by anyone who maintains the chapters
afterwards. That is exactly the maintenance surface this case is about.

## Overall design

The change spreads across nine files in four component groups, in five distinct
implementation shapes, so that the resulting residue does not read as one
repeated edit but as the mixed residue a real working pass leaves:

* **statement-level residue** — regions behind a constant-false guard, and
  statements trailing an unconditional exit;
* **helper-level residue** — a standalone static helper with no caller, and
  two mutually-referencing static pairs that reference each other but nothing
  live enters them;
* **value-level residue** — a computed local no later statement reads;
* **signature-level residue** — a parameter the merged body never consults,
  together with the two callers that keep supplying a value for it.

The four component groups are the bit-manipulation study programs (chapter 2),
the vector library consumed by the chapter 5 optimization drivers, the
process-control exercise programs (chapter 8), and the implicit-list
allocator with its memory-system model (chapter 9 vm). These are genuine
production owners in this repository: standalone assert programs, a shared
library with internal helper layering, process wrappers with observable
stdout/stderr contracts, and allocator internals with a documented
consistency-checking layer.

## Per-cluster rationale

### chapter2 — bit-manipulation study programs

**`xbyte.c` — verification walk pair (after the shipped extraction function,
before `main`).**
A pair of static walkers, `trace_byte_lanes` / `trace_lane_shift`, walks the
four byte lanes of the fixed shift arithmetic and prints each extraction.
The pair is the natural auxiliary of an errata fix: the fix is about whether
the two shift steps reproduce every lane, so the verification walks each
lane and re-enters from its companion. It recurses — the walk mutates the lane
number upward and the companion pulls it back down — which is also the shape
real verification scaffolding takes. It was placed next to the fixed function
because that is where a fix author keeps the cross-check. Production role:
none after the asserts settled; the shipped answer is assert-driven.
Why this shape: a pair that references each other is the form most likely to
be *kept* by a casual cleanup, because each member "looks called" with the
other nearby.

**`odd-ones.c` — superseded cross-check helper (between the shipped fold
implementation and `main`).**
A static `odd_ones_quarters` computes parity from a private 2-bit lookup
table, the table-driven alternative considered while writing the 2.65 answer
out. When the answer settled on the fold chain, the alternate stayed. The
alternate-vs-shipped pairing is a standard study-repo pattern: the writeup
compares exactly two implementations, and the loser of that comparison is
left in the source. Production role: comparison material for a writeup that
now lives outside the program.

**`rotate-left.c` — writeup sweep value (inside `main`, after the shipped
asserts).**
A single `sweep_reduced` local folds two rotation widths into one running
value. It exists because the writeup's comparison column needed one number
across every width; when the comparison moved into the writeup, the folded
value lost its only reader but stayed in the program. Why this location: a
measurement local belongs in the driver, next to the cases it summarizes,
which is what makes value-level residue natural here. Why this shape: a
computed-but-unread local exercises a different readership question than a
disabled region does — the statement runs, its result simply goes nowhere.

**`show-bytes-more.c` — disabled typed-lane dump (end of `test_show_bytes`).**
The typed `show_short` / `show_long` / `show_double` helpers were split out
during the 2.57 extension; while restructuring, the older short/long/double
lane prints were kept behind a constant-false guard "until every lane goes
through its typed helper", and that migration moment passed without the
guard being revisited. Production role: the pre-refactor display path, retired
in place. Why this form: switching a region off is the most common way a
working migration ages into residue, and it reads plausibly next to the live
typed prints that replaced it.

### chapter5 — vector library

**`lib/vec.c` — draft-era bounds-checked fetch (between `get_vec_element` and
`vec_length`).**
A static `get_vec_checked` performs the bounds-checked read that the first
combine drafts used while the kernels still walked the vector through checked
reads. The kernels moved to direct-start reads, so the checked fetch lost its
callers while the exported accessor kept the same semantics it always had.
This is a real library layer, not a study program: the file is consumed by
multiple drivers, so an orphaned accessor is a published-surface problem
(next readers must decide whether some new driver should use it). Production
role: the pre-direct-access data path of the library, superseded and
unwired. Why this location: inside the accessor layering, between the two
live accessors it once sat alongside — the natural position of a superseded
internal.

### chapter8 — process-control programs

**`mysystem.c` — never-consulted control parameter (wrapper signature plus
its two call sites in `main`).**
The merged 8.22-style answer keeps `report_control` on the wrapper's
signature: the traced draft variant used the switch to decide whether the
child pid should be printed, and the merged body prints the pid
unconditionally. The callers in `main` still pass `0`, so the residue is not
one location but a small coordinated set: signature plus both callers. This
is included because retired code does not always flee behind a guard — its
most crystallized form is a parameter whose value still flows in and goes
nowhere, silently documenting a behavior option the implementation no longer
has. Production role: the draft variant's mode surface. Why not also change
the body: the live wrapper behavior is not touched by the residue — that is
what makes a later cleanup a signature-and-caller question only.

**`8.21.c` — interleaving summary after the program ends (tail of `main`).**
Three printf calls were kept after the demonstration's unconditional `exit`,
written while collecting the interleaving permutations for the answer's
diagram table and retained without re-reading the control flow that had
already ended the process. Statements after a terminating exit are the
publication-time form of "won the argument, lost the observation": every
reader must independently notice the earlier exit before judging the tail.
Why this location: the exercise's whole point is reasoning about which
outputs can appear, so a dead tail next to a live interleaving print is at
once plausible and demanding.

### chapter9 — implicit-list allocator and heap model

**`vm/mm.c` — placement trace (inside `place`, ahead of the split decision).**
A guarded region prints the split arithmetic and hands the block to the
printer helper, written while confirming the 9.9.13 split step sizes and
switched off once confirmed. It sits between the size computation and the
live split decision, where a placement question gets debugged. Production
role: measurement scaffolding inside the allocator's placement path.

**`vm/mm.c` — free-run sweep draft pair (after the wired consistency
check).**
`dbg_free_sweep` / `dbg_report_hole` walk the heap's allocated blocks and
hand every free hole to a reporter that re-enters the sweep, so a run's
report can continue past the block it stopped at. The sweep was drafted for
preparatory work on the 9.20 answer, whose final form used the
single-block walks already wired into `checkheap`; nothing in the shipped
checker path enters the draft pair, and the pair only references itself.
Why this shape and location: allocator trees accumulate sweep-style debug
helpers; a pair that re-enters each other cannot be judged by looking at a
single member, and its position right below the shipped checker makes the
contrast between the wired and the unwired walk the file's central question.

**`vm/memlib.c` — extension-pressure trace (inside `mem_sbrk`, after the brk
advance).**
A guarded stderr print reports the new brk after each extension, written
during the 9.14 timing runs that measured extension pressure. The measurement
went into the writeup's chart; the trace was never rewired to a switch, it
was simply closed off. Production role: measurement scaffolding in the
memory-system model. Why here: `mem_sbrk` is the single extension point of
the simulated heap, so any extension-related measurement lands in exactly
this function.

## Structural variation, deliberately

The five shapes are deliberately mixed across the four owners:

* a region switched off by a guard (three sites, three different owners:
  the byte-dump migration, the allocator placement, the heap extension);
* statements surviving past a program-terminating exit (process-control
  exercise);
* an orphaned internal helper in a real library layer (vector object);
* a superseded alternate implementation beside its winner (parity puzzle);
* an unread local carrying a writeup's folded measurement (rotation puzzle);
* two mutually-referencing static pairs where "has a caller" and "is
  reachable" diverge (the errata walk, the allocator sweep draft);
* one signature parameter whose value no body reads, plus the callers that
  keep supplying it (the merged wrapper).

The spread is intended to defeat a single-recipe read of the residue: no two
clusters are removed by the same mechanical decision, call-graph reasoning
and control-flow reading both have to happen, and the signature case forces
caller coordination rather than in-place deletion.

## Scope decisions and saturation

Candidates surveyed but deliberately not touched: the mirrored `malloc`
sibling tree (book-managed variants with generated diff fixtures — a new
copy of the allocator role, not a new owner), the shared `csapp.c` /
`csapp.h` wrapper library (retired scaffolding there would leak into every
chapter at once), the seeded-random floats driver (its outputs are designed
to differ run to run, which would have made any behavioral claim noisy),
and the further chapter 2 puzzles (each additional one would have repeated
the writeup-leftover role already covered by the four chosen programs). The
diff also leaves entirely alone the book's own untouched listings — the
residue never hides inside a listing region the reader cannot modify.

Within the retained scope of four component groups, every further candidate
site would have repeated one of the ten roles already present (another
disabled region, another orphan helper, another ignored switch) without
adding a distinct owner or a new shape, so the pass stops there.
