# Maintenance request

The `notify-debouncer-full` crate has been growing by absorption for a while, and it has reached the point where working in it is actively slowing us down. Onboarding feedback says it all: it takes about an hour to trace how one raw event becomes a queued, de-duplicated, renamed, delivered event, because no step along the way has a single obvious owner.

I first ran into this in `notify-debouncer-full/src/file_id_map.rs`; please start there and follow the related call path.

Please separate the unrelated responsibilities that have accumulated here, keeping the central object focused on genuine coordination.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
