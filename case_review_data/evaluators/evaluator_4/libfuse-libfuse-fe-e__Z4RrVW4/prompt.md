# Maintenance request

While digging through the exit paths after a stuck-unmount report I kept landing in the same surprise: I'd go looking for "what happens when a session is told to stop", and the answer turned out to be open-coded in the wrong places every time. The session record and its lifecycle live in the low-level session implementation, and it already owns the exit/close/destroy side of the story.

I first ran into this while working around `fuse_session` in `lib/fuse_loop.c`; please start there and follow the related call path.

Please move the misplaced behavior closer to the data or component that owns it, leaving the surrounding coordinator focused on its own work.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
