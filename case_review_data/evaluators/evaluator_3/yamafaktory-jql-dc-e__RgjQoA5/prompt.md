# Give the parameters that always travel together a proper home

## Repository

jql — a JSON query language CLI (Rust, cargo workspace, edition 2024, MSRV 1.88).
`crates/jql` is the binary; `crates/jql-parser` turns query text into tokens;
`crates/jql-runner` evaluates queries. The runner has two evaluators that must keep
agreeing byte for byte: the original `serde_json::Value` evaluator, and a simd-json
tape evaluator that materializes only what a query selects and declines anything it
cannot answer identically.

## The problem

Production code passes groups of values that always travel together as loose,
individually named parameters, restating the same short list of names at every stop
and every call site along a chain. This is a data clump: the members are really one
piece of data — output options the caller already holds together in a record, build
context an object already owns — so every future member joins by touching the same
long parameter lists again, at every signature and every caller.

Two responsibility areas carry the pattern:

- the CLI front end in the binary crate, where the chain from input reading through
  query processing to result rendering receives the output options as separate
  parameters, spelled out once more at every input path and at the rendering step;
- the tape-evaluation path in the runner, where the steps the lazy evaluator
  delegates the tape build to receive the build context — the scratch buffers, the
  parsed tokens, the input document — as separate parameters instead of reaching
  through the evaluator that holds them.

## What to do

Find every place in the workspace where a group of parameters that always travels
together is received member-by-member at two or more signatures in a chain — not
only where the pattern is loudest — and give each group one fitting, well-designed
owner. A home the data already has is best: a record it is already part of, the
object that already owns the pieces, or a new abstraction only if that is genuinely
the right home for it. Then route every participating signature and every call site
through that owner.

Equivalent designs are acceptable. What matters is that the recurring group stops
being threaded member-by-member — including through a forwarding record whose
fields every caller immediately destructures back into the same loose list — and that
the result reads like an intended ownership, not a renamed parameter list.

## Behavior must not change

- Every rendered byte stays identical: pretty output, inline output, raw-string
  output, key sorting — object key order included.
- Error messages, exit codes, `--validate` semantics, and the one-output-per-line
  stream behavior stay identical.
- The tape evaluator selects and declines exactly the inputs it does today, and the
  Value evaluator keeps answering everything declined to it with identical bytes.
- The public API of the parser and runner crates does not move.
- The original Value-based evaluator is the frozen reference the tape path is
  measured against: do not restructure it.

## Gates the change must keep green

- The full workspace test run.
- `cargo clippy --all-targets --all-features -- -D warnings` on Rust 1.88.
- `cargo +nightly fmt --all --check` (the rustfmt configuration uses nightly-only
  options).
- Workspace rules: no `unsafe`, no new `unwrap`/`expect` in non-test code, public
  items documented, and no suppression attributes used to hide an overlong argument
  list.

## How you will notice you are done

Rendering the same inputs and flags before and after — pretty, inline, sorted keys,
raw strings, validation, a multi-line stream — stays byte-for-byte identical, the
repo's own parity tests say nothing moved, and a maintainer reading the changed
signatures sees the group carried once, behind its owner, at every stop.
