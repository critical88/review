# Maintenance request

We just spent a day being careful in the wrong place, and it was our own fault. A couple of changes ago we deduplicated the receive-side accounting that both read paths were repeating inline, and the sliding-window arithmetic that the v2 write path recomputed locally. Good instinct. But the shared helpers went onto the session, with the stream they operate on handed in as a parameter.

I first ran into this while working around `returnTokens` in `session.go`; please start there and follow the related call path.

Please move the misplaced behavior closer to the data or component that owns it, leaving the surrounding coordinator focused on its own work.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
