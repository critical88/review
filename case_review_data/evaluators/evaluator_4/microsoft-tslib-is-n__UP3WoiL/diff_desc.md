# Injection design record — tslib helper record surface

## Repository context

tslib is the runtime library of helper functions that the TypeScript compiler emits calls to.
Each helper exists in four shipped source surfaces that must stay behaviorally interchangeable:

- a UMD-formatted runtime (consumers on the default export path),
- an ES6-syntax CommonJS runtime (node/bundler `module` path),
- a pure-ESM runtime (bundler `import` path),
- a hand-maintained ambient declaration file that downstream tooling and consumers type-check against.

Each runtime file is a full copy of the same helper bodies, differing only in module wrapper and
indentation. Several helpers construct or consume a *per-call record object*: the generator op
record (`{ label, sent, trys, ops }` plus generator verbs), the sync `__values` array-like adapter,
the async adapter records of `__asyncGenerator`/`__asyncDelegator`/`__asyncValues`, the decorator
application context cloned per decorator call inside `__esDecorate`, and the resource-tracking
environment record shared by `__addDisposableResource`/`__disposeResources`.

## Maintenance motivation

Downstream compiler emit is generated code: it cannot adapt its call shape per helper family, and
emit codepaths that hand-build one of these records (decorator contexts, disposal environments)
have no way to know which capabilities a given helper's record actually offers. The natural fix a
maintainer reaches for is to make every record handled by the helper runtime expose one uniform
capability surface, so emit sites need no per-family knowledge. A second, smaller motivation: the
declaration file describes these records with vague types (`any`, `object`), and tooling consumers
would benefit from a named, documented record type instead.

## Evolution being modeled

The shipped helpers historically each defined tiny family-local closures for their own glue:
`__esDecorate` defined a private `accept` validator, `__asyncGenerator` defined a private
`awaitReturn` wrapper, `__asyncValues` defined a private `settle` helper. The evolution modeled
here is the over-unification step of that cleanup: instead of leaving each family's glue local,
all of it is hoisted into one shared capability protocol, and every record constructed or
handled by any helper — regardless of family — is *adopted into* that protocol. The capability
members chosen for the protocol are the union of the distinct glue shapes found across the
families (promise adoption, await wiring, settlement, decorator-result validation, and
initializer enqueueing), so the protocol members are individually plausible and each traces to a
real family's need — just not to the family that happens to be adopting them.

## Overall design

Each runtime file gains a small shared kit near its module top:

- `provideHelperCapability(context, name, capability)`, a provide-if-missing installer that uses
  `Object.defineProperty` with `enumerable: false` on engines that support it (ES5 dual-path
  assignment fallback otherwise), so site-owned members can never be shadowed and the records'
  enumeration behavior as seen by user code (`for-in`, `Object.keys`, JSON) is unchanged;
- `createHelperContext(record)`, which stamps the five capability members
  (`adopt`, `awaitable`, `settle`, `accept`, `addInitializer`) onto a record and returns it.

Every record-producing or record-consuming family then routes its record through
`createHelperContext`:

- records built inline (`return createHelperContext({ ... })`) or by prototype linkage
  (`g = createHelperContext(Object.create(...))`) in the iterator/async families;
- records handed in by compiler emit (`createHelperContext(env)` in the disposal pair, whose
  environment record is constructed by emit code, not by the helper);
- records cloned per decorator application in `__esDecorate`, with adoption performed *after* the
  site-owned `addInitializer` assignment so the site's own member always wins.

In the three families that already had equivalent family-local glue, the local closure is
removed and its former call sites read the capability off the adopted record instead. The
declaration file gains the protocol as a documented, exported interface with optional members and
an index signature (optional so that existing literal emit shapes keep compiling), and routes the
related helper signatures through it: five record-constructing helpers return it, the decorator
helper takes it as its context parameter, and the two disposal helpers intersect it with their
structural stack shape.

The design is ES5-only and per-file line-ending conventions are preserved (CRLF in the UMD and
CommonJS files and the declarations, LF in the ESM file).

## Per-cluster rationale

### The shared kit (one block per runtime format)

Placement: immediately after `extendStatics` at each file's module top, before the first helper.
This is the position a maintainer would choose for module-wide infrastructure, and it gives the
identical kit text in all three formats (mod 4/0/2-space indentation). The comment above
`createHelperContext` documents the intent in the maintainer's own words — one uniform runtime
surface for every record handled by the helper emit, citing iterator adapters, decorator
contexts, and resource-tracking envelopes. The provide-if-missing installer exists so adoption
can never alter observable record state: emit-built objects sometimes already carry members
under these names, and overwriting them would change behavior.

### Decorator application (`__esDecorate`, decorators cluster)

What changed: the private `accept` closure is deleted; its five use sites (accessor get/set/init
validation, plain-result validation, and the `addInitializer` enqueue) now read
`context.accept`; the cloned per-application context is adopted via
`createHelperContext(context)` placed after the site-owned `context.addInitializer` assignment.

