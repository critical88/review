# Maintenance request

Yesterday I spent an afternoon teaching a new toast option that every `<Toaster>` should be able to configure and every single toast should be able to override — the kind of thing this library promises makes adding options easy. I want to stop doing this. Please make the rules for how a toast inherits these look-and-feel options live in one place, so that the next person changing one precedence (or adding an inheritable option) edits one module and reads one answer — with every module that merely *uses* these values asking that place instead of deciding again.

I first ran into this in `src/appearance.ts`; please start there and follow the related call path.

Please consolidate the repeated decision or policy behind one clear owner so the next change can be made in one place.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
