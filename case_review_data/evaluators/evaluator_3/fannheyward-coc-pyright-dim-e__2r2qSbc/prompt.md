# Maintenance request

Several of the extension's feature code paths have drifted into very large functions, and it's slowing down maintenance. We care about the destination, not the route: you don't need to reconstruct any particular previous file layout or method naming. What matters is that a reader can tell shared machinery from feature logic, that shared machinery exists once and is called rather than copied, and that nothing observable changes for users.

I first ran into this in `src/commands.ts`; please start there and follow the related call path.

Please restore useful internal boundaries so the top-level flow coordinates the work instead of containing every implementation detail.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
