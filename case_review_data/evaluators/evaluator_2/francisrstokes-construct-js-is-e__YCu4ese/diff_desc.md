# Injection design record — construct-js interface segregation

## Realistic maintenance motivation

construct-js lets a caller build binary buffers by composing `StructType`
containers of named fields. Fields fall into two behavioral families:

- **value fields** — `U8Type`/`I8Type`/…`U64Type`, the `…sType` arrays, and
  `RawStringType`/`NullTerminatedStringType`. These hold a mutable value that
  callers set and read back (`field.get()`, `field.set(...)`) in addition to
  producing bytes.
- **computed fields** — `SizeOf8Type`/`SizeOf16Type`/`SizeOf32Type`/`SizeOf64Type`
  and `Pointer8Type`/…/`Pointer64Type`. These are read-only: they report another
  target's size or a deep offset, expose a `get()` reader, and have nothing to
  mutate.

A maintainer adding generic, type-agnostic field-value accessors wants to read
or write any field's value through one uniform handle without down-casting to
the concrete class. The natural feature is a pair of exported helpers,
`readFieldValue(field: IField): unknown` and `writeFieldValue(field: IField,
value: unknown): void`, that call `field.get()` / `field.set(...)` on whatever
`IField` handle a caller holds. For these accessors to compile, the maintainer
needs `set` / `get` to be reachable through the field handle type, so the
existing value-access contract (`IValue<T>`) is folded into the buffer-shape
contract (`IField`). This record describes how that unification, and the
accessors that motivate it, were implemented.

## Normal development evolution modeled

The change models the common step where a "reasonable-looking" unification
made for a new polymorphic feature erases an existing segregation boundary.
The value contract (`IValue<T>`) and the buffer contract (`IField`) are merged
into a single `IField` so the new accessors can read and write any field
generically. As a side effect, owners that previously implemented only the
buffer contract — the read-only computed fields and the field container — are
dragged onto the merged interface and must satisfy the value members they never
use, which they do with throwing stubs. The accessors themselves expose the
cost: calling `writeFieldValue` on a read-only computed field throws (the
field's stub), and calling `readFieldValue` on a nested `StructType` throws
(the container's no-argument read guard).

## Overall injection design

The merged interface keeps the name `IField` and is typed loosely
(`set(value: unknown): void; get(): unknown`) so the existing concrete
value-field signatures (`set(value: number)`, `get(): bigint`, …) still satisfy
it under TypeScript's method bivariance without any behavior change. The
standalone `IValue<T>` interface is removed, and its eight value-field
implementors are re-pointed to declare `IField` only. The five field owners that
under the merged contract cannot support the bundled value members each receive
a throw-stub implementation: the read-only computed-field base classes and
64-bit variants add a throwing `set(value)`, and the `StructType` container
adds a throwing `set(value)` plus a no-argument throw guard inside `get`. The
two generic accessors are added at the end of the module, written against the
merged `IField` so they can call `get` / `set` uniformly.

## Per-location rationale

### 1. Interface merge (`src/index.ts`, `IField` / `IValue<T>`)
The two declarations are the seam where the dependency boundary is collapsed.
The merged `IField` keeps the old buffer members and adds the two value
members; `IValue<T>` is deleted because its responsibility is now folded in.
The loose `unknown` typing is chosen deliberately so the genuine value-field
implementations keep their precise types and behavior while still satisfying
the merged contract — the injection must preserve behavior. Production role:
the single field handle every owner now depends on, and the type the new
generic accessors are written against.

### 2. Value-field implementor clauses (8 classes: `BaseField`, `BaseArrayField`, `U64Type`, `I64Type`, `U64sType`, `I64sType`, `RawStringType`, `NullTerminatedStringType`)
Each `implements IValue<X>, IField` becomes `implements IField`. These owners
genuinely provide and use `set` / `get`, so they carry the dependency-boundary
contrast: their value members are real, while the computed/container owners'
value members will be stubs. No method body changes here, by design — the
comparison between used and unused obligations is what makes the merged
interface's cost legible.

### 3. Read-only size-of family (`BaseSizeOf` + inherited by `SizeOf8Type`/`SizeOf16Type`/`SizeOf32Type`)
`BaseSizeOf` already implemented `IField` and already exposed a real
`get()` (the target's size); the merged interface additionally requires `set`,
which a size-of field cannot honor. A throwing `set(value)` is added on the
base so the three subclass variants inherit the obligation from one site.
Role: a read-only "report this target's size" field that now appears to be
assignable through the uniform handle.

### 4. Read-only pointer family (`BasePointer` + inherited by `Pointer8Type`/`Pointer16Type`/`Pointer32Type`)
Symmetric to the size-of family. `BasePointer` exposes a real `get()` (the deep
offset) and gains a throwing `set(value)` on the base, inherited by the three
subclass variants. Role: a read-only "report a deep offset into a target
struct" field now carrying an unused mutator through the uniform handle.

### 5. 64-bit computed variants (`SizeOf64Type`, `Pointer64Type`)
Unlike the 8/16/32 families, these two are standalone classes (no shared base),
so each gets its own throwing `set(value)`. Their existing `get()` returns a
`bigint` projection of the size/offset and remains the real read path; the new
`set` is the unused obligation. Role: the 64-bit read-only computed fields.

### 6. Field container (`StructType`)
`StructType` aggregates named fields and exposes
`get<T extends ConstructDataType>(name: string): T` to fetch a child field by
name — a fundamentally different operation from a scalar value's no-argument
`get()`. Under the merged interface it must also satisfy no-argument
`get(): unknown` and `set(value: unknown)`. The `get` signature is widened to
a generic-with-default form (`get<T extends ConstructDataType =
ConstructDataType>(name?: string): T`) so existing `s.get<...>('name')` call
sites keep their types and behavior, and its first statement throws when
invoked with no name — a no-argument read guard. A throwing
`set(value: unknown)` is added for the mutator obligation. Role: the field
container that holds no scalar value of its own, now bearing both an unused
mutator and an unused no-argument reader.

### 7. Generic field-value accessors (`src/index.ts`, `readFieldValue` / `writeFieldValue`)
The motivating client of the merged interface. Each accessor takes an `IField`
handle and calls `field.get()` / `field.set(value)` directly, so they type-check
only because `set` and `get` now live on `IField`. Production role: a uniform,
concrete-type-agnostic value access API over the single field handle. They are
placed at the end of the module as plain exported functions; their bodies are
deliberately written against the merged `IField` (not a narrowed value type) to
record why the boundary was collapsed — and so that the obligations forced onto
the read-only computed fields and the container propagate to a real client, not
only to the interface declaration.

## Deliberate structural variation

The stubs are deliberately not uniform across owners. The read-only computed
families stub only `set`, because they genuinely expose and use a `get()`
reader. The container stubs both `set` and the no-argument `get`, because it
has neither a scalar value to set nor a no-argument scalar value to read. This
asymmetry is intended: it reflects the genuine difference between a read-only
computed field and a field container, and it means the value members that
each owner is forced to carry differ by responsibility rather than being one
repeated edit. The accessor client depends on the fused members broadly, so
re-establishing the segregation cannot be done by a single edit to the
interface declaration alone.
