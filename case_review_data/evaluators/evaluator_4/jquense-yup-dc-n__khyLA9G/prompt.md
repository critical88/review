# Maintenance request

Rebuild the internal threading around the bundles instead of the positions: values that always travel together should be carried together in a named parameter object, so a seam receives one describable thing rather than a positional list. The bundles to model are up to you, but the pipeline moves three of them a resolution context (value/parent/context-style inputs), a description of a nested validation site, and the context a queued test runs with.

I first ran into this while working around `Lazy` in `src/Lazy.ts`; please start there and follow the related call path.

Please investigate this repeated parameter-passing pattern and refactor the affected path around the underlying concept, rather than continuing to coordinate the values independently.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
