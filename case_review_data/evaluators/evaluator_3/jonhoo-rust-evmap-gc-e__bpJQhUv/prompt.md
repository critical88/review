# Maintenance request

I maintain the dual-container map in this repo (the `left-right` crate). Lately every change seems to land on the write-side handle no matter whose problem it actually is. A couple of us ran into this while wiring up a new storage backend behind the map, and I would rather clean it up now than let the next feature land on top of it. The original split was clean: the writer applies operations, publishes, and waits on readers; the read side stays in charge of everything readers do.

I first ran into this while working around `Absorb` in `src/lib.rs`; please start there and follow the related call path.

Please separate the unrelated responsibilities that have accumulated here, keeping the central object focused on genuine coordination.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
