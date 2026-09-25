# Maintenance request

Working in this repo (the `firebase/geofire-js` monorepo: the `geofire-common` algorithms package and the `geofire` Firebase wrapper with its realtime `GeoQuery`), we keep bumping into the same awkwardness while reviewing and debugging the coordinate-handling paths. Somewhere along the way, an internal refactor started handing the two coordinates around as raw numbers instead of as the `[latitude, longitude]` values the API is built around.

I first ran into this in `packages/geofire-common/src/index.ts`; please start there and follow the related call path.

Please investigate this repeated parameter-passing pattern and refactor the affected path around the underlying concept, rather than continuing to coordinate the values independently.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
