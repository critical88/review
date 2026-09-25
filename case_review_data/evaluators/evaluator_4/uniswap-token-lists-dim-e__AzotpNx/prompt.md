# Maintenance request

A while back someone profiled the hot path of this library and, chasing call overhead, "flattened" some of the small pure helpers in the versioning and list-comparison area: instead of composing over the shared primitives, a few of the entry points had those primitives' logic copied into themselves and reworked into locally-styled code.

I first ran into this in `src/diffTokenLists.ts`; please start there and follow the related call path.

Please restore useful internal boundaries so the top-level flow coordinates the work instead of containing every implementation detail.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
