# Maintenance request

A few weeks back we shipped the container wiring diagnostics feature in this package (the area under `src/diagnostics`, re-exported from the package root): you can record a container's bindings as a snapshot, and then print a wiring report, pull a few statistics, diff two recordings, save a recording to a JSON file and load it back, and check a recording against a live container. The problem showed up as soon as the first change requests arrived.

I first ran into this in `src/diagnostics/binding_digest.ts`; please start there and follow the related call path.

Please move the misplaced behavior closer to the data or component that owns it, leaving the surrounding coordinator focused on its own work.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
