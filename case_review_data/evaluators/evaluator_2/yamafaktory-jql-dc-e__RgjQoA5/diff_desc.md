# Injection design record — jql, parameter groups threaded instead of bundled

## Maintenance motivation

jql's front end has one record — the parsed clap `Args` — that owns everything the
user asked for, and the runner has one object — `Evaluator` — that owns the scratch
space and the input a tape build consumes. Both ownership lines predate the change
below. The scenario modeled here is the ordinary middle of a refactor rather than a
finished feature: two contributors, working a few days apart, reach inside those
owners and start handing the pieces around themselves, each with an intention they
have not yet carried to the end.

The first contributor is preparing the front end's inner functions to be driven by
something other than the clap record. `process_json` and `format_json` take
`args: &Args` today, which drags the entire command-line surface — file paths, stream
flags, the query text — into functions that only ever read four fields of it. The
planned end state is a small driving surface built from plain options, and the
intended first step is mechanical: replace the record parameter with exactly the
options each step consumes, as individually named parameters, everywhere the pair
is called. The commit lands at that step. The next one, introducing the options
record those signatures actually want, does not exist yet.

The second contributor is hardening the tape build against escaped input. A simd-json
tape is built over a mutable byte slice that the builder mutates in place; when the
input contains a backslash escape the bytes must also be copied into scratch space
first, and the decline scan must still read the pristine input afterwards. Both build
modes currently live inside `Evaluator::evaluate`, interleaved with the branch that
chooses between them. The plan is to lift each mode into its own named function so the
cost of the escaped-input copy can be measured separately from the zero-copy path, and
to settle the ownership afterwards. The functions are carved out before that settling,
so they arrive as free functions that take the evaluator's pieces as parameters.

## Overall design of the change

The change touches two production files, both outside the frozen `Value`-based
evaluator, and keeps every observable byte the same: the same queries render
byte-for-byte identically, the same inputs take the tape path as before, the same
inputs decline to the runner, and stream mode still reuses scratch space across
lines. No public signature in `jql-runner` or `jql-parser` moves. The two work areas
share the same shape — data that one owner used to hold now arrives as loose,
individually named parameters — but they model the shape at two different depths:
the CLI options are plain copied booleans, while the tape-build context mixes a
mutable scratch owner, a read-only token slice, and the input document held under
two different borrow modes on the two sides of a branch.

## Cluster — the CLI front end (`crates/jql/src/main.rs`)

`process_json` previously received `args: &Args`; it now receives the query, the
`validate` flag, the three output options `inline`, `raw_string`, `sort_keys`, and the
evaluator. `format_json` changes in step, from `args: &Args` to the three output
options alone, since those are the only fields it reads. The validation short-circuit
keeps its place inside `process_json`, so `validate` joins the same signature without
being read anywhere down the call chain.

`main` adapts at all three of its input paths — file, line-by-line stream, piped
stdin — by spelling out `args.validate`, `args.inline`, `args.raw_string`, and
`args.sort_keys` at each `process_json` call site. The two unit tests beside `main`
restate the same options by hand. This site was chosen because it is the point where
a record's fields already exist as named locals (`args.inline` and friends), so
threading them individually is the least-effort mechanical step a contributor would
actually take, and because the front end is the one place in the workspace where the
same option list is walked across both a routing step (`process_json`) and a leaf
step (`format_json`) plus every caller of the pair. The production role the options
play — choosing the rendered form of every output line — is unchanged; only their
container during transit is.

## Cluster — the tape build (`crates/jql-runner/src/lazy.rs`)

The tail of `Evaluator::evaluate` that built a tape inline now dispatches to two
free functions: `tape_in_place(buffers, tokens, json)` for input with no backslash
escape, keeping the zero-copy in-place build, and `tape_escaped(buffers, tokens,
json, copy)` for escaped input, building over a freshly allocated scratch copy while
holding the pristine input read-only beside it. The choice between them stays in
`evaluate`, behind the same escape scan as before, so which inputs take which path
does not move.

Supporting the pair, the former `Evaluator::declined` method — which reads the
pristine input to record document starts for the runner fallback — becomes a free
function next to them, now taking the buffers it previously found on `self`; and the
gating that decides whether a taped outcome can be returned, or must fall back to
the runner, moves into `taped_outcome`. This keeps each sibling function a complete,
separately measurable step: build the tape, wrap or decline.

This site was chosen because the pieces threaded here are heterogeneous on purpose:
`buffers` is mutable scratch the caller owns and the builder mutates through,
`tokens` is a read-only slice already fetched from `self`, and `json` is mutable on
the in-place side and read-only on the escaped side, where the scratch copy joins it
as an extra parameter. A helper pair was the natural extraction target because the
branch already existed inside `evaluate`, and threading the pieces was the natural
way to carve them out ahead of the intended ownership settle. The evaluator itself
keeps its role: it still owns the buffers across a whole stream and still decides
which inputs it declines; the change only moves the two build steps out from behind
that decision without finishing the structure they were meant to grow into.

## Shape of the rest of the tree

The frozen `Value`-based evaluator, the parser, and every public signature are
untouched; the change stays inside the two work areas described above.
