# Cleanup request: retire the remaining fixed-column agenda output pipeline

## Context

gcalcli renders calendar queries through a single calendar interface:
the human calendar display, plus two machine-readable overlays. The
overlays (`--tsv` and `--json`) are assembled today from the details
requested via `--details`, through the composable detail-handler
registry in the event-details machinery: each detail handler
contributes its own columns and keys, so callers choose between
columns rather than between whole row formats.

That handler-driven column assembly replaced an earlier design. Its
predecessor laid every agenda row onto one fixed column table shared
by all consumers: a fixed field order, fixed cell widths, cells padded
to those widths, and both row representations (tab-separated cells and
a keyed JSON object) computed up front for every event. Adding a
column meant rebasing the layout on every consumer at once, which is
why the per-detail handlers were introduced in its place.

The rework is finished — but the predecessor pipeline was never
removed. Its remnants are spread along the output path:

- fixed-width renderer variants living on the calendar interface right
  next to the machine-readable renderers that superseded them, gated
  by the same event filters and sharing their output idioms;
- a row-assembly class of the old design that still lives beside the
  handler machinery that displaced it;
- a small module dedicated to the fixed agenda column layout (field
  order, per-column widths, field evaluation, cell padding);
- escape and padding text helpers inside the shared utility module
  that have no remaining callers outside the old pipeline;
- a tab-separated row emitter on the terminal printer that only old,
  pre-padded output ever used.

No command, flag, configuration, or test dispatches to any of it. It
is not fully dormant-looking, though: live modules still execute
imports of some of these names and of the layout module itself, so
parts of the old pipeline still execute their definitions at import
time even though nothing can ever arrive at them.

## The task

Finish the removal across the whole output path so that the output
code again contains exactly one row format per rendering goal.
Delete the retired pipeline, all of it, everywhere it left something
behind: the renderer variants, the retired row builder, the layout,
the stranded utilities, and the printer hook. Sweep behind it for
things that exist only in its service — import statements that now
bind nothing, helper or formatting constants whose only consumers
were inside the old code, and any leftover scaffolding of the old
format — and remove those too. A later maintainer reading the output
path should find no second row format, and no vestige dedicated to
one.

Telling the retired pipeline apart from the live one is the core of
this job, and the two look deliberately alike. Judge by reachability,
not by shape: trace what the supported entry points, the query
dispatch, the handler registry, and the test suite can actually
reach. Do not trust appearances in either direction. Some live code
in this package is wired dynamically (attribute dispatch,
import-time registries), so a naive call-site scan will misread parts
of the live output machinery too. The decisive question for anything
you find is: can any supported execution path, from the console entry
point or from the tests, actually arrive at this definition?

## What must remain exactly as it is

- The current `--tsv` and `--json` overlays as they behave today:
  handler-driven columns, keys, escaping, and the bracket/row idiom
  of the machine-readable renderers.
- The human calendar display, month and week grids, colors, weekend
  handling, reminders.
- The CLI surface: no flags, defaults, parser wiring, or config
  behavior is added, removed, or changed.
- The detail-handler registry, every handler class in it, and the
  import-time behavior of the details module.
- The full test suite outcome (it must pass) and package imports
  (importing gcalcli must keep working exactly as before).

## Definition of done

- No definition from the retired pipeline remains — not on the
  calendar interface, not in the details machinery, not in the
  utilities, not on the printer, and not anywhere else it has
  collected along the output path.
- Nothing remains that exists only to serve it: stale import
  bindings, format constants, or helpers whose only consumers were
  the removed code.
- The remaining output code reads as one system per goal: display
  output, tab-separated overlay, JSON overlay; there is no second,
  unused implementation of any of them.
- The repository's complete test suite passes without modification,
  and every supported behavior is unchanged.
