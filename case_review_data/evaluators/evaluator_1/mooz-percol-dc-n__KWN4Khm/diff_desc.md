# Injection design record — data clumps in percol

Repository: `mooz/percol` (Python), pinned at `4b28037e328da3d0fe8165c11b800cbaddcb525e`.
Scope of the change: `percol/finder.py`, `percol/model.py`, `percol/view.py`, `percol/__init__.py`, `percol/command.py`, `percol/cli.py`. 100 insertions, 37 deletions across 11 hunks.

## Motivation

percol filters a candidate list interactively. The unit of work at every stage
boundary is a *result row*: a `(line, match_info, original_index)` 3-tuple that
the finder produces, the model pages and marks, the view paints with keyword
highlighting, and the action dispatcher consumes on finish. The row is born in
`FinderMultiQuery.find`, lands in the model as `self.results`, and trails in
`Percol.execute_action`.

Alongside the row, a small set of *matcher flags* — `case_insensitive`,
`invert_match`, `lazy_finding` — is copied around by hand wherever a `Finder` is
constructed or reconfigured: bulk propagation in `clone_as`, per-flag writes in
`SelectorCommand.specify_case_sensitive` / `toggle_case_sensitive`, per-flag
writes in `cli.set_finder_attribute_from_option`.

The clean tree keeps both of those bundles encapsulated: rows travel as tuples,
flags as attributes. The change reverts that discipline. It is the shape a code
base drifts into when short helper refactors are applied one at a time over
months — each one locally reasonable, none of them ever reconciled.

## The normal evolution this models

1. Someone needs a seam in the finder loop, so the row is unpacked and a small
   row-composition helper is extracted. The helper takes the fields
   `line, res, idx` separately because the caller has just unpacked them and that
   is the path of least resistance.
2. The same extraction is repeated downstream, independently and idiomatically
   per module: the model wants the text field while walking its results, the
   selection paths want a re-shaped row, the dispatcher wants the candidate
   text, the view wants each field for styling and repainting. Each helper is
   written by a different person at a different time, and each one takes the
   row's fields as separate parameters because that is what its immediate
   caller has in hand.
3. The flag bunch goes the same way: `clone_as` needs some flags to build a new
   finder, the toggling commands need to re-apply flags after a switch, the CLI
   needs to apply option-derived flags at startup. Each site grows its own
   means of passing the same group of flags, one flag per parameter.

Nobody designs this all at once. Every step carries its own little
justification, and the aggregate is a code base where the row keeps falling
apart into three loose variables at every boundary and the flag set keeps
being spelled out parameter-by-parameter — with five distinct construction
sites to keep in sync.

## Overall design

The injection threads field values through the existing call graph without
altering observable behavior:

- Where a row-shaped tuple used to be handled as one value, it is broken into
  its `line`/`res`/`idx` components (or the view's `line`/`find_info`/`abs_idx`
  renaming of the same fields), reshipped as separate parameters between
  original and new helpers, and re-assembled only at the ends.
- Where the flag set used to be assigned as three attribute writes at each
  site, the sites now hand all three flags through function parameters to a
  shared application helper.
- `percol/cli.py::set_finder_attribute_from_option` keeps its CLI-option
  reading (including the double negations) and forwards the resulting flag
  values into the shared helper; its own signature is unchanged.

The details that keep behavior identical: `lazy_finding = not options.eager`
and `case_insensitive = not options.case_sensitive` double negations survive
verbatim; result rows remain 3-tuples at the public boundaries so the existing
positional indexing and unpacking keep working; `invert_match` continues to be
ignored for empty queries; the middle field of a *selection* row keeps its
own meaning.

## Cluster-by-cluster design rationale

### A — the `line, res, idx` row thread

Five signatures now demand the row's fields one by one:

- `FinderMultiQuery.compose_row` (finder.py): the finder loop unpacks every
  candidate row and asks the helper to assemble a result row from its parts,
  including the invert-match-filtering decision.
- `SelectorModel.row_line` (model.py): the model's `get_result` path reads the
  results list, splits each row, and follows which of its fields is the text.
- `SelectorModel.selected_row` (model.py): both current-selection extraction
  and mark-selection building funnel the fields of an indexed row through this
  single row-reassembly helper — one helper serving two call shapes.
- `Percol.run_action_row` / `Percol.candidate_text` (percol package): on
  `__exit__`, the dispatcher walks every selected action row, unpacks it, and
  extracts the argument text with the same one-field-at-a-time pattern.

Done across the three layers the row actually crosses (generation → model
selection → action dispatch), the same three loose parameters cross five
signatures belonging to different classes, and no one of them is a copy of
another — each has its own reason to exist where it does.

### A′ — the renamed row thread in the view

- `SelectorView.render_match_highlights` and its fresh client
  `SelectorView.row_style` joined `SelectorView.display_result` in threading
  the *same* row — but under the view's own names: match info is `find_info`,
  the original position is `abs_idx`.

Renaming the variant keeps the presentation layer plausibly self-consistent
(someone working only in this file does not reuse the finder's row names),
which is exactly how parallel helper chains drift apart while carrying the
same data. Structurally this cluster requires a solver to recognize that the
renaming hides the same underlying row — the semantics of the middle field in
the *selection* rows of cluster A and the semantics of the middle field in a
*result* row are not the same concept, and the repair has to respect that.

### B — the matcher flag bunch

- `Finder.configure_matcher`: where `clone_as` used to copy the flag
  attributes onto the new finder, the new helper instead takes every flag as
  its own parameter and writes them; call sites must re-derive the flags.
- `FinderMultiQuery.build_clone`: clone construction now threads its own copy
  of the flag parameters.
- `SelectorCommand.assign_matcher_flags`: the interactive commands that change
  case sensitivity no longer write one attribute; they re-compose all three
  flags (reading two straight from the current finder) only to hand the whole
  bunch back.

A reader has no single place to learn what a flag set is — every site carries
a re-derivation. Callers that only wanted to flip one flag are forced to read
and repeat back all three, which is the coupling that makes the next feature
(a new flag) touch every signature at once.

## Use of the overall call graph

The three clusters are not copies of one pattern; they occupy distinct life
cycle moments of the same domain objects — construction (cluster B at finder
and clone time), mutation (cluster B at command time), construction of derived
rows (clusters A and A′ at generation time), and consumption (clusters A and
A′ at model, view and action dispatch time). The row-field clusters cover the
three layers a row genuinely passes through; the flag cluster covers the three
sites where flags genuinely move. Extending either cluster further (e.g.
threading query/caret/index through the model's selection helpers, or
splitting the tty descriptors in `percol/tty.py`) would have added signatures
carrying concepts that are not the same clump, rather than expanding a genuine
one.

## Compatibility decisions

- Result rows still come back as 3-tuples from `find`/`get_results`, from
  model selection methods, and in `args_for_action`; only the internal
  handling is now field-by-field.
- all matcher/behavior attributes still live as real instance attributes —
  the change only adds parameter-threaded helpers *around* them, never
  replacing them with a struct, so direct writes and reads keep working.
- `--eager`, `--case-sensitive` and `--invert-match` all pass through
  unchanged from the argument parser to the finder instance.
- percol/view.py, percol/command.py and the CLI continue to behave identically
  for empty queries, single-candidate auto-match, quoted/unquoted output
  actions and model switching.
