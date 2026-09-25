# Maintenance request

We need help untangling the zig linker integration in this crate. Last quarter one of us spent weeks inside this layer chasing build failures that only showed up on specific zig versions or specific platforms (macOS depends on SDKROOT handling and text-based stub files, windows-gnu needs its dlltool picked the right way, ARM targets need their little support files in place, and the zig 0.16 argument changes rippled everywhere).

I first ran into this in `src/zig/cargo_env.rs`; please start there and follow the related call path.

Please restore useful internal boundaries so the top-level flow coordinates the work instead of containing every implementation detail.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
