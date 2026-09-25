# Injection design record: container wiring diagnostics

## Maintenance motivation

InversifyJS at this commit ships as the v7 facade package: `src/index.ts`
re-exports the published `@inversifyjs/container` and `@inversifyjs/core`
surfaces, and the in-repository production code is the facade itself. Users of
the facade have long asked for a way to *print* what a container is wired to —
which services are registered, under which scope and binding type, which ones
carry lifecycle handlers or name/tag qualifiers — without reaching into the
underlying container's private metadata. Support threads show recurring needs:
a plain-text wiring report for issue reports, quick statistics for release
notes, snapshot comparison before/after a refactor, snapshot files attached to
bug reports, and a cross-check between a recorded snapshot and what a live
container actually has bound.

This task models that feature request landing on the facade. The feature was
implemented as new, additive subsystem code: nothing in the pre-existing
facade surface needed to change except gaining the new exports, so the
container runtime behavior that the repository already provides is untouched.

## Normal development evolution being modeled

The subsystem was written the way features often grow in a hurry. The first
implementer knew the container already exposes binding metadata at import
time (scopes, binding types), so recording a binding as a small data object
was the natural starting point. A `BindingDigest` record captures one
recorded registration (service identifier, scope, binding type, on-activation
and on-deactivation handler presence, name and tag qualifiers), and a
`ContainerSnapshot` record captures one recorded container (label, default
scope, and the sorted digest collection).

Having captured the data, the developer then kept following the request
checklists: "a report …", "statistics …", "compare two …", "serialize …",
"check against a live container". Each checklist item produced a new helper
class surrounding the records, and each helper reached back into the
digests and snapshot fields to do its work — formatting fragments, computing
grouping keys, comparing registration versions, copying fields into payload
objects, and narrating runtime status. The helpers themselves hold almost no
state of their own for this behavior: they were written as pure function-like
classes over other objects' data. This is a pattern maintainers know from
retrospectives: the records ended up as passive data holders, and every
question one can ask about a recorded registration is answered by code in some
other file. Adding a single field to a digest means hunting down the report
formatter, the legend explainer, the statistics calculations, the comparator,
the serializer, and the audit narrator, because the knowledge of what each
field means is scattered across those helpers.

## Overall design

The addition is one coherent facade-owned subsystem under
`src/diagnostics/`:

- **Wiring record classes** — `BindingDigest` (per-registration wiring state,
  constructed from a `BindingDigestDescriptor`) and `ContainerSnapshot`
  (per-collection wiring state, with the digests copied into a readonly
  array). These are the subsystem's data vocabulary.
- **An output port and its default implementation** — the
  `DigestLineWriter` interface (`writeLine`) and `StringDigestLineWriter`,
  so the streamed identifier listing can target any line-oriented sink
  (string accumulation today, a logger or terminal later) without the
  subsystem depending on a specific consumer.
- **Operator classes over the records** — `SnapshotTextReport` (plain text
  report, per-registration row and legend text, streamed identifiers),
  `SnapshotStatistics` (scope/type breakdown with a lifecycle suffix,
  stateful-registration count, qualifier usage tally),
  `SnapshotComparator` (baseline/current difference objects),
  `SnapshotCodec` (JSON text round-trip with payload validation), and
  `WiringAudit` (cross-checks a snapshot against a live `Container` from the
  published package).
- **Public surface** — the diagnostics module barrel and the package facade
  in `src/index.ts` export the subsystem's public types so downstream
  packages can consume `ContainerSnapshot`, `BindingDigest`, the payload and
  writer contracts, and the operator classes directly.
- **Verification** — `src/test/diagnostics/container_diagnostics.test.ts`
  pins the observable outputs: the exact report/row/legend text, streamed
  lines, breakdown keys and counts, qualifier tallies, diff objects, the
  serialize/parse round-trip including rejection of non-payload text, and
  the live-container audit answers.

