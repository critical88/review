# Injection design record — "runner consolidation" wave across squirrel's execution seams

## Maintenance motivation

squirrel's public API has accreted a family of very small capability
interfaces: one wrapping Exec-style execution, one for Query-style execution,
one for query-row, one-shot context counterparts of each, one for Prepare, and
group interfaces that bundle them for proxy-style consumers. Users periodically
complain about the proliferation: to talk to a database you may need to
reference three or four distinct names, and generic helper code that just
wants to "run a statement somewhere" has to pick-and-embed per capability.

The change modeled here is the reaction a real maintainer has to that
complaint: stop maintaining so many shapes. Declare **one** interface that
describes everything a statement target may ever be asked to do — the
synchronous execution family, its context counterparts, and the prepare
capability — and re-point the existing seams at it, keeping the old names
alive as aliases so downstream code keeps compiling. The declared intent is
"hands can pass a single runner value everywhere"; the undeclared cost is
that every seam which actually consumes one capability now demands the whole
surface. Providers that own only part of the execution repertoire (a
BulkInsertExecutor that only implements Exec flavors, a query-only view, a
Redis-backed sharded proxy with no Prepare path) stop fitting the seams that
used to accept them, and future implementors grow stub methods.

## Development evolution being modeled

This is modeled as one consolidation commit wave, the way it typically lands
in a real library history rather than as a single monumental patch:

1. **Introduce the unified surface.** A new interface is declared — the union
   of the context execution families plus the prepare capability — together
   with narration about replacing per-capability juggling.
2. **Unify the wrappers.** The standard-library wrapper helper is re-typed to
   return the unified surface so a wrapped `*sql.DB` can be handed to
   "anything that runs or prepares statements"; the wrapper's own source
   target interface grows the prepare methods to make that plausible.
3. **Homogenize the context interfaces.** The three context capability
   interfaces stop being per-capability: each now embeds the synchronous
   trio and re-declares all three context methods, with a comment noting the
   name survives "for compatibility". Since helpers reference these names in
   their parameter positions, the seam obligations fatten without touching
   any helper body.
