# Maintenance request

I ran into this while extending the accessory protocol pairing flow, and it is starting to cost real time. Some earlier performance-oriented rework in this repository flattened the helper organization of the TLV marshalling code (`src/tlv.c`) and of the accessory state replication code (`src/accessories.c`). So: please get these two modules back to a delegated decomposition.

I first ran into this while working around `homekit_value_copy` in `src/accessories.c`; please start there and follow the related call path.

Please restore useful internal boundaries so the top-level flow coordinates the work instead of containing every implementation detail.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