The house style is respected throughout: class members carry explicit
annotations, interfaces are used for the port and payload shapes, imports are
kept sorted, object keys are sorted, number/boolean interpolation goes through
`.toString()`, and field accessor options of the records are copied through
the constructors (`?? false`, `?? []`) so the records normalize their own
optionality.

## Per-location rationale

### `src/diagnostics/binding_digest.ts` — per-registration wiring record

The digest is the smallest thing the feature is *about*, so the file declares
the record and its constructor-once copying from a descriptor. The descriptor
marks optional inputs as `boolean | undefined`-tolerant so callers may omit
handler or qualifier facts; the record then stores them as stable primitives
(`hasActivation`, `hasDeactivation` defaulting to `false`, `name` as
`string | undefined`, `tagNames` as a readonly array). This gives every
consumer one normalized shape to read. The production role: the subsystem's
unit of wiring knowledge, and the object every later helper interrogates.

### `src/diagnostics/container_snapshot.ts` — per-container wiring record

The snapshot groups digests under a container label and default scope, and
copies the passed array in its constructor so callers cannot mutate a
snapshot after recording. It answers "what did this container look like
when we recorded it" and is the object reports, statistics, comparisons, and
audits take as their entry datum. Its role deliberately mirrors how the
facade's own documentation groups bindings per container.

### `src/diagnostics/digest_line_writer.ts` — output port

Introducing a port (rather than returning arrays everywhere) matches how the
facade itself separates interfaces from implementations, and lets future
consumers (logging, terminal pager) substitute a sink. The
`StringDigestLineWriter` default implementation keeps a private `_lines`
array behind `writeLine` and exposes `render()`, which is all the current
reports need. This location exists to keep an integration boundary honest:
one role of the subsystem is handing information to an external consumer
rather than computing over records.

### `src/diagnostics/snapshot_text_report.ts` — presentation helpers

Written first, this cluster answers the "print the wiring" request. `render`
composes the report header (container label, default scope), delegates row
formatting per digest, and appends the registered-service total;
`renderDigest` produces the single-registration row — every field fragment
with the exact separators and the conditional name/activation fragments the
tests pin down; `renderBindingLegend` explains in prose what a registration
means for resolution (caching by scope, activation by binding type, plus a
lifecycle sentence). `renderInto` complements them by streaming just the
identifiers to the `DigestLineWriter` port, deliberately reusing the port
rather than another string builder. The implementation shape — a class whose
methods take the records as parameters and terminate in string assembly —
matches the checklist origin of this cluster: each public method is one user
question. The tradeoff made here in the heat of development is visible: the
knowledge of what a digest row looks like, and what a scope or binding type
*means* in prose, lives in this file rather than with the data it describes,
and this file holds no state of its own for the job.

### `src/diagnostics/snapshot_statistics.ts` — measurement helpers

The statistics cluster answers "give me numbers for the release notes": a
breakdown keyed by scope and binding type with a `+lifecycle` suffix when
handlers run, a count of registrations that keep instances or run handlers
(singleton scope, constant-value bindings, or either handler), and a
qualifier usage tally that counts each tag, each `name:`-qualified
registration, and an `(unqualified)` bucket otherwise. Map-based results were
chosen over arrays because the natural consumer output is a `Group By`. The
methods read digests by looping over the snapshot's collection and deriving
each key fact from the digest's fields. As with the report cluster, the
word-smithing of the keys (for instance the lifecycle suffix) and the
definition of "stateful" is implemented here, while the class holds nothing
itself — the sentences and the definitions are knowledge about digest data
that lives at a distance from it.

### `src/diagnostics/snapshot_comparator.ts` — comparison helpers

Comparison answers the "what changed between two recordings" request.
`diff` keys baseline digests by service identifier, walks the current
snapshot's digests, reports new registrations as `added` with a scope/type
description, and otherwise asks `describeRegistrationChange`. That helper
compares two digest versions field by field and returns the first difference
in a fixed precedence (scope, binding type, name, tag count, activation),
each as a human-readable `before -> after` fragment, or nothing when
identical. The result objects (`SnapshotDifference` with
`added`/`qualification`/`registration` kinds) were shaped so issue reports
can quote them verbatim. This cluster was implemented as an independent
class because the request arrived as its own bullet; it re-derives placement
and change knowledge from both a snapshot pair and a digest pair, and again
carries no own state.

