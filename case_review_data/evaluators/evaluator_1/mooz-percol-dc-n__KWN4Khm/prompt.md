# percol: the result row and the matcher flags have come apart at the seams

You are working on `percol`, an interactive filtering tool: candidates are
matched against a user query, the matches are displayed with keyword
highlighting, and the final selection is handed to an action on exit
(Repository: `mooz/percol`, Python. The current checkout is the state to work
from; commit `4b28037e328da3d0fe8165c11b800cbaddcb525e`.)

## What I ran into

A match produced by the finder is a small bundle: the original line, the match
information used for highlighting, and the position of the line in the input
collection. That bundle is the unit the whole tool works on — the finder
produces it, the model pages through it and remembers marked lines with it,
the view paints each entry from it, and on `RET` the selected entries are handed
to the configured actions.

While adding a feature I found that this bundle is no longer treated as a
thing anywhere inside the flow. Between the moment a match is created and the
moment it is shown to the user or passed to an action, the code keeps taking
the bundle apart into its three loose pieces, passing the pieces around
individually, and rebuilding a tuple somewhere else. Each helper along the way
— in the matcher loop, in the model's result and mark handling, in the view's
line painting, in the code that runs actions on the selected entries — takes
the same three standalone variables as parameters, as if they were unrelated
values that just happen to always travel together. In the view the pieces even
carry different names than in the rest of the code, so you cannot tell from a
signature that it is the very same bundle being threaded through once more.

The matcher flags have gone the same way. `case_insensitive`, `invert_match`
and `lazy_finding` describe one configuration of the matcher, and they are
grouped conceptually — they are set together from command-line options at
startup, carried together onto new finder instances when the match method is
switched, and re-applied together by the case-toggle commands. But today every
one of those sites hands the flags around as three separate parameters, and the
two interactive commands that only want to flip one flag still restate and
forward all three.

Roughly what this costs:

- I wanted to add a fourth match-mode flag and had to trace five different
  signatures that spell the same flag group out parameter by parameter, plus
  the call sites that re-derive the flags to feed them.
- When a result entry changes shape (I need the match info lazily), the
  pieces are unpacked and re-packed in so many places that I cannot tell which
  parameter of which helper is "the original position" without reading the
  whole pipeline every time. Some of those helpers take a fourth value (like
  "is this entry the current selection" or "should invert-match apply here")
  positioned right after the row pieces, so a reader cannot tell where the row
  ends and the extra state begins.

## Where to look

The row/flag handling spreads across the matching pipeline:
`percol/finder.py` (match generation and finder cloning), `percol/model.py`
(result bookkeeping, current-selection and mark handling), `percol/view.py`
(painting result lines and highlights), the finder/flag-related parts of
`percol/command.py` and `percol/cli.py`, and the action-dispatch exit path in
the `percol` package `__init__`. Not every helper in those files is involved —
part of the job is working out which signatures are genuinely carrying the
same bundle and which just happen to look similar.

## What I want

Please give this data a single home again, so that:

- a result entry is passed around as one value wherever it crosses between the
  finder, the model, the view and the action-dispatch path, instead of being
  dissolved into three parameters at each boundary (including the renamed
  variant used in the view);
- the matcher flags are set, carried and re-applied through one configuration
  path, so that adding or changing a flag means touching the configuration
  once — and the commands that toggle a single flag no longer have to know how
  to re-compose the whole flag group themselves.

## What must stay working

- Observed behavior must not change: matching order, inverting behavior with
  `-v`, case sensitivity, the empty-query case (with an empty query every
  entry is shown), lazy `--eager` behavior, and the exit flow (selected
  entries are given to actions; quoted output unchanged).
- Downstream code should not need changes: anything that consumes matches or
  selections from outside — result entries as produced by the matcher,
  selection entries with their position among results — should keep working
  unchanged, and the matcher flags must remain settable/readable as plain
  attributes on finder instances, because rc files and the CLI set them
  directly.
- Do not rename public options, do not change the terminal rendering logic
  itself, and do not remove existing behavior. Tests in the repository's test
  suite must pass as before.
