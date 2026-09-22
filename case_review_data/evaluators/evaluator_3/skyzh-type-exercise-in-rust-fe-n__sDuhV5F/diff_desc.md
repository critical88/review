# Injection design record: structural reporting additions

## Maintenance motivation

The type-exercise workspace teaches how a typed vectorized expression engine
grows: each chapter adds a physical array family, a scalar family, or an
evaluation and binding capability on top of the previous one. The course's
storage types (`StringArray`, `DecimalArray`, `ListArray`, `PrimitiveArray`)
each encapsulate payloads, offsets, and validity behind accessors, and the
binder in `type-exercise/expr` pairs every runtime expression with its checked
logical signature.

A natural next step for operating such an engine — and one many real engines
take — is a small structural reporting layer. Someone triaging why a physical
plan is wide, or why one batch is heavier than another, wants to answer
"how many bytes does this column family actually store, and how much of that
is null bookkeeping?" without reconstructing each family's storage layout by
hand. That motivates a per-family storage audit plus a couple of plan-shape
narration helpers, which is what this change adds.

## Modeled development evolution

The change models how a maintainer would extend the course in one sitting:

1. Introduce one reporting type and a few family-agnostic helpers in a new core
   module, so all audits speak the same vocabulary.
2. Write one audit routine per physical array family, reading that family
   through its public accessors.
3. Add plan-level narration helpers that summarize batches and bound
   expressions, since the same storage-question surfaces when an operator
   wants to log what it was actually handed.
4. Wire the new modules into the crate re-export surface, as every other core
   module is wired.

## Overall design

- `type-exercise/core/src/layout.rs` (new): the reporting type `ColumnLayout`
  (`storage_bytes`, `row_count`, `null_rows`, `boundaries`, an optional
  family-specific `descriptor`), a derived `bytes_per_row` convenience, and two
  helpers that need no particular family: `element_byte_width(&PhysicalType)`
  and `count_set_bits(&BitVec)`.
- Four audit functions in the same module, one per family:
  `string_array_layout(&StringArray)`, `decimal_array_layout(&DecimalArray)`,
  `list_array_layout(&ListArray)`, `i32_array_layout(&I32Array)` — each takes
  the array by reference and produces one `ColumnLayout`.
- `type-exercise/core/src/expression/shape.rs` (new): two helpers that render
  compact batch-shape lines, `describe_plan_inputs(&[ColumnViewImpl])` and
  `first_plan_input_line(&[ColumnViewImpl])`.
- `type-exercise/expr/src/binder.rs`: one added narration function,
  `describe_declared_inputs(&BoundExpression)`, plus the small
  `output_physical_type` accessor it uses for the physical half of the line.
- Wiring: `mod layout;` + re-export in `type-exercise/core/src/lib.rs`, and
  `mod shape;` + re-export in `type-exercise/core/src/expression/mod.rs`.

Nothing else in the workspace is touched; existing evaluation, binding, and
array behavior does not shift.

## Per-site rationale

### `layout.rs` — `ColumnLayout` and family-agnostic helpers

Placing a small vocabulary module beside `array` and `column` follows the
workspace's habit of grouping explanatory types close to what they describe.
`element_byte_width` exists because two audits (decimal and list) need the
slot width implied by a `PhysicalType`, and it keeps each audit from open-coding
its own width table. `count_set_bits` exists because three audits reduce a
`BitVec` validity to a count of valid rows the same way. `bytes_per_row` is a
consumer-side convenience for the most common follow-up question.

### `layout.rs` — `string_array_layout` (E1)

String storage is the family with the most pieces — payload bytes, row
boundaries, packed validity — so it is the natural first audit site. The audit
reads `data()`, `offsets()`, `validity()`, and `len()` through the array's
public accessors and produces the shape an operator logging a batch actually
wants: payload bytes, row and null-row counts, and one fewer boundary than
stored offsets.

### `layout.rs` — `decimal_array_layout` (E2)

Decimal is the only family carrying per-type-width metadata, so this site
introduces the audit's `descriptor`: a `(precision, scale)` pair obtained from
`decimal_type()`, alongside the values/validity/len reads used to compute
storage bytes at a fixed 16-byte slot width.

### `layout.rs` — `list_array_layout` (E3)

List storage nests a child array, so this audit estimates the payload by
multiplying the child's row count by `element_byte_width` of the child's
`element_type()`. It also exercises a different validity representation
(`&[bool]` instead of a packed `BitVec`), which is exactly the kind of
family-specific detail an audit layer is supposed to absorb.

### `layout.rs` — `i32_array_layout` (E4)

The fixed-width integer family is the baseline storage story, and its audit
adds the empty-array guard (`is_empty()`) that the other families do not need.
Reading it through `values()`, `validity()`, `len()`, and `is_empty()` gives
the audit its slots, nulls, and rows at a 4-byte slot width.

### `shape.rs` — `describe_plan_inputs` and `first_plan_input_line` (E5)

Operators that accept an erased batch want one line per input column: the
`physical_type()`, whether the column `is_empty()`, the verified `len()`, and
a leading value from `get(0)` rendered by scalar family. This module pairs the
per-column renderer with a minimal `first input: …` variant covering the
common single-column log. It lives under `expression/` because it narrates the
batches expressions consume, and it is wired through the module's re-export
like its siblings.

### `binder.rs` — `describe_declared_inputs` (E6)

Binding failures and physical-kernel selection both benefit from one line that
shows which declared logical inputs produced which physical kernel, so this
helper renders `physical_name()`, the declared `input_types()`, the logical
`output_type()`, and its `output_physical_type()` reload. The physical half
needs the small `output_physical_type` accessor added to `BoundExpression`,
which keeps the narration self-contained on the hoisting type.

## Scope note

The audits and narration helpers form a reporting layer only. Array and
expression behavior, builder and binder contracts, and the published test
surface of the workspace are untouched by this change; the layer is additive.
