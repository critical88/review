# Maintenance request

I maintain the solutions tree for the CS:APP 3e chapters, and the last several weeks of work on it fixing the byte-extraction errata, writing up comparison tables for the writeups, debugging the allocator exercises, and merging a draft of the system-wrapper answer into the shipped version has left the sources dirtier than I'd like. Everything still builds and every self-test still passes, which is exactly why nobody noticed: the residue never runs, so the programs can't complain about it. But I keep re-reading it, re-indenting it, and explaining to readers why it's there.

I first ran into this while working around `odd_ones` in `site/content/chapter2/code/odd-ones.c`; please start there and follow the related call path.

Please remove the obsolete path and the production scaffolding that exists only to support it, while keeping the active flow straightforward.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
