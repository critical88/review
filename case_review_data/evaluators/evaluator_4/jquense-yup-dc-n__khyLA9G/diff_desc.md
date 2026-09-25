# Injection design record — yup resolve/validation parameter threads

Smell target: `data_clumps` — groups of values that frequently travel together but
are repeatedly declared and passed as separate, unrelated pieces.

## Maintenance motivation

yup resolves references, interpolates message parameters, derives defaults and
scaffolds nested validation on every `validate`/`cast` call. During a
profiling-driven pass over the resolve pipeline, a maintainer notices that each
seam allocates an intermediate object bag ("resolve options") only to pick three
fields out of it again. The upstream `Reference#getValue(value, parent, context)`
helper already takes the resolution inputs as separate values, so the natural
quick win is to stop materializing the intermediate object and hand the three
values straight over. That style then spreads the way internal conventions
usually spread in this codebase: once the resolution paths stopped using the
object bags, the adjacent seams that feed them — nested-test scaffolding and the
queued-test dispatch protocol — were converted for consistency ("we're passing
all these values anyway, let's pass them directly"), because keeping two
threading conventions side by side felt worse than finishing the pass.

This models a very ordinary evolution: an internal, behavior-preserving
performance/housekeeping refactor that grows a positional argument convention
across a pipeline because each next step locally looks like the same cleanup.
Public entry points keep their documented options objects; all of the change is
below the API surface, in how internal seams exchange the values.

## Overall design

The change re-threads three distinct value groups that the pipeline already
moves as objects:

1. **Resolve-context values** — the value being resolved, its parent object,
   and the shared validation context. Previously each seam accepted an options
   bag (cast options, `ResolveOptions`, default-thunk options) and picked
   `value`/`parent`/`context` out of it; now the three values are separate
   positional parameters at each seam, mirroring `Reference#getValue`.
2. **Nested-site descriptor** — for object fields, array/tuple entries and lazy
   schemas: the key or index, parent value, parent path, original (pre-cast)
   parent, and options. Previously one config object carried this bundle into
   `asNestedTest`; the schema base, the lazy schema and the `ISchema` contract
   now declare the bundle as six positional slots.
3. **Test-run protocol values** — value, path, options, originalValue, schema,
   plus the `panic`/`next` callbacks. Previously the queued-test protocol passed
   one options object per test and a single bag to the runner; the contract type
   and the dispatch method now spread the context across positional slots.

Consequences that the change owns consistently:

- Call sites in the collection validators decompose their options object at the
  point of use (e.g. threading `options.path`, `options.originalValue` and the
  parent value positionally into nested tests, and passing `undefined` for the
  path slot whose value they already have in scope).
- `Reference#cast` receives the parent/context slots directly in
  `ObjectSchema#_cast` for reference fields, instead of passing the cast
  options bag and letting the reference pick it apart.
- Default derivation in `ObjectSchema#_getDefault` computes each field's inner
  value/parent pair instead of cloning the options bag per field.
- The now-unused internal config type aliases (`TestRunOptions` in
  `schema.ts`, `NestedTestConfig` in `types.ts`, `TestOptions` in
  `createValidation.ts`) are left in place as exported surface while the
  implementations no longer use them, and the `ResolveOptions` import is dropped
  from `createValidation.ts` since the values no longer arrive as that object.
- A closed-over callback in `Schema#asNestedTest` renames its unused protocol
  slots with leading underscores (the codebase's convention for intentionally
  unused arguments).

## Per-cluster rationale

### 1. `src/Reference.ts` — reference resolution seam

`Reference#cast` drops its options bag and declares `(value, parent, context)`,
forwarding directly to `Reference#getValue`, which has always taken exactly
that trio. Rationale: `cast` is a thin wrapper around `getValue`; with the
wrapper no longer allocated an object bag to immediately destructure it, the
two functions finally share one visible signature shape. Role: entry point of
reference resolution (cast side), consumed by object casting for ref fields and
by anything that materializes a ref.

### 2. `src/util/createValidation.ts` — message-parameter and ref resolution

`resolveParams` and `resolveMaybeRef` previously accepted a `ResolveOptions`
bag and pulled the trio out of it; now `(params, value, parent, context)` and
`(item, value, parent, context)`. `TestContext.resolve` and
`TestContext.createError` therefore pass the current validation triple
positionally when they unwrap refs or interpolate message parameters. The
`validate` runtime and the exported `Test` type mirror the queued-test
protocol shape (see cluster 3) so that the test closure reaches the validator
with plain positional arguments. Rationale: this is the hottest resolve path in
the library (every `createError` message build), the most obvious place a
positional pass would start, and it shares `resolveMaybeRef` with the reference
seam, so the same helper serves both installments of the resolve-context
thread. Role: `TestContext` facilities — `createError` params interpolation and
ref unwrapping through `this.resolve`.

### 3. `src/schema.ts` — dispatch protocol and defaults on the base schema

Three seams on the base `Schema` class adopt the positional pass:

- The `RunTest` contract type and `Schema#runTests` spread the queued-test
  context — tests plus value, originalValue, path, options and the
  `panic`/`next` callbacks — across positional slots, and the inner dispatch
  invocation forwards them positionally (`undefined` marks slots the runner
  already bound at construction).
- `Schema#asNestedTest` declares the six-slot nested-site descriptor and
  returns a closure that re-declares the whole protocol positionally before
  delegating into `_validate` with the test options it derived.
- `Schema#_getDefault` accepts the resolve-context values positionally and,
  for thunk defaults, builds the documented default-thunk options object at the
  last moment; `getDefault`/`describe` therefore decompose their public options
  objects internally (passing `options.value`, `options.parent`,
  `options.context` positionally) instead of forwarding the bag.

Rationale: the base class is where all three value groups converge; converting
it is what forces every other owner of the pipelines to adopt the same ordering
when they participate, because they override or call these members. The public
`getDefault`/`describe`/`resolve` entries keep their documented objects — the
decomposition is strictly below the API. Role: shared dispatch engine and
default/invariant machinery.

### 4. `src/types.ts` — the `ISchema` contract

`ISchema#asNestedTest` mirrors the widened positional form of the base-class
method so that the duck-typed schema interface (used by `Lazy` and
non-`Schema` participants) can express it; `NestedTestConfig` remains exported
although internal call sites no longer construct it. Rationale: a positional
internal convention only works if the structural contract matches; widening the
interface keeps compile-time participants (tuple/array/object inner types) honest.
Role: the type-level contract every schema implementer satisfies.

### 5. `src/Lazy.ts` — lazy schema delegation

`Lazy#asNestedTest` converts to the six positional slots, resolves the lazy
builder with the resolved value, and forwards the same positional bundle to the
underlying schema's `asNestedTest`. Rationale: `Lazy` is the non-`Schema`
participant of the nested pipeline; adopting the same signature shape keeps its
delegation transparent and avoids a per-lazy-field adapter object. Role: lazy
schema resolution entry point for nested validation.

### 6. `src/object.ts`, `src/array.ts`, `src/tuple.ts` — collection validators

Each collection `_validate` builds its nested tests by passing the site's
pieces positionally: the key (objects) or index (arrays/tuples), the current
value, `options.path`, the original parent value, and the options, then hands
the queued tests to the runner positionally. `ObjectSchema#_getDefault` walks
nodes with the positional context and computes each field's inner value/parent
pair instead of cloning an options flow per field; `ObjectSchema#_cast` passes
the ref field's parent/context slots positionally to `Reference#cast` while
schema fields keep the cast options bag.

Rationale: the collection validators are the natural call-site consumers of
clusters 3, 4 and 5; their decompositions are what make the positional ordering
a shared, repeated convention across independent modules rather than a
single-file idiom, and their assigned roles (fields, entries, tuple items,
defaults, ref fields) are the pipeline's real workloads. Role: the recursive
validation/casting paths for object fields, array entries and tuple items —
where the nested descriptor quality of the thread matters most.

## Scope boundaries

- `src/Condition.ts` and the `when()`/`lazy()` builder callbacks were left
  untouched: their options objects are part of the documented builder API and
  the values they need travel with members (`index`, `originalValue`, full
  parent scope) that the resolve-context pieces alone do not describe.
- The public surface (`resolve`, `describe`, `getDefault`, `cast`, `validate`,
  `validateAt`/`validateSyncAt`, `~standard`) keeps its documented shapes.
- The codebase code style is followed throughout (single quotes, trailing
  commas, and the leading-underscore convention for intentionally unused
  callback arguments).
