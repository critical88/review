# Maintenance request

I was writing an internal note about our Cloudflare `next-on-pages` runtime templates and got stuck on something in the suspense cache plumbing. The internal suspense cache is what serves Next.js incremental-cache requests inside the generated worker, backed by either a Workers KV namespace or the Cache API, whichever the deployment provides. A little while ago we made "a suspense cache adaptor" a single declared contract so the pieces of that subsystem could be exchanged freely.

I first ran into this in `packages/next-on-pages/templates/_worker.js/utils/cache.ts`; please start there and follow the related call path.

Please narrow the internal contracts so each participant provides only capabilities it can meaningfully support.

Preserve existing user-visible behavior and public compatibility. Keep the change focused and use the repository’s existing tests and build checks to verify it.
