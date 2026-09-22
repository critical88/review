# Rendering pipeline maintainability: chafa

## Observations

The symbol-mode rendering pipeline in chafa — the path that turns an image
into terminal text — has become genuinely hard to work with. Over several
rounds of performance tuning, the driver routines along this pipeline grew
into sprawling flat monoliths. Steps that used to live in small, separately
readable helper functions now run inline inside their callers: whole loops
that extract per-cell colors, error scoring, palette construction, symbol
map parsing, and per-mode output formatting have been copied directly into
the driver bodies, and the helpers they used to delegate to are gone.

The result is painful in exactly the ways you'd expect. The guard conditions
that helpers used to enforce with early returns are now hoisted into thickets
of nested ifs, some tails run through label jumps, and near-identical copies
of logic sit side by side in one function body. In the map preparation path
and in the output formatting path, and elsewhere along the pipeline, interpreter-like
branch stacks now fold what used to be clearly distinct modes and strategies
into a single wall of code. Reviewing, profiling, or extending any one step
now requires understanding an entire phase at once, and related changes that
should be localized to one helper have to be made coherently inside several
hundred statements.

Concrete example: trying to follow how a work cell ends up with its foreground
and background pens means tracing one huge function that simultaneously does
candidate selection, mean color extraction, error scoring, color quantization,
and fill fallback — formerly five distinct, separately testable steps. Similar
stories hold for building the symbol map, generating the palette, and turning
the finished character grid into terminal escape sequences.

## What we want

Restore a layered, maintainable structure in the file-local implementation of
this pipeline. Identify the code that was flattened into the driver routines
and pull it back out into well-scoped, single-purpose helper functions at a
sensible abstraction level — one helper per responsibility, parameterized
where the inlined body is mode-specific — so each phase of the pipeline once
again reads as a short driver delegating to a readable call tree. Where inline
copies are near-duplicates that differ only in strategy or in swapped
arguments, consolidate them behind shared helpers rather than leaving
parallel copies in place.

Keep the driver routines themselves as readable orchestration code. The goal
is not to move code between files or rename things, but to re-establish the
decomposition inside the implementation files so that each step along the
pipeline is independently readable again — including for stages we did not
describe one by one above.

## Non-negotiables

- Behavior must be preserved exactly: the rendered text, color sequences and
  attribute handling, palette contents, and symbol maps must come out
  byte-identical to what the current code emits — for every color mode,
  symbol selection, fill/invert/fg-only option, thread count, work factor,
  and grid layout, on both the symbol grid and rows-oriented entry points.
- The library's public and internal function signatures must not change, and
  every entry point from outside the pipeline's implementation files must
  keep working unchanged. You may freely introduce, delete, or reorganize
  file-local static helpers as part of the restructuring.
- The wide-cell and rows-oriented rendering paths share helpers with the
  paths being reworked; they must remain correct and must not regress.
- The full test suite must keep passing after the rework.
