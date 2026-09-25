# Maintenance request

I've just lost an afternoon to this, and talking it over with the team it turns out I'm not the only one, so I want the surfaces involved cleaned up properly rather than papered over. Here is how it went for me. I was scripting a wheel-scroll interaction with the action-sequence builders, and because the builder offered me key events and pointer events on the very same object I was chaining, I mixed them in: click, type, scroll, all in one fluent chain. It compiled without a whisper, ran to completion, and the page did nothing for the key and pointer parts.

I first ran into this while working around `ActionSequence` in `src/actions.rs`; please start there and follow the related call path.

Please narrow the internal contracts so each participant provides only capabilities it can meaningfully support.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
