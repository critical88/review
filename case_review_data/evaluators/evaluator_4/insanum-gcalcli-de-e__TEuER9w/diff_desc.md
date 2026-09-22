# Injection design record — retired fixed-column agenda pipeline in gcalcli

## Maintenance motivation and modeled evolution

gcalcli renders calendar queries in three shapes from one interface:
the human calendar display, and the two machine-readable overlays that
`--details` pulls in through the composable detail-handler
registry in `gcalcli/details.py` (`HANDLERS`). This tree carries real
work in that direction: the `--tsv` row assembly and `--json` entries
are built handler-by-handler (`fieldnames` per detail, columns selected
by `--details`), superseding the previous arrangement where every agenda
row was laid out against one fixed column table shared by every
consumer of the output.

The evolution being modeled is the interval right after such a
rework ships: the predecessor pipeline — fixed field order, fixed
cell widths, cells padded to shared columns, both row forms computed
up front and picked by the renderer — is stopped being dispatched to
but not yet deleted. This is the classic post-migration window. The
retired pipeline lingers in the places its author last touched it: a
dedicated layout module, a row builder beside the registry that
displaced it, fixed-layout renderer variants sitting next to the live
handlers on the calendar interface, text helpers inside the shared
utils grab-bag, and a row emitter on the terminal printer. The code
still looks maintained: names match its domain, docstrings explain what
it is a predecessor *of*, and live modules import parts of it, so all
of it appears wired in even though no supported configuration reaches
any of it.

The practical maintenance cost modeled here is the one that makes such
leftovers expensive: the retired pipeline still claims the same nouns
as the live one (agenda rows, TSV output, JSON output, columns,
fields), so every future change to output formatting forces a
maintainer to decide which of the two parallel vocabularies a given
symbol belongs to, and imports of retired names keep executing module
code on every startup even though nothing can arrive at the
definitions behind them.

## Overall injection design

The injection adds one complete, internally coherent fixed-column
output pipeline and spreads it across the real owners of the output
path so the removal task is composed of five genuinely related
production clusters rather than one sacrificial method:

- `gcalcli/columns.py` (new file): the fixed agenda column layout —
  the field order, the per-column display widths, the evaluation of
  one fixed column for an event, and padding of cells to width.
- `gcalcli/gcal.py`: two dead fixed-width renderer entry points on
  `GoogleCalendarInterface`, plus the shared private row helper they
  call, mirroring how the live machine-readable renderers are used
  through the query dispatch.
- `gcalcli/details.py`: the retired row builder
  (`EventRowBuilder`), placed beside the handler machinery that made
  it obsolete.
- `gcalcli/utils.py`: the retired cell-text helpers (a TSV field
  escaper and a cell padder) among the live date/text helpers.
- `gcalcli/printer.py`: the retired tab-separated row emitter on
  `Printer`.

Two live modules grow import lines while no live code path grows a
call: `gcalcli/gcal.py` imports the layout and the row builder, and
`gcalcli/details.py` imports the escape helper. In real Python this is
exactly what a half-finished migration looks like — the wiring that
was supposed to route output through the new path was removed, the
imports were left for a follow-up that quietly never happened.

Nothing added changes user-visible behavior: no CLI flag, parser
default, config key, or dispatch condition is added or edited, and the
added code is only ever *defined*, never reached, from any supported
program path.

## Per-cluster rationale

### `gcalcli/columns.py` — fixed agenda column layout (new module)

A module is where this pipeline would really live: the fixed layout is
a self-contained concern (field order, widths, evaluation, padding)
that predates the handler registry's composable fieldnames. Keeping it
as its own module models the typical "the old format module nobody
listed for deletion" outcome of a migration.

- `FixedColumnLayout` owns the shared column widths and exposes the
  padded header, the padded row for one event, and the width of a
  single column — the three queries a fixed-width renderer needs.
- `AGENDA_FIELDS`/`AGENDA_WIDTHS` record the fixed field order and
  widths. The comment states why that rigidity was the format's
  undoing: adding a column meant rebasing the table on every consumer
  at once, which is precisely the pressure that produced the
  handler-derived fieldnames in `gcalcli/details.py`.
