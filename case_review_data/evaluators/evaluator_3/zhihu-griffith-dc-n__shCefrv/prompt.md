# Maintenance request

While adding a per-quality field to our player recently we had to touch several unrelated spots before the data even reached the playback component: the playlist parsing helpers, the shared context value, the player core's internal props, and each built-in playback component's prop type. Concretely: playlist parsing should group each entry's data; whatever distributes per-quality data to the player core and the built-in components should hand over the grouped form; and the consumers should read the fields they need from the group instead of aligning arrays by index. The utility that builds the adaptive playlist for auto quality should take the grouped per-quality data and pull bandwidth / resolution / address out of it.

I first ran into this while working around `VideoComponent` in `packages/griffith-hls/src/VideoComponent.tsx`; please start there and follow the related call path.

Please investigate this repeated parameter-passing pattern and refactor the affected path around the underlying concept, rather than continuing to coordinate the values independently.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
