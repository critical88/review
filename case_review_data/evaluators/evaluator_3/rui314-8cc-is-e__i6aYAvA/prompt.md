# Maintenance request

You have been maintaining this small C-to-x86-64 compiler. A while back someone folded all of the compiler's reading and writing behind a single I/O abstraction so the backends would be interchangeable. It compiles everything the same as before and the full test suite is green, but the abstraction has been bothering you, and you want to clean it up. Here is what you noticed.

I first ran into this in `8cc.h`; please start there and follow the related call path.

Please narrow the internal contracts so each participant provides only capabilities it can meaningfully support.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
