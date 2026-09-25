# Maintenance request

I spent last week getting `cross` ready for an additional architecture. Adding it in the place where our container platform information is defined was easy. The surprise came afterwards: some builds picked up the new platform correctly, others still treated the same target as unsupported - and a couple of paths began disagreeing with each other about what the target implied at all. All of this should live in one place, and everything else should ask that place.

I first ran into this while working around `get_possible_image` in `src/config.rs`; please start there and follow the related call path.

Please consolidate the repeated decision or policy behind one clear owner so the next change can be made in one place.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