Why this site: `__esDecorate` clones a fresh context record per decorator call, so it is the one
family where the record is *both* constructed by the helper and handed to third-party code
(decorators). The site-owned `addInitializer` member enqueues into `extraInitializers` and
differs from the kit's generic always-reject stub; placing adoption after the assignment keeps
the site member authoritative while still forcing the whole protocol onto the record.

Production role: ECMAScript decorator emit path; decorators observe `context` directly.

### Generator record (`__generator`)

What changed: the prototype-linked generator record `g` is created inside the adoption call.

Why this site: `__generator` is the most record-shaped family — the compiler-visible op
protocol lives on a separate record `_`, while the returned object `g` inherits from
`Iterator.prototype` (or `Object.prototype`) and receives `next`/`throw`/`return`/`Symbol.iterator`
members. Wrapping the `Object.create` expression is the smallest form that routes an
inheritance-receiving record through the protocol without touching member assignment order.

Production role: every downlevel generator function the compiler emits.

### Sync iterator fallback (`__values`)

What changed: the array-like `next`-only adapter record, built inline for objects without
`Symbol.iterator`, is wrapped: `return createHelperContext({ next: ... })`.

Why this site: this family constructs the smallest, most obviously single-purpose record in the
runtime — a one-member adapter. Its narrowness makes the uniform-protocol decision visible here
at its most extreme, and the record already exists as an inline literal, so wrapping it is a
one-expression change.

Production role: `for-of`/spread over array-likes lacking an iterator method.

### Async adapter trio (`__asyncGenerator`, `__asyncDelegator`, `__asyncValues`)

What changed: all three async adapter records route through adoption —
`i = createHelperContext(Object.create(...))` for the async generator (inheritance receiver,
like `__generator`), `i = createHelperContext({})` for the delegator and async-values adapter
records. In two of the three, existing family-local glue is replaced: `awaitReturn` in
`__asyncGenerator` becomes `return i.awaitable(f, reject)`, and the `settle` closure in
`__asyncValues` becomes `i.settle(...)` inside the verb promise wiring.

Why this shape: the async families are the protocol's promise-flavored consumers, so the two
capability redirections (`awaitable`, `settle`) live here; the delegator shows the pure
surface-widening case (adoption with no glue removal) right next to them, since its verbs never
read the capabilities at all. Keeping the three siblings in the same cluster in each file
reflects how a maintainer would handle one codepath family in a single pass.

Production role: `async function*` bodies, `yield*` delegation of sync iterators into async
ones, and `for await`/spread over async iterables.

### Resource-tracking pair (`__addDisposableResource`, `__disposeResources`)

What changed: both helpers adopt the emit-supplied `env` record via `createHelperContext(env)` as
their first statement.

Why this site and form: the disposal environment is the one record not constructed by the
library at all — compiler emit builds `{ stack: [], error: undefined, hasError: false }` and
hands it in. Adoption-in-place on the incoming record is the only available form, and doing it
in both helpers of the pair is what a uniform-surface pass would do; neither helper reads any
protocol member.

Production role: `using`/`await using` disposal emit; LIFO disposal and suppressed-error
chaining behavior is untouched.

### Declaration surface (ambient declarations)

What changed: a new exported interface documenting the five capabilities as optional members
plus a doc comment for the index signature; the five record-constructing helper signatures now
declare the interface as their return type; the decorator helper's `contextIn` parameter and
its `@param` doc reference it; the two disposal signatures intersect it with their existing
structural stack shape.

Why this surface: the declaration file is the published boundary — it is what every downstream
type-checker resolves, so any recorded protocol is only real to consumers once it is routed
through the signatures. Optional members and the index signature keep every existing literal
emit shape assignable (adding required members would have rejected decorator contexts and
disposal envelopes that emit constructs, which would break consumer compilation). The doc
comment on the interface is written from the maintainer's unification narrative rather than
from any one family's perspective.

### Per-format notes

The identical cluster set is applied to all three runtime formats with only indentation
differences (the UMD copy nests its helper bodies inside a factory function; the ESM copy uses
two spaces less than the CommonJS copy). This repetition is not padding: each file is a
separately consumed artifact, and any kit or adoption present in only some formats would make
the trio non-interchangeable — which is exactly the compatibility property the repository's own
test layout (running every helper against all three formats) is built to protect. Line endings
follow each file's existing convention.

## What was deliberately left alone

- Classical inheritance helpers, argument-shaped helpers, and legacy promise scaffolding keep
  their signatures and bodies: they hold no per-call record to adopt into.
- Import/export interop builders keep their structural record copies: adoption there would
  shadow copied module members through the provide-if-missing path or change their observable
  member set.
- Value-wrapper helpers (frozen template-object arrays, one-field await boxes, WeakMap-backed
  private-field states) keep their locked-down shapes; widening those records had no plausible
  emit-side consumer.
- The module-directory re-export surface is untouched: it names helper functions only and
  introduces no record boundary.
- The newline conventions, ES5-only syntax constraint, and the exact enumerable-member sets of
  every returned record are preserved throughout, per the behavior contract above.
