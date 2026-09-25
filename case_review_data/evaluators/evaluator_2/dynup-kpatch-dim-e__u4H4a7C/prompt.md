# Maintenance request

Lately the kpatch-build tools have become painful to review. The awkward part is that the original unit-step functions mostly still exist in the shared component code, and the ones that disappeared only did so because a driver ate them — their bodies now live only inside the driver.

I first ran into this while working around `kpatch_mangled_strcmp` in `kpatch-build/create-diff-object.c`; please start there and follow the related call path.

Please restore useful internal boundaries so the top-level flow coordinates the work instead of containing every implementation detail.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
