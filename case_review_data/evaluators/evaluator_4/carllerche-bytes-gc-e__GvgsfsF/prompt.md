# Maintenance request

I was trying to make a small write-side change in the buffer adapter code (the `buf` module — the adapters around `Buf`/`BufMut` that give us windowed reads and writes, two-region chaining, the `io::Read`/`io::Write` bridges and byte iteration), and I kept losing track of where a rule actually lives. Somewhere along the way, the cursor rules of all of those adapters were pulled into one internal type, and the adapters themselves now just forward to it.

I first ran into this in `src/buf/chain.rs`; please start there and follow the related call path.

Please separate the unrelated responsibilities that have accumulated here, keeping the central object focused on genuine coordination.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
