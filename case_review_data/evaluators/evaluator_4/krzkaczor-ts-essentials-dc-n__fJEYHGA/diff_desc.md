# Change description: per-family built-in switches across the deep rewriting utilities

## Repository context

ts-essentials is an almost purely type-level library: the production surface under
`lib/` is a set of exported TypeScript utility types. The "functions" of this codebase
are exported generic type aliases, and their "parameters" are generic type parameters.

A recurring structure in this family is the **deep rewriter**: a utility that walks a
type's structure (objects, tuples, arrays, maps, sets, promises, iterators) and
rebuilds every level. Each deep rewriter has to answer one question before it steps
into a value: *is this value a traversal leaf?* For primitives and functions the answer
is always yes. For the built-in object families `Date`, `Error` and `RegExp` the answer
is a per-utility design decision, and it is not uniform across the library:

- `DeepReadonly` recurses into `Error` fields but keeps `Date` and `RegExp` as leaves
  (default record `{ date: false, error: true, regexp: false }` in
  `DefaultDeepReadonlyOptions`).
- `DeepPartial` and `DeepWritable` use the leaf check `Exclude<Builtin, Error>`, i.e.
  the same decision: `Error` is rewritten, `Date` and `RegExp` are left alone.
- `DeepRequired` rewrites `Error` through a dedicated first branch
  (`Type extends Error ? Required<Type>`) and leaves the rest of the built-in families
  untouched (`Type extends Builtin`).
- `DeepNullable`, `DeepUndefinable`, `DeepNonNullable`, `DeepPick` and `DeepOmit` stop at
  every built-in family (`Type extends Builtin`).

Since `Builtin = Primitive | Function | Date | Error | RegExp`, that per-utility decision
is exactly a small, cohesive group of three values: *how are `Date`, `Error` and
`RegExp` treated during the rewrite?* These three values always travel together — every
deep rewriter needs an answer for all three at the same time, and every recursive step
repeats that answer.

## Maintenance motivation

`DeepReadonly` had already grown user-facing configurability for this group (the
`OverrideDeepReadonlyOptions` record merged via `CreateTypeOptions`). Users of the other
deep utilities keep asking for the same knob, because the hardcoded decisions do not
fit all domains:

- owners of models where `Date` fields are plain data want nullable/partial variants
  that also descend into dates, while others want the current stop-at-`Date` behavior;
- teams serializing error-augmented objects want to opt out of rewriting `Error`
  fields (stack traces must survive deep transforms);
- schema-driven code that deep-picks or deep-omits wants the traversal to start
  descending into built-in containers instead of stopping at them.

The natural subject of this change is therefore: **roll per-family built-in handling out
from `DeepReadonly` to the rest of the deep rewriting family**, while keeping every
existing user's types exactly as they are today.

## The evolution being modeled

The upstream history this change imitates is a familiar one: a long-requested
capability is finally delivered as a series of small, independent PRs, each touching the
one utility the contributor cared about. Each PR re-derives the underlying question on
its own, picks a naming scheme that reads well locally, threads the new values through
"just enough" call sites, and ships — because for that one utility the change is small
and obviously behaves the same with the defaults supplied.

Concretely, the modeled rollout:

1. Each deep utility gains a set of trailing **boolean type parameters, one per
   built-in family**, with defaults that reproduce that utility's current leaf
   behavior. Because they are trailing defaults, every existing instantiation
   (`DeepPartial<Order>`, `Buildable`, `StrictDeepPick`, every test) keeps compiling
   and computing the same type.
2. Each utility grows a module-local helper alias that combines those booleans into the
   effective built-in leaf set the recursion stops at.
3. Every recursive reference inside the utility passes the booleans down one by one in
   fixed order, so the traversal really is configurable at depth.
4. `DeepReadonly` participates in the same direction: its internal layers trade the
   single options-record parameter for the three per-family flags that the rest of the
   family now uses, while its exported signature still accepts the object-style
   override record users already depend on.

No shared abstraction for the new values is introduced anywhere in the diff; each module
owns a closed copy of the mechanism that fits its local style.

## Overall design

- **Group**: the three built-in traversal decisions (`Date` / `Error` / `RegExp` handling)
  per deep utility. They are conceptually one unit — a built-in leaf policy — but they
  are materialized in this change as standalone boolean type parameters.
