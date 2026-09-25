# Maintenance request

I went hunting for a bug in the amfid cdhash path and kept tripping over the same thing: the embedded-signature superblob is being unpacked by hand, again and again, wherever a signature happens to be within reach. The validation walk that is supposed to be checking the Mach-O header reaches into the superblob's fields to second-guess the signature's shape. This worries me beyond tidiness. We keep these tools in step with what the platform enforces, and when those rules move — an index-count bound, which SHA flavors are usable, what a length that doesn't match its container means — someone now has to find every re-implementation and fix them in sync.

I first ran into this in `AppSyncUnified-installd/cdhash.m`; please start there and follow the related call path.

Please move the misplaced behavior closer to the data or component that owns it, leaving the surrounding coordinator focused on its own work.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
