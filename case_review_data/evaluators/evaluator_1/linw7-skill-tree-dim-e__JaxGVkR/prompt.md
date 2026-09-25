# Maintenance request

I maintain a small C project whose core is a red-black tree. Everything to do with the tree lives in the module under `code/RBTree`: the public header describes the small set of things you can do with a tree, and the implementation hides the red-black mechanics behind private helpers. Concretely, this hurts: when a rebalancing case needed correcting last week, I had to re-derive where inside the operation the fixup logic now lives, piece together which inline fragment corresponds to which classic case, and check every other place the same fragment might have been copied to.

I first ran into this while working around `rb_free_subtree` in `code/RBTree/RBtree.c`; please start there and follow the related call path.

Please restore useful internal boundaries so the top-level flow coordinates the work instead of containing every implementation detail.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
