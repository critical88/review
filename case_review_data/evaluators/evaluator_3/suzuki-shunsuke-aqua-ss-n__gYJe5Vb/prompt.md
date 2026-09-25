# Maintenance request

While preparing a small change to how aqua organizes the packages it ships for itself, I hit a wall I should have noticed much earlier. aqua keeps its own tools the proxy companion it uses on Windows, and the aqua binary itself when it self-installs or self-updates separate from the packages it manages for users. Please consolidate this. There should be one authoritative implementation that decides where aqua's own packages live and, as part of that, which packages count as aqua's own and every flow that needs that answer should get it from there rather than re-deriving its own.

I first ran into this while working around `wrapExec` in `pkg/controller/exec/exec.go`; please start there and follow the related call path.

Please consolidate the repeated decision or policy behind one clear owner so the next change can be made in one place.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
