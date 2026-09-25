# Maintenance request

We maintain a deployment of sorl-thumbnail, and last week we finally tried to do something that should have been trivial: give our thumbnail cache a fresh namespace so old metadata rows would stop being reused after a settings migration. We expected to change one thing. Instead, the work kept dragging us deeper into the cache internals. None of the little helpers that used to centralize these decisions exist anymore; each site is a self-contained copy of the same knowledge. This did not come from one bad commit it accumulated through individually reasonable performance and precision tweaks.

I first ran into this while working around `ThumbnailBackend` in `sorl/thumbnail/base.py`; please start there and follow the related call path.

Please consolidate the repeated decision or policy behind one clear owner so the next change can be made in one place.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
