# Maintenance request

Over the last several rounds of IEEE 802.15.4 / 6LoWPAN bring-up and DHCPv4 lease work, a few of our packet-handling methods have kept growing. Go over those areas and restore a maintainable structure: separate the interleaved protocol stages back into well-named, well-scoped functions, so a reader can see the stage sequence of each path; collapse repeated per-branch copies of the same stage into one shared implementation; use your judgment about where the boundaries belong; keep any inlining that is genuinely one small step rather than a stage, and do not add indirection for its own sake; leave doc comments and stage notes consistent with whatever structure you end up with.

I first ran into this in `src/iface/interface/mod.rs`; please start there and follow the related call path.

Please restore useful internal boundaries so the top-level flow coordinates the work instead of containing every implementation detail.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
