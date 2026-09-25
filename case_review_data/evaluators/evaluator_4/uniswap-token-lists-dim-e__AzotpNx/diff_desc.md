# Injection design record — token-lists versioning surface

## Maintenance motivation

`@uniswap/token-lists` is a small, dependency-free library that consumers use to
decide whether a token list they fetched is an update they should accept, and what
kind of semantic-version bump a change represents. Those decisions run on a hot
path: a consumer re-fetches lists regularly, validates them, and classifies the
difference before rendering anything. The natural library design keeps this cheap
by composing small pure functions — an ordering comparison, a per-field equality
check, a list diff — and letting the public entry points stay one short step above
those primitives.

The scenario modeled here is a performance-motivated "flattening" pass performed
by a contributor who profiled the library and concluded that the call overhead of
 delegating to helpers, and the intermediate `diff` structures built along the
 way, were waste on this hot path. Instead of trusting the bundler's inlining or
 opening a design discussion, they hand-copied the bodies of the helper functions
 into their callers, restructured the copies so they would read as natively
written code (not as pasted blocks), and dropped the now-unused imports and, in
one place, a now-unused helper entirely. The commit message of such work typically
reads like "perf: remove indirection from the comparison hot path".

The staged outcome is a codebase whose public entry points re-implement — in
adapted form — algorithms that still exist elsewhere in the library, and in one
place an algorithm whose only named implementation was deleted outright. Every
future behavior fix must now be made twice in structurally different code, and
the entry points' bodies are long enough that their original one-sentence
responsibility is no longer visible.

## The development evolution being modeled

This models a common real-world devolution: a library with well-scoped helper
methods accumulates an edit that trades design clarity for micro-perf. The
helpers are not removed from the public API where consumers depend on them, so
externally nothing changes, but internally the delegation edges disappear. Later
maintainers reviewing the code face a classifier that contains the entire diff
algorithm, a diff routine with hand-embedded comparison branches, and a
predicate with a hand-rolled ordering ladder — none of which mention the helpers
they replaced, because the copies were restructured with different idioms.

## Overall injection design

The injection touches the three production files that own all of the
repository's delegation edges:

- `src/minVersionBump.ts` — the minimum-bump classifier flattens its whole
  computation: what used to be "call the list diff, then classify" becomes a
  single-pass monolith that indexes the base list, walks the update, performs
  per-field change detection inline, tracks removals, and only then classifies.
- `src/diffTokenLists.ts` — the diff routine absorbs its module-private
  equality helper into its change-detection loop and the helper is deleted.
- `src/isVersionUpdate.ts` — the update predicate replaces its delegated
  ordering call with a hand-rolled component-by-component ladder.

Every copy was deliberately restructured rather than pasted verbatim, so no
fragment of the inlined code matches its origins structurally or textually.
The public API, the returned data, and all observable classification decisions
are unchanged.

## Per-location rationale

### `src/minVersionBump.ts` — classifier turned depth-3 monolith

**What changed.** `minVersionBump` previously delegated the entire base/updated
comparison to `diffTokenLists` and reduced the resulting `{added, removed,
changed}` structure to one of four enum values in four lines. After the change it
contains a full copy of that algorithm, adapted:

- The reduce-built nested index of the diff routine became a `Map` keyed by a
  composite `"<chainId>:<address>"` string, with `Set` membership tracking which
  identities the update contains; instead of building `added`/`removed` arrays
  and a `changed` map, the function keeps three boolean flags and short-circuits.
- The per-token change detection loop (formerly the diff routine's filter
  pipeline of `Object.keys(...)`) is re-expressed as a plain `for...of` walk with
  guard-style `continue`s.
- Inside that loop, the body of the former module-private equality helper is
  inlined a second time down: the strict-equality check, the type-mismatch
  comparison, and the element-wise array prefix check all appear as branches of
  the loop. This is the deepest inlined point — three levels of formerly
  separate implementations (classifier, diff algorithm, per-field equality) now
  nest inside one function frame, matching the concerning "posted quickly at
  01:00" flavor of the flattening work.

**Why this location and shape.** This entry point is the repository's only
three-level call chain, so it is where the depth claim is most convincing; a
composite-string index keyed by the natural identity pair reads as idiomatic
modern TypeScript rather than as pasted older code, which is exactly what a
performance-minded author would write when re-homing the logic. The classifier's
short precedence ladder (removal forces MAJOR, then addition MINOR, then change
PATCH, else NONE) sits at the end so the monolith's low-resolution reading —
"one pass over both lists decides the bump" — still looks plausible on review.
Its production role is unchanged: deriving the minimum semantic bump of a
token-list change.

### `src/diffTokenLists.ts` — change detection with an embedded helper

**What changed.** The module-private `compareTokenInfoProperty` function —
the library's constrained "do not attempt deep equality" value comparison — was
deleted, and its three-way comparison (strict equality, then type mismatch,
then element-wise array comparison) became branches written directly inside the
routine's change-detection section, merged with the two-stage key filtering that
used to precede it. The surrounding diff machinery (the reduce-based indexed
base, the update scan, the removal pass, the result object shape) is untouched.

**Why this location and shape.** This is the same flattening applied one level
down the same call subtree: the diff routine is itself a consumer of a helper,
so the sweep that absorbed `minVersionBump`'s calls naturally re-absorbed this
one too, and once both inlined copies existed the named helper had no remaining
callers — a flattening author who just deleted "dead" private code during the
pass writes exactly this. Locating the comparison branches inside the loop
rather than a local closure is the author's (arguably lazy) preference for one
flat pass. In production this routine remains the library's core diff and must
keep producing the same `TokenListDiff`.

**Distinct-accountability note.** The shallow-comparison semantics intentionally
carry subtlety and live on, unrolled, in two places: same-typed non-array
values are only equal when strictly identical; array pairs are compared
element-wise with the update array effectively acting as the prefix that must
match; nested objects are never compared deeply, so equal-valued extension
objects still count as changed. Neither unrolled copy warns the reader about any
of this, which is precisely the burden created by absorbing the helper.

### `src/isVersionUpdate.ts` — predicate with a hand-rolled ordering ladder

**What changed.** The one-line predicate — delegate to `versionComparator` and
test whether the base sorts before the update — now types out an
early-return ladder over `major`, then `minor`, then `patch` with six branches
and the unused import of the comparator removed.

**Why this location and shape.** This belongs to the same sweeping pass and
the same justification story: the shared comparator remains exported for API
compatibility while its internal predicate stops using it. The guard-ladder
idiom (an unrolled lexicographic comparison) differs from the comparator's
`else if` chain, the argument order is reversed once by the copy being made to
"read forward" (update compared against base), and each branch's variable names
are freshly chosen, so the copy does not textually resemble the function it
replaced. In production the predicate is the "is this fetch an update at all"
gate used by list consumers via the package entrypoint.

## Deliberate structural variation

The three inlined sites were re-homed with different idioms so none of them is
recognizable as a copy of its origin: a composite-key `Map`/`Set` index with
presence flags replaces the diff structure in the classifier; merged loop-guard
control flow replaces the filter pipeline in the diff routine; a guard ladder in
reversed operand order replaces the comparator call in the predicate. In two of
the three sites the original helper still exists elsewhere in the library for
its external consumers; in the third, the absorbed helper was deleted, so there
is no remaining named implementation to diff against. Long expository comments
were added to the fattened bodies describing the new flattened data flow.
