# Maintenance request

Reading `Fst::get` the other week to chase down some lookup latency, I wandered into `src/raw/` and then kept wandering, because nearly every function I opened had the same silhouette: a long parameter list where the same few values show up again and again, unpacked and passed along by hand. None of it is broken — everything works, and the format is untouched. But the structures that used to define the boundaries here have dissolved, and the cost is showing up as friction: every new reader duplicates the handoff, the decoders no longer advertise what they actually consume, and the two registries have drifted toward field-level bookkeeping that nobody intended as a design.

I first ran into this in `src/raw/build.rs`; please start there and follow the related call path.

Please investigate this repeated parameter-passing pattern and refactor the affected path around the underlying concept, rather than continuing to coordinate the values independently.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
