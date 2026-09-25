# Maintenance request

We maintain **vue-loader** (this repo): it turns `*.vue` single-file components into webpack modules. Last week I picked up a small bug in how template reloads are emitted for scoped components. That fix should have been a three-line change to the hot-reload part of the loader — instead it became a careful expedition through `src/index.ts`, because everything about processing a `.vue` request now lives together in one class in that file. While in there I noticed how much the entry has absorbed over time. The same class now

I first ran into this in `src/cssModules.ts`; please start there and follow the related call path.

Please separate the unrelated responsibilities that have accumulated here, keeping the central object focused on genuine coordination.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
