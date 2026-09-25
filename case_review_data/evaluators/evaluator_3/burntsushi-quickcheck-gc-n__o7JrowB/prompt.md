# Maintenance request

I maintain this crate, and I need help unwinding a pattern that has quietly taken over the property-testing runtime. Over the last stretch of changes, every job that needs "the full picture of a run" was added to the runner type that drives property testing. I want these responsibilities to live with components that actually own them, so the runner coordinates a run instead of containing the whole run. The code is spread across the module that drives the runs, the crate root, and the module where arbitrary values are generated and shrunk the re-homing should extend to all of it, not just to whatever is most visible in the runner's own module.

I first ran into this while working around `VecShrinker` in `src/arbitrary.rs`; please start there and follow the related call path.

Please separate the unrelated responsibilities that have accumulated here, keeping the central object focused on genuine coordination.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