- **Per-utility helper**: `DeepPartialBuiltin`, `WritableBuiltin`, `NullableBuiltin`,
  `UndefinableBuiltin`, `NonNullableBuiltin`, `RequiredBuiltin`, `ConditionalBuiltin`,
  `PickBuiltin`, `OmitBuiltin` — each a private alias that maps the booleans to a leaf
  set (an `Exclude<...>` over `Builtin` in most modules, an enumerated union in
  `deep-readonly`).
- **Threading**: the booleans are forwarded as separate type arguments at every
  recursive instantiation — 10 to 17 forwarding positions per exported alias, including
  container branches (Map/ReadonlyMap/WeakMap/Set/ReadonlySet/WeakSet/Promise),
  array/tuple branches, object mapped-type branches and the iterator protocol
  special cases in `DeepWritable`/`DeepReadonly`.
- **Behavior**: defaults are chosen per utility so the pre-change leaf set is exactly
  reproduced. Four distinct default configurations appear in the change, mirroring the
  four pre-existing leaf behaviors listed above (Error-recursing mutators, plain
  all-leaf nullability handling, the Error-first branch of `DeepRequired`, and the
  record-derived default in `DeepReadonly`).
- **Formatting/style**: new code follows the project's prettier configuration, and the
  added doc comments (one per helper) match the tone of the surrounding JSDoc.

## Per-site explanations

### `lib/deep-partial/index.ts` (39 insertions, 9 deletions)

*What changed.* The former inline leaf check `Type extends Exclude<Builtin, Error>` is
replaced by a private helper `DeepPartialBuiltin<DatePolicy, ErrorPolicy, RegexpPolicy>`,
and `DeepPartial` gains three trailing parameters `DatePolicy = false`, `ErrorPolicy =
true`, `RegexpPolicy = false`, forwarded at all 12 recursive references.

