# Please untangle the interactive lab programs

We maintain the interactive console exercises for the course (the lab
programs under `src/`). Somewhere in last semester's late-night fixes they
stopped being pleasant to work on, and we finally have a window to clean
them up. Concretely, what's bugging us:

- Opening several of them, you can no longer see where `main` ends and the
  individual menu behaviors begin. A couple of the drivers have grown past a
  hundred lines, with whole interactive flows buried in the dispatch switch
  and if-chains. Last month one of us adjusted a single option's output,
  had to re-read nearly the entire `main` to find it, and still nearly broke
  the option next to it.
- The prompts feel inconsistent from lab to lab — reading a number from the
  student seems to be done several different ways in different drivers,
  even though the tree has always provided shared input helpers for exactly
  this. We suspect people hand-rolled a "quick" version and it stuck.
- At least one file still defines a function nobody calls anymore, and in at
  least one driver the same logic appears twice — once inlined where it's
  used, once in a function that other options still call normally. Nobody
  trusts which copy is the real one anymore, which is exactly how the next
  edit regresses.

We'd like the interactive lab drivers back in the shape the rest of the
course material expects: each console should read as a clear menu loop with
the per-command work organized so a maintainer can find and change one
command's behavior without dissecting the whole driver, prompt handling
should be consistent across the labs (stand on what the tree already
provides rather than fresh hand-rolled copies), and the files should end up
tidy — no duplicated logic sitting next to its own copy, nothing orphaned
behind.

Scope: the interactive exercises in the lab03 through lab09 families. The
lecture demonstration programs under `src/lecture` are not part of this
cleanup (one of them, the old `gets()` sample, already fails to build with
modern compilers — that predates all of this; the exercise targets
themselves build fine).

Hard requirements — please take these seriously:

- Not one byte of student-visible output may change: menus, banners,
  prompts, status messages, and exit codes must stay identical for the same
  input session, including what each program does on EOF. If you want
  confidence, drive a driver with the same scripted input before and after
  your change and compare.
- These exercises intentionally contain unsafe C patterns — missing bounds
  checks, format-string usage, deliberate stack quirks. That is the course
  material. Do not harden, sanitize, or "improve" any of it.
- Everything must keep building and running under the existing 32-bit
  course setup (`cmake -S . -B build && cmake --build build -j4`, exercised
  the usual way).

And please treat it as one cleanup: investigate the affected drivers as a
family and fix every one of them, not just the first that looks bad — we'd
rather not discover the same mess again next semester.
