# Maintenance request

unstorage grew a `maxItemSize` option recently: configure a maximum stored value size on a driver and its write path refuses oversized values. Users are piling into exactly the situations this was meant for keeping one huge SSR payload from evicting the in-memory cache, avoiding `QuotaExceededError` in browser stores, not shipping a 30 MiB blob over HTTP for nothing.

I first ran into this while working around `DRIVER_NAME` in `src/drivers/capacitor-preferences.ts`; please start there and follow the related call path.

Please consolidate the repeated decision or policy behind one clear owner so the next change can be made in one place.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