- `event_field` evaluates one fixed agenda column for an event
  (start/end dates, their times with am/pm or military handling,
  title), reusing the live date formatting helpers `agenda_time_fmt`,
  `is_all_day`, and `_valid_title`. Reusing real helpers instead of
  inventing look-alikes is both what a predecessor module would have
  done (it predates and shares vocabulary with the details module) and
  what makes its retired status a reachability fact rather than
  something visible from the function bodies alone.
- The module imports live helpers and is itself imported by live
  `gcalcli/gcal.py`, so its top level still executes and the module
  never becomes an orphan file — the opposite-of-a-hint that keeps the
  removal a liveness decision rather than a file-listing.

Role: the format definition of the retired pipeline; the part a
maintainer must be able to name as live-or-dead without touching the
other four clusters.

### `gcalcli/gcal.py` — fixed-width renderer variants on the calendar interface

`GoogleCalendarInterface` is where every query shape is rendered
(`_tsv` for the tab-separated overlay, `_json` for JSON, the grid
displayers for humans). The retired pipeline belongs to that boundary
too:

- `_tsv_fixed_width` renders the agenda as fixed-width
  tab-separated rows: padded header, a `-`-rule whose widths come from
  the layout, and per-event rows, gated by the same
  `ignore_started`/`ignore_declined` filters its live sibling uses.
- `_json_fixed_width` renders fixed-layout JSON objects, with the same
  bracket/first/loop idiom as the live JSON renderer so the two read as
  consecutive generations of one code path.
- `_agenda_row` is the private helper both renderers share: it forms
  the TSV cells and the keyed JSON object for one event in a single
  pass. Its docstring preserves the design compromise that justified
  the fixed pipeline (both representations formed up front, renderer
  picks its form later).

Placement directly after the live `_tsv`/`_json` pair imitates the
natural sedimentation order of a real migration: the replacements were
written right below each predecessor. The import line additions
(`FixedColumnLayout`, `EventRowBuilder`) place this module in the
module import graph twice — graph edges without reference-flow.

Role: retired entry points. Their position beside live namesakes is
deliberate: the live `_tsv`/`_json` pair and the retired fixed-width
pair must be told apart by what can reach them, not by their names or
bodies — the twins differ only in dispatch.

### `gcalcli/details.py` — retired row builder beside its replacement

`EventRowBuilder` is the row assembler of the fixed pipeline and sits
in the details module because row assembly is that module's concern;
keeping it directly beside the handler classes and the `HANDLERS`
registry that replaced it models the way superseded machinery usually
survives — in the file that owns the concept, where nobody expects
dead weight.

- The class docstring names what made the fixed format rigid: a row
  always carried every fixed agenda column. Its `FIELDS` tuple keeps a
  second, hand-maintained copy of the field order, with a comment that
  it is kept in step with the columns module by hand. Duplicating that
  list is a genuine legacy compromise (the two modules never shared one
  definition), and it doubles the number of places a maintainer must
  consult before changing agenda columns.
- `cells_for` evaluates the fixed columns for one event, reusing the
  same live helpers the columns module uses (`agenda_time_fmt`,
  `is_all_day`, `FMT_DATE`, `_valid_title`). It is a self-contained
  evaluation of the old format, not a wrapper around new machinery.
- `build_tsv` returns escaped fixed-layout cells for tab-separated
  output; `build_json` returns the fixed-layout row keyed by its
  fieldnames. The two are exactly the pair the `_agenda_row` helper
  consumes.
- The module's utils import line grows one name for the escape helper —
  the deferred cleanup detail that follows the class wherever it
  lives.

Role: the retired counterpart of the registry-driven row assembly one
screen above it — the clearest single place where the old and new
ownership of "which columns does an agenda row carry" sit in conflict.

### `gcalcli/utils.py` — retired cell-text helpers

