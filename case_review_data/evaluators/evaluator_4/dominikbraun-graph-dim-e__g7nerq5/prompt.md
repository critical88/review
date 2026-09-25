# Maintenance request

This Go graph library (`github.com/dominikbraun/graph`, root package `graph`) has become hard to work on. Colleagues keep bumping into the same thing: the algorithm functions are enormous, and reviewing even a small change means re-deriving how some hand-rolled slice-shuffling keeps track of things. I noticed a particularly odd detail while debugging this week: one of the all-paths functions raises an internal `unable to remove layer: empty stack` error, and a second one is produced a few lines later, but there is no stack anywhere near that code - the library's own stack helper doesn't appear at all in that file.

I first ran into this while working around `contains` in `collection.go`; please start there and follow the related call path.

Please restore useful internal boundaries so the top-level flow coordinates the work instead of containing every implementation detail.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
