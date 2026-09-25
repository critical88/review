# Maintenance request

This repository is serf, the cluster membership and orchestration library. Nodes gossip, track member state, and can run named queries that other nodes answer: a node issues a query, matching nodes acknowledge and/or respond, responses can be relayed back through other members for redundancy, and the cluster tries to rejoin members that have failed. I was adding a small feature to the query machinery — acknowledging relayed response copies — and expected it to be a weekend-sized change. It was not, and the reason was not the feature. It was the plumbing.

I first ran into this while working around `NotifyMsg` in `serf/delegate.go`; please start there and follow the related call path.

Please investigate this repeated parameter-passing pattern and refactor the affected path around the underlying concept, rather than continuing to coordinate the values independently.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
