# Maintenance request

While working on the fetch pipeline I kept ending up editing the same long argument lists. Constructing a fetcher takes the crate name and version, the repo URL, the target triple, the package metadata, the template values, the shared repo-info cache, and the signature policy -- and that same set of values shows up over and over, signature after signature. I would like that pack of construction facts to live in one place, so a fetcher and its helpers receive one value carrying them instead of listing the elements out every time.

I first ran into this while working around `is_valid_path` in `crates/binstalk-bins/src/lib.rs`; please start there and follow the related call path.

Please investigate this repeated parameter-passing pattern and refactor the affected path around the underlying concept, rather than continuing to coordinate the values independently.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
