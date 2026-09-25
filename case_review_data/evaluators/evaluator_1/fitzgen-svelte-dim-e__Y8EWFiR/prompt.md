# Maintenance request

A few of the `twiggy` size analyses have become hard to read. The top-level analysis function for each collects the items to report, filters them by the user's flags, works out the per-row numbers, and builds the summary rows all in the same body. When you split things up, please cut at the genuine stage boundaries rather than just hoisting the whole body into a single helper — the point is to get the distinct responsibilities back on their own, not to relocate the same tangle under a new name. If the dominators analysis ends up wanting the reachability walk that the garbage analysis already has, share it instead of keeping a private copy.

I first ran into this while working around `dominators` in `analyze/analyses/dominators/mod.rs`; please start there and follow the related call path.

Please restore useful internal boundaries so the top-level flow coordinates the work instead of containing every implementation detail.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
