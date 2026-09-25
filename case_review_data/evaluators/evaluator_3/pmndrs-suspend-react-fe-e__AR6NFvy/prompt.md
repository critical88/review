# Maintenance request

I lost an afternoon to this while adding a small expiry tweak to the request cache, so filing it while it still hurts. Backstory: the cache used to be one file — a record-shaped type next to a module-level array with everything inlined. We've since pulled the record and the live-record collection out into real typed structures of their own, with the plain array/promise helpers beside them, and the render-phase request lifecycle in its own module. The entrypoint now just wires up the public operations. So far, so good.

I first ran into this in `__tests__/clear.test.ts`; please start there and follow the related call path.

Please move the misplaced behavior closer to the data or component that owns it, leaving the surrounding coordinator focused on its own work.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
