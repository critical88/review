# Maintenance request

Last week I finally finished the cleanup I'd been putting off: I pulled all the repeated "open the memory file, read my collection, paste it back" ceremony out of this CLI's record-backed features and put it into a small store module next to the existing memory package. One class per collection tasks, tags, todos, reminders, and the command routines each owning its keys and its save.

I first ran into this in `jarviscli/packages/memory/record_store.py`; please start there and follow the related call path.

Please move the misplaced behavior closer to the data or component that owns it, leaving the surrounding coordinator focused on its own work.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