`tsv_escape_field` (escape a cell so tab-separated rows round-trip:
backslashes first, then tabs and newlines) and `pad_cell` (fit a cell
into exactly its fixed column width) are module-level text helpers.
They sit next to the live agenda/date helpers as the surviving
dependency tail of the retired pipeline: helpers outliving their last
caller is the single most common shape of dead production code in
long-lived utility modules.

- `tsv_escape_field` has real content rather than filler: the
  backslash-first order is the actual correctness constraint for
  escaping.
- `pad_cell` is also real: truncation vs. right-padding to a fixed
  width, the two cases any fixed-column layout needs.

Role: the "stranded half of a pair" topology. The live helper it sits
next to (`agenda_time_fmt`) looks exactly as plausible on every axis —
location, naming, docstring — so only reachability distinguishes which
of the neighboring module functions still has a role.

### `gcalcli/printer.py` — retired row emitter

`Printer.tsv_line` writes one tab-separated agenda row with the
printer's color treatment, styled and placed alongside the live
`msg`/`err_msg`/`art_msg` emitters. Its docstring explains the one
plausible reason it looks unfinished-belonging ("rows arrive as
pre-padded cells ready for the shared column widths") — the format
detail that only the retired pipeline satisfies.

Role: the output-tier stub of the retired pipeline. It ties the
cluster to the terminal-output class without whose removal the cleanup
would be incomplete, and it exercises the method topology inside the
smell: a public, documented method with not one live caller on any
path from `main`.

## Deliberate structural variation

The retired pipeline is deliberately *not* uniform in shape, because a
half-lived leftover wouldn't be:

- **Reference topologies differ.** Some added definitions have no
  reference of any kind anywhere (the orphaned endpoint symbols);
  others are referenced exclusively from scopes that are themselves
  unreachable — the renderers call their private helper, the builder
  calls the layout, the layout calls its own evaluation function; and
  two classes plus two helpers are imported into live modules while no
  live scope ever uses the imported names. A single grep for
  call-sites therefore answers a different question than the one a
  maintainer actually faces: every one of these symbols appears, in
  the right files, with plausible importers or internal callers.
- **Distance from live code differs.** The columns module is textual
  dead-code isolation (a file of its own, but still import-wired);
  `EventRowBuilder` lives in the same module as its replacement; the
  renderer variants live in the same class as their live twins;
  `tsv_line` and the utils helpers are scattered into completely live
  neighborhoods.
- **Kinds differ.** The cluster covers a module-level pair of text
  functions, a class plus its three methods, a second class plus its
  evaluation core, an interface method trio, and an emitter method —
  i.e. every definition kind the rule is claimed to cover appears.
- **Live siblings as near-misses.** The fixed-width renderer triple
  lives directly below the live `_tsv`/`_json` pair and reuses their
  filters and output idioms; the row builder sits under the registry
  that replaced it. The distinction therefore does not survive any
  shortcut that keys on names, docstrings, or shape, and a maintainer
  (or their tooling) that confuses the two pairs would delete working
  output paths rather than the retired ones.
- **Vocabulary is spread.** No single artificial token (no `legend`,
  no `old_` prefix strip) marks the additions; the naming follows the
  domain (columns, rows, builders, cells, fields, emitters), so a
  token-level or heuristically name-driven reduction of the cluster
  would at best be a taxonomy of the whole output subsystem rather
  than of the cluster.

## Boundary decisions

- No CLI/parser/config surface was touched: wiring the retired
  renderers to a flag would have added behavior, and dead comparable
  code reachable through a visible flag would read as a feature
  toggle, not a leftover.
- `gcalcli/exceptions.py` contains one helper that the pinned revision
  itself leaves unreferenced (`raise_one_cal_error`). It was left
  exactly as found: crediting the case with removing it would mix a
  baseline problem into the pipeline.
- No test file was edited: they are the behavior reference for the
  repo and were left as found.
- Other subsystems (ICS import, config parsing, actions registry,
  conflict display) were examined and deliberately not used: no
  credible consumer of fixed agenda columns exists there, so extending
  into them would have broken the one-developer-task boundary rather
  than deepened the pipeline.
