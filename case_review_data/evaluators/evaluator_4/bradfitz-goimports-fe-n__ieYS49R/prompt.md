# goimports housekeeping: the new objects don't own their work

We finally split the old goimports monolith up: one invocation's
command-line state now lives on a run/session object, every input is
carried through the pipeline as a small work record, output goes
through a reporting object, and the filesystem traversal and the
standard-input path got their own traversal object. That was supposed
to be the cleanup.

Now that I'm working in the code, the split keeps fighting me:

- Finishing one file means deciding what the run's reporting modes
  (`-l`, `-w`, `-d`) call for, and those decisions are still written
  out mode by mode, twice per outcome, inside the reporting and
  traversal layers — each time by digging through the run object's
  fields.
- The per-input work record is a dumb bag. Whoever needs "did
  formatting change this one?", "write a gofmt-style diff for it", or
  "format its bytes and keep the result" just reaches into the
  record's fields and does the work themselves, from the outside.
- The standard-input path keeps private copies of rules that already
  belong to the run: how formatter options are derived for stdin, and
  what happens when no reporting mode was picked.
- The reporting object even holds a live handle to the run, purely so
  it can keep reading the run's mode fields whenever it needs a
  decision; callers that already have the run still end up supplying
  it again through a different door.

Please put the data back together with its duties. Wherever a layer is
still leafing through another component's fields to make its
decisions, move that behavior next to the data it interprets, so each
component answers questions and performs work over its own state
instead of exposing raw internals to whoever walks by. And please do
the whole reporting-and-traversal side of the pipeline — the same
pattern shows up in both layers, and in more than one place each;
fixing the first spot you trip over is not the job.

Requirements:

- CLI behavior must stay byte-identical in every observable way:
  stdout/stderr text, exit statuses (2 for any input that had to be
  given up on, and for the negative-tabwidth guard), walk semantics
  (a bad file never aborts a directory walk), gofmt's
  `diff %s gofmt/%s` diff headers, and standard input being formatted
  as a fragment under the display name `<standard input>`, with the
  same reporting modes applied to it as to named files.
- No component should keep a handle, parameter or copy of another
  component purely so it can read that component's fields; hand the
  work to the owner instead. Scaffolding that only existed to make
  the reach-through possible should not survive.
- The command-line surface (flags `-l`, `-w`, `-d`, `-e`, argument
  handling, no-argument stdin mode) is the compatibility boundary;
  internal structure is fair game.
