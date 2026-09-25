# Maintenance request

During the last few migrations (the DNS library switch, the move to the new address types) we kept having to edit the same helper chains under the zone-loading code and the DNS request path. Please investigate both fat routines, work out which pieces of absorbed logic belong where, and restore a sane structure: each responsibility should live once, in a well-scoped function or method at a sensible abstraction level, with the two entry points back to orchestrating instead of reimplementing. Where an original home for a piece still exists, use it; where it doesn't, create one.

I first ran into this while working around `serve` in `server/serve.go`; please start there and follow the related call path.

Please restore useful internal boundaries so the top-level flow coordinates the work instead of containing every implementation detail.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
