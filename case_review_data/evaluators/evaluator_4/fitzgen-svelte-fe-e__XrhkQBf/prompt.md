# Report-building logic that lives on the wrong types

## Context

This repository is a profiler for WebAssembly binary sizes. The `ir` crate owns the data
model: after parsing, the whole binary is an items graph, and the types in that crate hold
everything known about items — their sizes, retained sizes, names, neighbors, and the graph
structure itself. The `analyze` crate turns that graph into reports: each analysis (the top
listing, the dominators tree, the monomorphization report, and the retaining-paths report)
produces text, JSON, and CSV output through its emitter implementation, using small entry or
summary structs for anything that is not purely graph data.

## Observation

A review of the report-building code found a recurring pattern. Several emitters now delegate
their per-item work to methods on the analysis structs, but those methods never touch the
analysis's own fields: everything they read comes from somewhere else. A method on the top
analysis reads only the items graph to produce an item's sizes, shares, and name. Methods on
the dominators tree type read only the items graph to build their text rows, JSON objects,
and CSV records. Methods on the monomorphizations analysis read only the entry struct that
describes one generic function. And the paths analysis now asks itself where traversal should
start, answering entirely from the items graph. The analysis struct in each case contributes
nothing but a receiver.

This is textbook feature envy: behavior whose entire data dependency belongs to another owner.
Knowing an item's shallow and retained size and its share of the binary is a fact about the
items graph, here to serve the top report. Building one dominator tree node's record from its
size, retained size, immediate dominator, and name is a fact about the items graph. Writing one
generic function's bloat bytes, total size, and monomorphization list is a fact about the
entry type. Choosing which items a paths traversal starts from — by name, by regular
expression, or by size order — is a question the items graph answers about itself.

## What we want

Move this behavior so it lives with the data it serves. The data owners — the items graph
type in the `ir` crate, and the per-analysis entry types — should own the operations that
derive report content from their data, and the analyses should consume those operations where
they previously replicated them. Concretely:

- The logic that derives one item's measurements (shallow size, retained size, their shares,
  and the display name) belongs with the items graph data, not inside the top analysis.
- The graph queries that decide traversal order and starting positions (largest-first
  ordering, frontier selection, matching item names) belong with the items graph data.
- The logic that fills one entry's JSON fields or CSV record belongs with the entry type that
  holds those fields.
- Analysis types are free to keep orchestration — iterating their item lists, driving their
  tables and writers, and applying their options — but they should stop hosting data-derived
  logic that never uses the analysis.

Apply this across the whole report-building stage of the analyses — the same pattern exists
in more than one analysis and more than one output format, and this request covers all of
them, not just the examples above. Also tidy up what the move leaves behind: hosting helpers
that no longer earn their place (odd unused receiver parameters included) should not survive
the change.

## What must not change

- Reports stay byte-for-byte identical in every format. The workspace's tests include
  expectation files that compare text, JSON, and CSV output exactly, for each analysis and
  including runs where retained sizes are enabled — keep them passing.
- The retained-size discipline: retained sizes are only meaningful once the analysis has
  asked for them to be computed, and code that is about to use them can expect that this
  has happened for its run; preserve the existing guards and do not move a retained-size
  computation to a place where that expectation breaks.
- The public surface: each analysis's constructor keeps its signature (still returning the
  boxed emitter the CLI drives), the crate boundaries between `ir`, `opt`, and `traits` are
  unchanged, and the text/JSON/CSV emitters remain feature-gated exactly as they are.

## Verifying

Build and run the workspace's full test suite (`cargo build --workspace` then
`cargo test --workspace`) as the final step and make sure everything passes.
