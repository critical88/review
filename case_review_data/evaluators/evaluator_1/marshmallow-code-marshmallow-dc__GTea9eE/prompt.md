# Refactor: nested-schema field-selection options travel as a group

## Observation

marshmallow propagates the field-selection options `only`, `exclude`,
`load_only`, and `dump_only` from a `Schema` into its `Nested` fields. A recent
refactor extracted that propagation into a set of free helper routines, but the
four options still pass through every helper as four separate positional
parameters, and each helper takes whichever subset it needs. The four always
describe the same thing — which fields a nested schema keeps and in which
direction — and they share the schema-to-nested selection lifetime, so changing
any one of them, or how it is derived, means updating a signature and every
caller.

## Diagnosis

These four options form a cohesive group: every one is a collection of field
names with the same lifetime, and they travel together across the helpers in the
nested-field-selection subsystem. The current design decomposes that group and
re-threads it as individual parameters at every helper boundary, which is a
data clump — the same set of values handed around together without a single
abstraction to carry them.

## Scope

The subsystem is nested-schema field selection: the free helper routines that
carry the selection options across propagation boundaries, together with the
call sites that drive them from `Schema` and the `Nested` field. The relevant
responsibilities are roughly the schema's nested-option normalization and the
nested field's two schema-materialization paths (intersecting/unioning
`only`/`exclude` onto an existing nested-schema copy, and building a nested
schema from a registered class by name).

## Desired outcome

Bundle the four field-selection options into a single abstraction that carries
the cohesive group as one value, and thread that value through the helper
routines and their call sites instead of the four separate parameters. The
helpers should read the option they need from the bundled value, and the call
sites should construct the bundled value where the selectors originate. The
extraction/convenience the refactor added should remain; a repair that simply
inlines everything back into the classes does not bundle the group.

## Behavior that must stay stable

- `only`/`exclude` keep their current dotted-name handling: the top-level
  `only` keeps the parent field of each dotted entry (`"nested.field"` ->
  `"nested"`), and the top-level `exclude` drops dotted entries because the
  nested schema excludes them.
- `load_only`/`dump_only` keep their current prefix resolution: `"name.field"`
  on the root becomes `"field"` on the nested schema, and root entries that do
  not start with the nested field's prefix are dropped.
- On a copied existing nested instance, `only` is still intersected with the
  nested schema's existing `only` (or all field names when it is `None`) and
  `exclude` is still unioned with the nested schema's existing `exclude`.
- When a nested schema is built from a registered class by name, `many`/`only`/
  `exclude` still pass through from the parent `Nested` field and
  `load_only`/`dump_only` are still resolved from the root.
- `only = None` still means "all fields"; invalid `only`/`exclude`
  (non-collection strings, unknown field names) still raise the existing errors
  at the same points.
- The public `Schema` and `Nested` field APIs and the full marshmallow test
  suite stay unchanged.

## Out of scope

Do not change the semantics of `only`/`exclude`/`load_only`/`dump_only`, the
nested-field selection algorithm, serialization/deserialization behavior, or the
public field/schema API surface.