*Why this site.* `DeepPartial` is the most commonly reached-for deep rewriter and the
one users most often complain about w.r.t. `Date` handling ("I want my timestamps partial
too" vs "never touch my timestamps").

*Why this shape.* The three booleans default to exactly the old leaf set: only `Error`
is excluded from `Builtin`, so `Error` keeps being rewritten and `Date`/`RegExp` stay
leaves. The helper is an `Exclude` over three per-family conditional terms —
`Flag extends false ? never : Family` — so a boolean reads as "this family is rewritten".

*Production role.* Optional-ness rewriting; the public boundary stays two-argument from
the caller's point of view thanks to trailing defaults.

### `lib/deep-writable/index.ts` (51 insertions, 13 deletions)

*What changed.* Private `WritableBuiltin<DateIsBuiltin, ErrorIsBuiltin, RegExpIsBuiltin>`
replaces the inline `Exclude<Builtin, Error>`; `DeepWritableObject` (the object and
iterator-protocol mapper) takes the three flags and re-threads them at its five
positions; `DeepWritable` gains `DateIsBuiltin = true`, `ErrorIsBuiltin = false`,
`RegExpIsBuiltin = true` and forwards at all 12 recursion sites.

*Why this site.* The mutability twin of `DeepPartial`; the same user asks is usually
"deep-writable should optionally keep Dates/Errors as-is".

*Why this shape.* Here the flags are named after the *positive* question ("is this
family a builtin leaf?"), so the defaults read `true/false/true`: `Date` and `RegExp`
are leaves, `Error` is rewritten — exactly the previous behavior. This module threads
through one extra layer (`DeepWritableObject` for the iterator protocol), so the two
internal aliases carry the flags as well.

*Production role.* Deep mutability rewriting for one of the library's flagship types.

### `lib/deep-nullable/index.ts` (46 insertions, 15 deletions)

*What changed.* Private `NullableBuiltin<IsDateBuiltin, IsErrorBuiltin, IsRegExpBuiltin>`
replaces `Type extends Builtin`; `DeepNullable` gains three defaults, all `false`, and
forwards them individually at its 17 recursion sites, including the two guard checks
in the WeakMap/WeakSet branches (`DeepNullable<Keys, ...> extends object`) where the
flags had to be carried into a conditional test, not just an instantiation.

*Why this site.* Nullability spread is where "keep the leaf type but wrap it" semantics
is most visibly family-sensitive, and the module has the widest branch set in the
family (the tuple/array split plus the WeakKey workarounds).

*Why this shape.* With all flags `false` the helper computes `Exclude<Builtin, never>`,
i.e. the original `Builtin` check, so nothing changes for existing users; flipping one
family to `true` opts that family into being rewritten.

*Production role.* Nullability rewriting used by API-response modeling.

### `lib/deep-undefinable/index.ts` (36 insertions, 15 deletions)

*What changed.* Private `UndefinableBuiltin<LeafDate, LeafError, LeafRegExp>` (its
parameters are declared unconstrained, defaulted with the literals `true`/`true`/`true`)
replaces `Type extends Builtin`; `DeepUndefinable` takes `LeafDate = true`,
`LeafError = true`, `LeafRegExp = true` and forwards them at 17 sites.

*Why this site.* Same request class as `DeepNullable`, mirrored for `undefined`.

*Why this shape.* Written closest to how a downstream contributor would phrase it
("a family stays a leaf when its `Leaf` flag is on"), which inverts the polarity used
in `deep-partial` and leaves the parameters unconstrained with literal defaults — it
compiles and behaves identically with the defaults, but it is easy to mis-read next to
its sibling modules.

*Production role.* Optional-value rewriting for partial input shapes.

### `lib/deep-non-nullable/index.ts` (40 insertions, 9 deletions)

*What changed.* Private `NonNullableBuiltin<BuiltinDateFlag, BuiltinErrorFlag,
BuiltinRegExpFlag>` replaces `Type extends Builtin`; `DeepNonNullable` adds the three
flags, all defaulting to `true`, forwarded at 12 sites.

*Why this site.* Completes the nullability pair — its inputs often come from union
types returned by the previous two utilities.

*Why this shape.* Coin flip of the same style used elsewhere: conditionals of the form
`Flag extends false ? Family : never`, guards under the `Flag extends boolean`
constraint, defaults `true` meaning "is a leaf". With the defaults the union subtracted
from `Builtin` is empty and the old `Builtin` check is reproduced.

*Production role.* Null-stripping counterpart used before validation layers.

### `lib/deep-required/index.ts` (40 insertions, 14 deletions)

*What changed.* `DeepRequired` gains `DatePolicy = false`, `ErrorPolicy = true`,
`RegexpPolicy = false`. `ErrorPolicy` does not feed the leaf helper at all — it
switches the dedicated `Type extends Error` branch (`ErrorPolicy extends false ? Type :
Required<Type>`) — while a reduced private helper
`RequiredBuiltin<DatePolicy, RegexpPolicy>` (two members, not three) drives the leaf
set. All three flags are still forwarded at every recursive position (15 instantiation
sites each for `DatePolicy`/`RegexpPolicy`, 14 for `ErrorPolicy`).

*Why this site.* `DeepRequired` is the proof that the per-family decision is structured
and not a flat toggle: this utility treats the `Error` family differently from the other
two, so its wiring keeps an asymmetric branch.

*Why this shape.* Keeping the existing Error-first branch intact (rather than folding
it into the leaf set) preserves the `Required<Error>` behavior users depend on, and
with `ErrorPolicy = true` the branch behaves exactly as before. The helper carrying
only two of the three flags mirrors where each member is actually consumed —
`ErrorPolicy` rides along through every recursion even though only the first branch
reads it, since the recursion signature carries the whole group.

*Production role.* Required-ness rewriting including container instantiations.

### `lib/deep-readonly/index.ts` (58 insertions, 29 deletions)

*What changed.* The one utility that *already had* the group as an options record is
rebuilt in the opposite direction of its history: `ConditionalBuiltin`,
`DeepReadonlyObjectWithOptions` and `DeepReadonlyWithOptions` replace their single
`Options extends Required<DeepReadonlyOptions>` parameter with three separate flags
`BuiltinDate`, `BuiltinError`, `BuiltinRegexp` (constraint `extends boolean`), and the
exported `DeepReadonly` — which keeps its public object-style
`OverrideDeepReadonlyOptions` parameter — computes each flag by indexing the merged
record: the same `CreateTypeOptions<...>` expression is written out three times, once
per family (`...["date"]`, `...["error"]`, `...["regexp"]`).

*Why this site.* Users pass `DeepReadonly<T, { builtin: { date: true } }>`; the public
record contract must not change. At the same time the internal layers had to adopt the
same loose-flag threading the rest of this change introduced, since the options record
was the only bundled representation of the group left in the family.

*Why this shape.* Lowering the record to flags at the public boundary is the minimal
edit that lets the internal aliases match their new siblings module-for-module; the
cost is that the merge itself is now spelled three times, and that the leaf-condition
orientation (`Flag extends false ? Family : never`) inherits the record's old polarity,
which reads differently from the `*IsBuiltin` flags in `deep-writable`.

*Production role.* Deep immutability rewriting; the library's second flagship mutability
type, including the iterator-protocol mapper.

### `lib/deep-pick/index.ts` (31 insertions, 10 deletions)

*What changed.* Private `PickBuiltin<IsBuiltinDate, IsBuiltinError, IsBuiltinRegExp>`
replaces `Type extends Builtin`; `DeepPick<Type, Filter>` gains three trailing flags
(all `false`) forwarded at the 10 recursion sites of the filter traversal, most of
which are curried behind the pairwise `Filter` comparison.

*Why this site.* Structural filter traversal shares the built-in leaf question but was
never given a knob; picking from date/error-bearing containers is a recurring request.

*Why this shape.* Flags of the form `Flag extends true ? Family : never` with `false`
meaning "is a leaf" — note this is the mirror polarity of the equally local
`IsBuiltinDate` naming, where `false` is what preserves today's behavior.

*Production role.* Deep structural picking used by its own strict wrappers.

### `lib/deep-omit/index.ts` (33 insertions, 10 deletions)

*What changed.* Private `OmitBuiltin<BuiltinDatePolicy, BuiltinErrorPolicy,
BuiltinRegexpPolicy>` replaces `Type extends Builtin`; `DeepOmit<Type, Filter>` gains
three trailing flags (all `false`) forwarded at its 10 recursion sites; the final
mapped-type branch was reformatted because its key output line grew past the project's
print width.

*Why this site and shape.* Omit is pick's twin and is kept in lockstep with the
`deep-pick` treatment so the two files continue to read as a pair (same polarity,
differently spelled names). Defaults reproduce the `Builtin` stop.

*Production role.* Deep structural omission, also wrapped by strict consumers.

## Deliberate structural variation across the sites

The diff intentionally does not normalize the new mechanism to one template:

- **Naming**: `DatePolicy` / `ErrorPolicy` / `RegexpPolicy` (deep-partial, deep-required),
  `DateIsBuiltin` / `ErrorIsBuiltin` / `RegExpIsBuiltin` (deep-writable),
  `IsDateBuiltin` / `IsErrorBuiltin` / `IsRegExpBuiltin` (deep-nullable),
  `LeafDate` / `LeafError` / `LeafRegExp` (deep-undefinable),
  `BuiltinDateFlag` / `BuiltinErrorFlag` / `BuiltinRegExpFlag` (deep-non-nullable),
  `BuiltinDate` / `BuiltinError` / `BuiltinRegexp` (deep-readonly),
  `IsBuiltinDate` / `IsBuiltinError` / `IsBuiltinRegExp` (deep-pick),
  `BuiltinDatePolicy` / `BuiltinErrorPolicy` / `BuiltinRegexpPolicy` (deep-omit).
- **Flag polarity**: `true` = "family is rewritten" in some modules and `true` = "family
  is a leaf" in others, with `deep-required` mixing consumption styles *within one
  alias*: two flags feed the leaf set, the third switches a structural branch.
- **Parameter declarations**: most flags are constrained `extends boolean`; the
  `deep-undefinable` author left the three parameters unconstrained with literal
  boolean defaults.
- **Combination style**: most modules derive the leaf set with `Exclude`-over-conditionals;
  `deep-readonly` keeps its historical enumerated union (`Primitive | Function | ...`),
  and `deep-readonly` lowers the group from its pre-existing record at the entry alias.
- **Threading depth**: single-alias modules forward 10–12 times; modules with an
  object/iterator mapper or the WeakKey guards forward 14–17 times, so both shallow and
  deep carriers of the new values appear in the change.

This variation is meant to model normal growth: the same underlying question, answered
independently nine times, with each answer shaped only by the local file's conventions
and by the constraint that nothing observable changes for users who do not pass the new
parameters.

## What the change deliberately does not touch

- **Composite consumers** (`lib/buildable` = `DeepPartial<DeepWritable<Type>>`, and the
  strict pick/omit wrappers) rely entirely on the defaults and keep their existing
  signatures, so the rollout stops at the utilities themselves.
- **`lib/create-type-options`** — the generic record-merge helper used by
  `DeepReadonly` is left as-is; whatever internal form a utility picks for the group is
  decided per module in this change.
- **`lib/paths` and its option domain** (recursion depth, array index accessors) — a
  genuinely different set of settings that has nothing to do with built-in families.
- **Test and display sources** under `lib/**` (`*.test.ts`, `*.display.ts`,
  `test-types.ts`, `test-const.ts`, `ts-version.ts`): the public surface change is
  purely additive (defaulted trailing parameters), so the existing suite and docs
  continue to apply unchanged.