### `src/diagnostics/snapshot_codec.ts` — serialization helpers

The codec answers "attach the snapshot to a bug report". `serialize` deep-copies
the snapshot and each digest into a plain, alphabetically-keyed object shaped
by the `SnapshotPayload`/`DigestPayload` interfaces and returns
`JSON.stringify` output; `parse` validates the loaded text with a structural
guard (label and scope strings — the scope must be a known scope value — and
a digest array) and rebuilds records through `restoreFromPayload`. The
payload interfaces live here because this module is the interchange boundary:
they describe *external JSON text*, not the in-memory records. The field
copying was written key by key so a digest's JSON memory sits in this file
alongside the parse side that understands it, keeping the round-trip
self-contained at the cost of duplicating the field inventory once more in
a consumer of the record.

### `src/diagnostics/wiring_audit.ts` — live container cross-check

The audit class is the only place the subsystem touches a live
`@inversifyjs/container` `Container`. `listUnboundRegistrations` walks the
snapshot and reports recorded identifiers the container has no binding for;
`describeRuntimeStatus` produces one status sentence per registration stating
whether it is bound, its scope/type placement, and its handler registration
state (`Katana: bound (Singleton/Instance) activation registered, no
deactivation`). The container dependency is injected through the
constructor, matching the facade's constructor-injection idiom, and calls
into the container are kept as narrow delegation (`isBound`). One deliberate
design line: the audit answers *recorded* questions against the live object,
so the snapshot data remains the authority for what is being described.

### `src/diagnostics/index.ts` and `src/index.ts` — barrels

The module barrel groups the subsystem export list in one place
(class-exports separated from type-only exports as the house style
requires), and `src/index.ts` extends the facade with a single re-export
block, mirroring how the existing package surface is re-exported from
published packages. This keeps downstream imports working through the
package root exactly as users expect (`import { ContainerSnapshot } from
'inversifyjs'` style), with no changes to the previously exported names.

### `src/test/diagnostics/container_diagnostics.test.ts` — behavior pinning

The tests express the feature's acceptance checklist against the public
exports, in the repository's established style (`mocha` with `chai`, classes
built through the public constructors). They pin exact strings for the
report, row, legend, and audit sentences; exact breakdown keys and counts;
exact qualifier tallies; exact difference objects for scope changes and
added registrations; the serialize/parse round-trip; rejection of text that
is not a serialized payload; and a real container scenario for the audit
using `container.bind(...).to(...)`. These tests document what users were
promised, so any later internal reorganization of the subsystem has a
behavioral harness to rely on.

## Scope decisions and saturation

The subsystem was taken end to end rather than choosing one convenient
entry point: every operator role the request list implies (presentation,
narrative explanation, statistics, comparison, serialization, audit) has a
file, and both record classes participate, because each role reaches into
the records through a different shape of access — a snapshot parameter, a
per-digest parameter, a `for...of` binding over a snapshot's collection, a
map callback parameter, or a digest pair. Locations that were considered and
deliberately shaped differently: the line-writer port is an integration
boundary rather than record data processing; the live `Container` is consulted
through narrow calls as an external runtime collaborator rather than being
mined for fields; and the codec's payload objects are interchange contracts
carrying digest data in and out. The ports and containers were given just
enough behavior to keep those boundaries explicit while all the
record-derived knowledge — formatting, key vocabulary, comparison
precedence, JSON field inventory, status wordings — was placed in the helper
classes, which hold no meaningful own state for it. The feature stops here
because the request list is exhausted: adding further consumers (a markdown
formatter, an HTML report) would repeat the presentation role without
introducing a new way of consuming the records.
