# Maintenance request

I was tracing a latency report on the send path this week and lost half a day. To follow a single application write I had to read the embedded protocol engine's internals from *inside the connection object*: the connection slices the application buffer by the engine's own segment-size field, walks the engine's send queue to grow segments, decides fragment countdowns itself, and pushes into the queue directly. it overwrites the decoder's shard geometry, empties its shard sets, reallocates its caches, replaces its codec and clears its flags. The detector used to be a pure signal analyzer: sample the stream, report the pulse period.

I first ran into this while working around `FindPeriod` in `autotune.go`; please start there and follow the related call path.

Please move the misplaced behavior closer to the data or component that owns it, leaving the surrounding coordinator focused on its own work.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
