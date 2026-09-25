# Maintenance request

Before the last deployment we started keeping per-connection activity records. While the daemon tries strategies on a connection, we note what happened during that treatment cycle: which strategy is on, how many we have burned through, whether the client's request and the server's response came through, whether the censor hit the connection with a reset, which discrepancies our insertion packets used, and the server-side TTL our crafted packets assume. Now that we are actually using these summaries, maintenance hurts.

I first ran into this while working around `tcpinfo` in `src/cache.c`; please start there and follow the related call path.

Please move the misplaced behavior closer to the data or component that owns it, leaving the surrounding coordinator focused on its own work.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
