# Maintenance request

While triaging a bundle-size complaint I compared what we ship with what can actually run, and there is a chunk of weight we cannot justify: the compatibility paths we kept around for the pre-v3.0 print flow when the project moved to the hook API, and then decided to abandon. The migration finished a long time ago; nothing in this package can ever turn those paths on.

I first ran into this in `src/consts.ts`; please start there and follow the related call path.

Please remove the obsolete path and the production scaffolding that exists only to support it, while keeping the active flow straightforward.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