4. **Collapse the statement-cache shells.** The prepare-facing interfaces
   the cache declared — including the proxy group interfaces — are
   re-declared as the unified surface, again with "kept as a compatibility
   name" comments. The runtime narrowing probe in the cache's context method
   (a type assertion that used to distinguish context-capable targets and
   yields the library's no-context-support error) is left in place, now
   asserting a distinction the fattened type system guarantees.
5. **Fold the debug hook into the placeholder seam.** The placeholder-format
   contract grows the debug-token method that only the debug renderer
   consults, rationalized as "custom formats cannot accidentally ship without
   it". Every statement-data renderer's stored format now carries the
   obligation whether or not rendering ever debugs.
6. **Re-point the one-shot helper signatures.** The sync execution helpers'
   parameter types are swapped to the shared sync base, with comments
   explaining every execution helper now "shares one runner signature".

Steps of exactly these shapes are common in Go library histories; the result
is a package that compiles, passes its suite untouched, and yet no longer
accepts its own narrower implementations at its seams.

## Overall design of the injected diff

The wave is deliberately **tethered to a superset-safe provider**: every
widened obligation is placed where the real-world provider is a full
`*sql.DB`-like target that already has every capability. This keeps runtime
behavior identical — nothing is exercised that was not already present,
rendering is untouched — while making every capability-scoped provider
outside the standard library structurally incompatible with the seams. The
diff therefore reads entirely as interface re-composition, plus the two
smallest signature retypes that force each seam to take the new shape, plus
one call-site wrapper injection needed to keep the package building.

Shape choices:

- The unified interface is declared once, next to the existing runner group
  interfaces, as the natural "home" for such a declaration in this package.
  Its method set mixes sync and context capabilities and the prepare pair,
  making it unambiguous that it is a union rather than a capability.
- Old names are not deleted but **re-declared by embedding** the unified
  surface. This is the realistic way "compatibility" is kept in a Go
  codebase, and it preserves the property that no exported
  identifier disappears.
- Where a signature had to move, the helper body is left byte-identical and
  only the parameter type and a narrating comment change; the seams fatten
  by declaration-site edits, not by consumer rewrites.
- The one place the wave could not compile unchanged — the cache proxy that
  feeds a standard `*sql.DB` into a constructor now demanding prepare
  capability at execution-surface width — was resolved the way a hurried
  consolidator resolves it: by hard-coding the standard wrapper at the call
  site instead of reconsidering the contract.

## Per-location rationale

### `placeholder.go` — capacity merge in the formatting seam

The library already had a private capability probe for debug tokens: the
debug renderer narrows a placeholder format to a tiny internal interface to
scan for the format's debug placeholder token. Folding that method into the
**public** placeholder-format interface is the classic "keep the whole
capability together" play — the argument writes itself, as the added comment
shows ("custom formats cannot accidentally ship without it").

This site was chosen because its leverage is enormous relative to its size:
the format interface is stored by every statement-data struct in the package
(select, insert, update, delete), so a single added method in one declaration
silently obligates four renderers whose rendering path never invokes it. The
implementation shape — add the method to the existing declaration and update
its doc paragraph, leaving the private probe intact — is exactly how such a
merge lands in practice; the production role of the seam (render `?`
placeholders per dialect) is untouched.

### `squirrel.go` — the unified surface and the sync one-shot helpers

This file hosts the runner group interfaces, the wrapper helper, and the
sync one-shot helpers, so it is where the wave begins and where its anchor
declaration naturally lives.

- **The unified interface declaration** sits directly after the existing
  group interfaces, embedding the three context capability interfaces and
  declaring the prepare pair. Its narration ("Instead of forcing callers to
  juggle the small per-capability interfaces, one interface describes
  everything a statement target may ever be asked to do") is the surface
  rationale for the whole wave.
- **`ExecWith` and `QueryWith` retypes** swap parameter types from the
  exec-only and query-only capability interfaces to the shared synchronous
  base group. The helper bodies each still call exactly one method, so the
  seam now demands a capability sibling that the helper itself ignores.
  These two retypes are the smallest honest form of the wave: two
  signature-line edits with comments rationalizing "one runner signature"
  per helper. The query-row flavored helper was intentionally left
  untouched — see exclusions in the design record below.
- Importing `context` here is required by the prepare-context method on the
  unified interface declared in this file, and shows how one union
  declaration drags dependencies across previously unrelated files.

### `squirrel_ctx.go` — homogenizing the context trio and pushing the wrapper out

The context file is where the wave's central tension lives: it declares the
three context capability interfaces that the three context helpers reference
parameter-by-parameter. Those one-method interfaces are precisely what a
consumer scoping a single context capability needs, and precisely what a
consolidator finds redundant.

- **Each of the three interfaces** is re-declared to embed the synchronous
  trio and declare all three context methods, making their method sets one
  fused surface. The doc comments narrate, in each case, that the name
  survives "for compatibility" and that "every context-capable target is now
  required to implement the full execution surface" — the sort of comment
  real consolidations leave behind. The helper bodies below them are
  untouched, each still invoking exactly one method of the six now
  demanded.
- **The wrapper helper's return type** moves from the context group
  interface to the unified surface, exactly what step "unify the wrappers"
  should do — a wrapped target can now be passed wherever any capability is
  consumed, including the cache.
- **The wrapper's source interface (the standard target contract)** grows
  the prepare pair, so that any standard-library target it adapts maximally
  satisfies the unified surface — an obligation added to a *specification of
  the standard library*, not just to the package's own glue. In practice
  `*sql.DB` has these methods, so nothing breaks — but implementations of
  the standard-target contract are likewise narrowed even though they are,
  by definition, merely "things matching database/sql".

This spread — capability interfaces, a return type, and a source-interface
specification — models how a single wave pushes one design decision through
a wrapper family in three different structural positions.

### `stmtcacher.go` — collapsing the cache's prepare shells

The statement cache is the purest consumer-side victim of the wave: it holds
a value in a struct field, hands it to two constructor functions via their
parameters, and consumes from it exactly one capability — Prepare (plus its
context sibling via the runtime probe). Everything else those values could
ever do is demanded, never asked — exactly the over-consolidated shape a
unification wave produces when it reaches caching code declared alongside the
proxy group interfaces that bundle execution capabilities.

- **The prepare-facing interface and the proxy group interface** are both
  re-declared as the unified surface with "kept as a compatibility name for
  the unified SqlRunner surface" comments. Nothing in this file's bodies
  changes — the cache field and both constructor parameters keep their
  nominal types, which now mean something eight methods wide.
- **The proxy constructor call** is the one body edit: wiring the standard
  wrapper around the raw `*sql.DB` target so the constructor's newly fat
  parameter can be satisfied at all. The comment on the constructor is
  untouched — the retrofitting sits, realistically, transparently inside the
  line building the proxy. A reader of the cache code learns nothing except
  by already knowing which target can fill the fat slot.

### `stmtcacher_ctx.go` — the context-side cache shells and constructors

The context flavor of the cache file hosts the declaration both constructors
drain through: the deprecated name for the prepare-with-context contract,
and the context variant of the proxy group interface. Both are re-declared as
the unified surface, with comments describing themselves as the context-side
compatibility aliases of the same consolidation. The constructor function
bodies are untouched — the parameters keep their nominal types — and the
constructors' doc comments no longer describe what they accept honestly.

A bonus detail left from a real history of such waves: the cache's own
context method narrows its stored preparation target through a type
assertion to this very interface, yielding the library's no-context-support
error when the assertion fails. Before the wave, this was a genuine
capability probe — a cache holding some other value could lack the context
path. After the wave, the declaration guarantees the capability statically,
leaving a runtime assertion that cannot fail while still claiming to guard
an unsupported case: a fossil of the distinction the wave erased.

## Design exclusions within the wave's boundary

- The query-row flavored one-shot helper keeps its original narrow parameter
  type. The lazy capability probe on the statement-data side (which yields
  the runner-not-queryrow error) still wants to acknowledge partial runners
  on that path; folding it would not have been one honest single-shape
  decision but a semantic edit. A real consolidator shipping this wave
  plausibly missed the third sibling — histories are rarely symmetric.
- The fluent builder setter chain and the statement-data runner storage are
  not re-typed: runners flow through a setter path whose clients re-narrow
  them with capability probes per execution site, and storage there is
  capability-optional by design. Real waves leave this area alone because
  the builders are the package's tested core.
- No rendering, no execution body, no builder compositing is modified
  anywhere in the diff; the entire change is declared-shape-level.
