# Refactor request: helpers that spend their time on another type's data

## Maintainer observation

This repository - a Go RPC framework with a pre-modules GOPATH layout -
recently got some cleanup in three areas: how error responses are serialized
to JSON, how the descriptor registry behind the code generator indexes
message definitions, and how the protoc plugin resolves package imports and Go
type names. The code works and the behavior is fine. What bugs us is where
some of the extracted helpers sit: they are methods on one type, but nearly
everything their bodies read - fields, getters, nested data - belongs to a
different object handed in as a parameter. Their own receiver is mostly
unused. Practically, reading the code means flipping between two layers: you
open a helper on one type just to find it spends its whole body walking some
other type's data, and every follow-up change has to be traced across that
boundary. Ownership questions like this keep coming up in review, and the
layering between the error path, the descriptor registry, and the plugin now
reads worse than before the cleanup.

## What we'd like you to do

Please straighten out this layering. Data-processing logic should live with
the data it processes: a helper whose body is dominated by access to one
type's data should belong to that type, and its former owner (and every
caller) should be updated consistently - including removing whatever
pass-through plumbing is no longer needed after the move. Where Go's rules or
the surrounding design make a move awkward, thread the necessary state
explicitly instead of leaving a forwarding shim behind.

The problem is not limited to one helper. We noticed the same pattern while
reviewing all three of these areas, so a complete change finds the related
sites and handles them together rather than stopping after the first one:

- the JSON writer that renders an error into the wire-format `{code, msg,
  meta}` payload in the core twirp package;
- the registry the code generator bootstraps message definitions from
  (message naming, nesting, and publicly-imported messages included);
- the protoc plugin's logic for resolving a service method's message
  dependencies and a message's Go type name while it emits generated code.

You will need to judge case by case what the data-owning type is; where logic
genuinely splits its attention, keep it where the design reads best and leave a
note if you kept something deliberately.

## What must remain stable

- Observable behavior is byte-identical: error response bodies (field layout,
  metadata rules, the over-large-message clamp, the fallback body when
  serialization fails) and the `.twirp.go` sources emitted by the plugin for
  the same proto inputs do not change.
- All repository tests keep passing under `go test -race ./...` with the
  pre-modules GOPATH layout this repository pins; do not weaken, skip, or
  delete tests.
- The public API of the core twirp package (the Error type and its
  constructors, the error-response writer used by middleware) stays
  unchanged.
- No comments-only or rename-only reshuffles: the placement of the logic must
  actually change, and no dead helper, unused import, or duplicate copy of
  moved logic is left behind.
