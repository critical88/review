# Maintenance request

I maintain the codec layer of imageflow (the Rust core). I'd like a refactor, and I'll try to explain the problem the way I ran into it. Everything decodes through one shared `Decoder` trait: a codec hands its backend to the pipeline as a `&mut dyn Decoder`, and the rest of the engine stays format-blind. That's the design, and it's the right one. But over a couple of recent changes, the trait has kept picking up "does your backend know about X? if so report it" members

I first ran into this in `imageflow_core/src/codecs/gif/mod.rs`; please start there and follow the related call path.

Please narrow the internal contracts so each participant provides only capabilities it can meaningfully support.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
