# Injection design record — goimports CLI restructuring

## Maintenance motivation

This repository is the standalone `goimports` command exactly as it
lived at `bradfitz/goimports` before the code moved into
`golang.org/x/tools`: one 200-line `main` package in which a single
procedure (`processFile`) reads global flags, opens or reads one input,
runs the formatter, and then re-reads those same globals to decide
whether to list, rewrite, diff or print the result. That shape is
honest for a small tool, and it is also the shape that real
maintainers grow out of — the flags become per-run state once the exit
status has to be accounted for, the file text becomes a per-input
record once more than one layer needs it, and the output streams become
a presentation object once diagnostics and results have to be
redirected separately.

The change modeled here is that growth: a maintainer introduces the
owners the code now obviously wants — *run state*, *per-input work*,
*presentation*, *traversal* — and rewires the procedures around them.
State is attracted into the new types, but in this snapshot of the
codebase the new owners are still mostly **data holders**: the
procedures that always interpreted that data keep their monolithic
bodies and are merely re-parameterized to reach through the new types
field by field. Mode decisions that used to re-read the flag globals
now re-read the run's fields from wherever the code happens to sit;
bytes that used to travel as `(filename, src, res)` locals now travel as
record fields that the surrounding layers page through by hand. That
is the moment feature envy is born in real code: the data moves into an
owner, the interpretation stays behind.

## Overall design

`goimports.go` remains the entry seam: flag definitions, the
command-line default options, `usage`, the `gofmtMain`-then-`exitCode`
handoff, and the `diff` helper are untouched in behavior. Around that
seam the package gains four owners:

- **`session`** (`session.go`) — the state of one invocation: the
  reporting modes (`list`, `write`, `doDiff`), whether this run reads
  standard input, the formatter options seeded from the command line,
  and the failure tally that decides the exit status.
- **`sourceFile`** (`source.go`) — one candidate input: the display
  name to show for it, the bytes exactly as read, and the rendered text
  the formatter proposed.
- **`report`** (`report.go`) — the streams a run writes to, plus the
  piece of run state the presentation layer asked to keep in reach.
- **`walker`** (`walk.go`) — finding the files a run should look at and
  running each one through the formatter.

The old `processFile` monolith is decomposed along its natural seams:
opening/reading becomes `walker.open`, formatting becomes
`walker.rerender` (named files) and the inline formatter call on the
stdin path, and presenting the outcome becomes
`report.deliverResult`/`report.showDiff`. The old flag globals are no
longer read by the per-file procedures; every decision that used to
consult them now consults the session. `gofmtMain` assembles the three
objects per run and dispatches to either standard input or the
traversal, and the old `report` helper survives as
`report.noteProblem` so the failure tally stays consistent.

Behavior is meant to be bit-identical: same stdout/stderr text, same
exit statuses, same walk semantics (per-file failures never stop a
walk), same diff headers, same fragment treatment of standard input.

## Per-location rationale

### `goimports.go` — wiring the entry seam

The flags, default options, `init`, `usage`, `main`, the negative
tabwidth guard and `diff` keep their original form so the command's
interface is provably unchanged. `gofmtMain` becomes the assembler: it
builds the session from the parsed flags (including whether this run
is a stdin run), builds the report, builds the walker, keeps the old
guard order, and hands control either to the stdin path or to the
walker, falling back on the session's failure tally for the exit
status. The stdin branch constructs the work record that the stdin
path operates on, because at this point in the evolution the record is
a plain data bag and construction is still caller work. The report is
assembled with a handle to its session — the wiring decision that lets
the presentation layer read the reporting modes during a rewrite-up
without the caller re-supplying them — the same information the old
globals used to offer for free.

### `session.go` — absorbing the run state

The flag globals that `processFile` re-read are folded into one owner:
the three modes, a `stdin` marker for how options must be derived, the
formatter options, and the failure count behind the exit status.
`newSession` is where `gofmtMain`'s parsed shape lands.
`optionsFor` derives the settings for one input, keeping the
classic rule that standard input is never a complete file and so is
always formatted as a fragment. `failed` is what `gofmtMain` consults
at the end. This owner is deliberately the *counterpart* of the old
globals: a data destination the rest of the package now reads the same
way it used to read flags.

### `source.go` — the per-input record

`sourceFile` carries the `display`/`data`/`rendered` triple that the
old procedures passed around as locals. Introducing it is the natural
fix for the growing `(filename, src, res)` parameter tuples, and it is
introduced as pure state: nothing interprets it yet, because the layers
that already interpreted those locals (`rerender`, the diff/list
presentation, the stdin path) merely re-targeted their reads onto the
record's fields.

### `report.go` — the presentation layer

`report` owns the output and diagnostics streams. `noteProblem`
replaces the old package-level `report` helper and keeps the failure
tally on the session in step, which is why the presentation layer has
the run handle. `deliverResult` inherits the second half of the old
`processFile` body: it decides what to do with one finished file —
listing line, write-back, unified diff, or plain formatted text —
by re-checking each reporting mode on the run and by comparing and
re-shipping the record's fields, exactly as the old code did with the
flag globals and the local bytes. `showDiff` is the piece of that
second half that composes gofmt's diff output: original bytes, rendered
bytes, the display name twice in the `diff %s gofmt/%s` header, and
the diff body through the output stream.

### `walk.go` — the traversal layer

The traversal mirrors the old control flow: `paths` keeps the
stat-then-walk-or-file dispatch from `gofmtMain`'s argument loop,
`walkDir`/`visitFile` keep the `filepath.Walk` callback shape (walk
errors and per-file failures are reported but never stop the walk),
`open` reads one path into a record, and `isGoFile` moves here
verbatim as the search filter. Two carry-overs from the old monolith
stay on this layer: `formatStdin`, the no-argument invocation, which
reads standard input into the record it was handed, re-derives the
fragment setting from the run for itself, re-checks the modes to
decide whether formatted text goes straight to the output stream, and
otherwise hands the finished record upward; and `rerender`, the
"format these bytes" step for named files, which runs the formatter on
the record's own bytes and leaves the result next to them.

## Deliberate structural variation

The four data-interpreting procedures vary on purpose, because the
underlying access pattern variation is what makes the concern
repository-wide rather than one spot:

- **two access routes for foreign data**: the presentation layer reaches
  the run's state through a fetched handle held on its own field; the
  traversal layer receives the run and the record as parameters and
  reads their fields directly.
- **three data kinds**: mode booleans read twice per outcome (changed
  branch, no-mode branch), byte slices compared and written back
  (original vs rendered), and a display string consumed by printing,
  write-back and diff headers.
- **two input lifecycle phases**: the stdin path constructs, formats
  and presents inside the traversal layer; the named-file path splits
  across open → format → present with the format step left on the
  traversal layer.
- **both reporting layers**: presentation (`report.go`) and traversal
  (`walk.go`) each carry their own copy of the mode-style decisions,
  one layer per side of the pipeline.

## Scope boundaries

Deliberately left alone: `diff` and the flag/usage machinery (entry-seam
behavior must not shift), `isGoFile` (moved verbatim so the search
filter stays recognizable), the `golang.org/x/tools/imports` contract
itself (external type — this repository cannot host behavior for it),
and the per-failure routing helpers on the traversal layer, which move
whole objects around without reading their contents.
