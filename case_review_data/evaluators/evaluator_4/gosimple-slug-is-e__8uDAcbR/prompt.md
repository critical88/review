# Maintenance request

Do it the way you think it should be structured — we don't care whether focused contracts remain somewhere or whether the indirection disappears where it never paid for itself — but the generated slugs must be completely unchanged.

I first ran into this while working around `bgSub` in `languages_substitution.go`; please start there and follow the related call path.

Please narrow the internal contracts so each participant provides only capabilities it can meaningfully support.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
