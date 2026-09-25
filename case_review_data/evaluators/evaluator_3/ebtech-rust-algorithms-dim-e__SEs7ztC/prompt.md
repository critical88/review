# Maintenance request

We mentor new contributors on this crate, and over the last several review sessions the same complaint keeps coming up about our flagship algorithms: you open the main entry point of a family and there is no "there" there. Our best guess is that this crept in through well-intentioned performance passes and an aversion to one-line indirections. Whatever the history, it is now hurting onboarding and correctness reviews: copies of the same leaf computation drift apart, and every fix has to be applied in several hand-adapted variants.

I first ran into this while working around `is_prime` in `src/math/mod.rs`; please start there and follow the related call path.

Please restore useful internal boundaries so the top-level flow coordinates the work instead of containing every implementation detail.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
