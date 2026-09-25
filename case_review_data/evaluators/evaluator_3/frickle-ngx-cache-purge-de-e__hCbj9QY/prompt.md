# Maintenance request

While getting re-acquainted with `ngx_cache_purge` I keep stumbling over remains of the abandoned multi-purge ideas — the experiments around removing a *set* of cached entries that we gave up on years ago (the TODO's objection about walking shared-memory nodes under the mutex still stands, so let's not reopen that question). Two things to keep in mind while you dig: the experiments were tried more than once and their naming drifted in between, so do not assume one search term will surface the whole set; and the live single-entry purge path must keep behaving exactly as it does now, including our compatibility build for configurations without `NGX_HTTP_CACHE` and the version guards for older nginx releases.

I first ran into this while working around `ngx_http_cache_purge_init` in `ngx_cache_purge_module.c`; please start there and follow the related call path.

Please remove the obsolete path and the production scaffolding that exists only to support it, while keeping the active flow straightforward.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
