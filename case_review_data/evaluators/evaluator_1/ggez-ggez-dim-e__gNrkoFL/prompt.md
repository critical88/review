# Maintenance request

While chasing a frame-time regression before the last release we spent a while inside the renderer with break points everywhere, and to make that bearable we collapsed the helper indirection in the canvas drawing code so every construction argument would be visible in one place. The regression turned out to be elsewhere and, well, the flattened version shipped. The copies currently behave correctly, so the refactor must not change behavior: same GPU objects, created lazily under the same conditions and in the same order, same rendering output, no public API changes, and the crate should still build warning-free both with the default features and with the `3d` feature enabled (the 3D paths are feature-gated).

I first ran into this in `src/graphics/gpu/bind_group.rs`; please start there and follow the related call path.

Please restore useful internal boundaries so the top-level flow coordinates the work instead of containing every implementation detail.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
