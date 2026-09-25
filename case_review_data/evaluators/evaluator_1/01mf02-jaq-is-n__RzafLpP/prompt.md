# Maintenance request

I was helping someone embed jaq with a custom value type this week and it ended in a long Slack thread. They support a decent chunk of jq but their value type has no concept of UTF-8 string internals, slicing, or byte access and yet the trait they have to implement in `jaq-core` now makes them provide the entire string/number/sequence surface anyway. Please straighten this out across the codebase: put the capability requirements back on the right side of each interface, focused and cohesive, so that a value type implements the string/number capabilities only if it has them, and a data kind that has no input stream is not made to pretend it has one.

I first ran into this while working around `DataT` in `jaq-core/src/data.rs`; please start there and follow the related call path.

Please narrow the internal contracts so each participant provides only capabilities it can meaningfully support.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
