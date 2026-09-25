# Maintenance request

I've been extending the storage engine's low-level file I/O layer the file-abstraction code that every disk-backed reader, writer, journal, and file-format upgrade in this repository sits on. At some point the per-purpose capability traits that used to describe a file (reading from it, writing to it, asking it to persist changes, moving around inside it) were consolidated into a single "a file can do all of this" interface with a policy that handles which cannot honor a direction should just refuse it at runtime. Working against that design is slowing me down in three ways: Adding a new kind of file-like handle is now a big production.

I first ran into this while working around `FileWrite` in `server/src/engine/storage/common/interface/fs.rs`; please start there and follow the related call path.

Please narrow the internal contracts so each participant provides only capabilities it can meaningfully support.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
