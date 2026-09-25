# Maintenance request

We had just added a couple of generic helpers — `readFieldValue(field: IField)` and `writeFieldValue(field: IField, value)` — so callers can read or write any field's value through the one field handle without down-casting to the concrete type. To make those helpers work against the handle, the field interface was unified so that every field implements a single common `IField` carrying `computeBufferSize`/`toUint8Array` together with `set`/`get`.

I first ran into this while working around `AlignmentPadding` in `src/index.ts`; please start there and follow the related call path.

Please narrow the internal contracts so each participant provides only capabilities it can meaningfully support.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
