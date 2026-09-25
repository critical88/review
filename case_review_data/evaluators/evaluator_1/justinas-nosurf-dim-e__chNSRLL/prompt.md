# Maintenance request

I finally had time to read the middleware core properly, and the request handling path of this CSRF library is in rough shape. A while back a change was merged that was supposed to shave call overhead off the per-request path; nobody can find the benchmarks, but the code that landed reads like someone transcribed an entire call tree into each entry function by hand and deleted the intermediate steps.

I first ran into this while working around `defaultFailureHandler` in `handler.go`; please start there and follow the related call path.

Please restore useful internal boundaries so the top-level flow coordinates the work instead of containing every implementation detail.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
