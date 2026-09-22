# sqlite-vec: retire the pre-0.1 leftovers still compiled into the extension

## Where we noticed it

The loadable extension was largely rewritten ahead of the 0.1.0 line: the
scalar quantization, JSON output, rescore chunk storage, IVF and DiskANN
components were rebuilt on the designs we currently ship. While reading the
sources again in preparation for the next release, we found that a handful
of pieces from before that rewrite — and from experiments run while tuning
the current ones — are still being compiled into the extension. Nothing on
the extension's supported surface can ever run them. Concretely, the patterns
we keep tripping over:

* a legacy implementation of behavior we now provide differently, selected
  by an internal switch that is initialized off and that no table option,
  command, or registration path ever sets — the guard in the live function
  *looks* like a runtime decision, but the switch is effectively a constant;
* abandoned drafts and experimental variants that never got wired into the
  shipped flow (an earlier storage encoding, an earlier rendering strategy,
  a seeding routine that lost to the one we use, a maintenance walk that was
  replaced by a rebuild-on-invalidation approach);
* code propped up to look referenced: peer routines that only ever call each
  other, and handler routines living in a file-local table that nothing in
  the shipped extension actually walks.

These hide from the compiler's own unused warnings — always-off guards,
mutually-calling pairs, and table-registered handlers all *appear* to have
references — so deciding what is dead requires reasoning from what the
shipped extension can actually execute, not from what a build happens to
report.

## What we want

This is a dead-code cleanup: remove the leftover implementations entirely,
across the whole extension source. The C sources form one translation unit
(`sqlite-vec.c` textually includes the other component files), so audit every
one of the component files, not just the obvious spots. Areas that were
rebuilt during the rewrite and deserve a close pass: the scalar quantization
behind the SQL functions, the JSON rendering of vector values, the chunk
blob encoding used by the rescore column, the experimental IVF maintenance
code, and the DiskANN graph maintenance surface.

When you find code that no supported use can reach:

* remove it outright together with everything that exists only to sustain
  the appearance that it is alive: its private helpers, spec strings, policy
  tables, depth/count macros, file-local dispatch tables and the struct
  types they use;
* a live function that carries a guard branch selecting a legacy mode is
  itself part of the cleanup — remove the dead branch (and the never-set
  switch it reads), keeping the live function's supported behavior intact;
* do **not** resurrect anything. Don't add options, commands, SQL
  functions, environment variables, or registrations to activate the
  leftovers — the current implementations are the ones we ship.

Keep everything the supported surface can still reach: the current
quantizer, the current JSON builder, the initializer the clustering code
actually calls, the chunk storage path used by inserts and updates, the
incremental edge repair, and everything reachable through any supported
statement, table operation, or command. The experimental IVF code is
compiled only when `SQLITE_VEC_EXPERIMENTAL_IVF_ENABLE` is defined: audit
with that flag on as well as off, and keep whatever the enabled build
legitimately runs — only definitions that stay dead in *both* build states
are in scope for removal.

## What must not change

Observable behavior must be identical: the same SQL functions with the same
results, the same accepted vec0 table options and commands, and the full
test suite passing exactly as before. The extension must build with the
repository's own targets, in both build states of the experimental flag.
When you are done there should be no stranded references to anything you
removed: no lingering declarations, orphaned types or macros, or tables
whose entries are gone.
