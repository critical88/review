# Injection design record — `frickle-ngx-cache-purge-de-e`

## Maintenance motivation

`ngx_cache_purge` is a single-file nginx module. Its own `TODO.md` has carried, for
years, one standing wish next to the cache purge feature: purging a *set* of cached
entries — a prefix or wildcard purge — is currently rejected as impossible with the
existing cache implementation. That entry is the realistic seed of this case: a
maintainer periodically attempts the feature, gets partway through the module's
request path, runs into the shared-memory iteration problem described in the TODO,
backs the attempt out of the dispatch chain, and moves on. Short of deleting every
line written during the attempt, the most natural thing a busy maintainer does to
"keep the work" is exactly what this diff models: leave the helpers, leave the
response formatting pieces, leave the small computations they were preparing to
use, and restore the live dispatch to the single-entry behaviour that ships today.

Crucially, none of the retained code is reachable at runtime after the revert, and
none of it changes observable behaviour: this is a change that is inert from the
outside and yet costs every future reader real work to see past.

## Normal evolution modeled

The diff models **two attempts**, because a feature that has sat in a TODO for years
does not get tried only once:

* **Iteration one — "wildcard purge" keyed off the PURGE method.** The idea is to
  let the configured method carry a trailing `*` (or the incoming method name carry
  an embedded marker), truncate the cache key at the marker, and purge every entry
  whose key starts with that prefix. This iteration naturally adds: a marker scan at
  directive-parse time and a second scan at request time; key-prefix validation and
  truncation helpers; a prefix matcher; a hint line announcing the capability in the
  response; a token table for the accepted marker characters; a snapshot of the key
  length after evaluation; and a snapshot of the zone population during zone lookup.
  The attempt strands at the point where the single-entry purge handler would have to
  loop over shared-memory nodes with the mutex held — the TODO's original objection.

* **Iteration two — "pattern purge" response reporting.** A fresh approach: keep
  purging single entries as today, but extend the response to report how many
  entries a prefix covers. This iteration adds a text/plain response helper for the
  report, a matched-entries log helper, a per-entry removal helper that factors out
  the locked accounting to make a multi-entry loop possible, a success flag computed
  after the purge so the report can name the entry, the length of the "Matched"
  summary line in the response builder, a snapshot of the searched cache list in the
  fastcgi content handler (fastcgi being this module's original protocol and the
  natural dogfooding target; the other three protocols were never touched, matching
  the repository's own protocol-by-protocol growth), and a per-location
  `report_handler` member seeded from the core location handler so the response
  phase had a dispatch slot to take over from. This attempt dies the same way —
  the loop over in-memory nodes is the blocking problem — and gets reverted out of
  the dispatch chain, again incompletely.

Two-era drift has a further property worth designing in deliberately: naming drifts
between iterations (the first attempt left `wildcard_*`, the second `pattern_*` and
a `report_handler` slot that does not mention either), which is exactly how
abandoned work becomes hard to find by search alone.

## Overall design

The retained work appears in three materially different forms, so that the case
exercises different reading skills rather than one repeated pattern:

1. **Fully unreachable helpers and state** — internal-linkage functions and
   file-scope constants that nothing in the translation unit references anymore.
2. **Reachable-looking but never-dispatched configuration state** — the
   `report_handler` member, seeded by a merge-time block sitting right next to the
   live `conf->handler` and `conf->original_handler` seeds, which makes it look like
   part of the module's standard handler wiring while no code path ever dispatches
   through it. The location configuration type is private to this translation
   unit, so nothing outside the module could ever read it either.
3. **Values computed inside live functions and never consumed** — small leftovers
   in functions that absolutely are live and request-serving: a scan result, a
   cached count, an evaluated length, a success flag. These can only be found by
   reading the live code, since their names are ordinary local names and their
   functions are on the hot path.

Every helper references only live nginx APIs (allocator, log, mutex, buffer,
response, file helpers), so each abandoned helper is independently dead and the
case does not degenerate into one dead call cluster: the helpers neither call one
another nor reference the dead constants. The five helpers are also *mutually
inconsistent in role* on purpose — response rendering, logging, entry removal,
config-time validation, request-time matching — because a diversely-useful
half-built feature is what makes the leftover set credible and what forces a
reader to keep the whole story in mind while clearing it.

The retained pieces are deliberately spread across the module's lifecycle stages —
directive parsing, location merge, access phase, protocol content handler, zone
lookup, key construction, shared-memory accounting, finalisation, response
rendering — so that the cleanup obligation touches every stage of a purge request
instead of clustering in one corner of the file.

## Per-location record

### Registration and configuration state

* **`ngx_http_cache_purge_loc_conf_t.report_handler` member (before the struct's
  closing brace).** A `ngx_http_handler_pt` slot, declared right after the live
  `handler` and `original_handler` members, whose only intent was to hold the
  response-phase handler for the pattern report. It is written at merge time and
  never dispatched: the struct type is file-private, and no statement anywhere
  calls through it. Its production role is pure liability — an extra slot in the
  per-location configuration that every future reader of the merge logic has to
  consider.
* **Merge-time seed in `ngx_http_cache_purge_merge_loc_conf`** — `if
  (conf->report_handler == NULL) { conf->report_handler = clcf->handler; }`,
  placed directly after the live `ngx_conf_merge_ptr_value(conf->conf, ...)
  statement and *before* the live `if (conf->handler == NULL)` / `if
  (conf->original_handler == NULL)` blocks it imitates. It is the piece that makes
  the member look like standard handler wiring. Removing the member without
  removing the seed (or vice versa) leaves the module uncompilable, which is the
  coordination cost this cluster imposes.
* **Stale zero-initialisation note in `ngx_http_cache_purge_create_loc_conf`** —
  one line inside the existing `set by ngx_pcalloc()` comment naming
  `conf->report_handler = NULL`. Locations created by `ngx_pcalloc` need no
  explicit assignment, so after the revert this line is documentation of state
  that no longer exists. It is the kind of comment rot that misleads readers of
  the factory for years.

### Declaration protocol

* **Five forward declarations between the content-handler declaration and the
  `ngx_http_file_cache_purge` declaration** — `wildcard_compile`,
  `wildcard_match`, `pattern_respond`, `pattern_note`, `pattern_unlink`, in the
  module's aligned static-declaration style. Real partial reverts leave exactly
  this: the definitions' removal gets done, the declaration block gets forgotten.
  The declarations are the only place where the retired work still touches the
  module's public-looking header region of the file.

### Iteration one — wildcard helpers and state

* **`ngx_http_cache_purge_wildcard_compile`** — validates that a configured method
  value ends with `*` and is more than a bare marker, allocates and fills the
  prefix from the pattern, returning `NGX_OK` / `NGX_DECLINED` / `NGX_ERROR`.
  Config-time validation is where a purge-by-suffix approach necessarily begins;
  the helper kept the nginx idiom (parse-time check, pool allocation, `ngx_memcpy`).
* **`ngx_http_cache_purge_wildcard_match`** — `ngx_memcmp`-based prefix match of a
  request's cache key against the compiled prefix, returning `NGX_OK` /
  `NGX_DECLINED`. This is the request-side half of iteration one.
* **`ngx_http_cache_purge_wildcard_hint[]`** — a response-fragment constant
  announcing prefix purges to clients, formatted exactly like the module's
  existing page-top/page-tail pieces. It never gets spliced into any buffer.
* **`ngx_http_cache_purge_wildcard_tokens[] = "*?"`** — the accepted marker
  characters, in the style of the module's existing small constant tables.
* **`marker` scan inside `ngx_http_cache_purge_conf`** — the directive parser
  additionally scans the configured method for `*` when the method has more than
  one character. This is config-time validation from the live function's point of
  view: the computation runs on every `cache_purge` directive, and its result went
  into `wildcard_compile`. After the revert the scan keeps executing and its
  result feeds nothing.
* **`token` scan inside `ngx_http_cache_purge_access_handler`** — the access-phase
  counterpart: the incoming `r->method_name` is scanned for `*` on every request
  in a purge-enabled location. Iteration one tried both ends (configured method at
  parse time, incoming method at request time) before being abandoned; both scans
  remained.
* **`prefix_len` inside `ngx_http_cache_purge_init`** — after evaluating the
  complex-value cache key, the key length is snapshotted; the truncation that would
  have consumed it never came back with the revert.
* **`zones` inside `ngx_http_cache_purge_cache_get`** — while resolving the cache
  zone by name, the number of configured zones is recorded
  (`u->caches->nelts`), extending the live loop counter's declaration
  (`ngx_uint_t i, zones;`). The multi-zone iteration it was preparing for never
  returned.

### Iteration two — pattern report pieces

* **`ngx_http_cache_purge_pattern_respond`** — a self-contained `text/plain`
  response helper that reports the matched-entry count: builds a temporary buffer
  with `ngx_create_temp_buf`, formats the count with `ngx_sprintf`, sends the
  header and filters the chain. It mirrors the module's live
  `ngx_http_cache_purge_send_response` response discipline closely enough that a
  skimming reader can mistake the two for alternatives of the same feature.
* **`ngx_http_cache_purge_pattern_note`** — `ngx_log_debug2` note of the matched
  count and file name, in the same debug style as the live purge note in the
  content handler.
* **`ngx_http_cache_purge_pattern_unlink`** — a per-entry removal helper for the
  multi-entry loop: takes the mutex, checks the node still exists, performs the
  shared-memory size accounting (`cache->sh->size -= c->node->fs_size`), clears
  `exists` (and `updating` under the version guards, matching the live purge
  code), unlocks, and deletes the file. Its existence in addition to the near
  identical accounting inside the live `ngx_http_file_cache_purge` is precisely
  the kind of duplication aborted refactors leave behind.
* **`matched` inside `ngx_http_cache_purge_handler`** — a success flag computed
  right after the live `rc = ngx_http_file_cache_purge(r);`, matching the
  signature both report helpers above accept. The dispatch that consumed the flag
  was reverted; the flag's computation was not.
* **`suffix_len` inside `ngx_http_cache_purge_send_response`** — the response
  builder additionally computes the combined length of the success-page tail and a
  would-be "Matched: " summary line, in the same `sizeof(...) - 1` idiom as the
  live length accounting directly above it.
* **`searched` inside `ngx_http_fastcgi_cache_purge_handler`** — after the live
  `ngx_http_cache_purge_cache_get` zone resolution, the handler snapshots
  `r->upstream->caches` into a local (declaration inside the existing
  version-guarded block, matching the surrounding declaration style). Fastcgi is
  where this module's purge support began, so it is the natural protocol to have
  carried the experiment; the proxy, scgi and uwsgi siblings were left untouched,
  which keeps the live copy-paste quartet visibly asymmetric — an asymmetry real
  partial reverts are famous for.

## Deliberate structural variation summary

* Two development eras with drifted identifier vocabularies (`wildcard_*` versus
  `pattern_*` plus an era-neutral `report_handler`), so search-per-token does not
  surface the full set.
* Three different relations (unreferenced definitions; registered-never-dispatched
  configuration state; computed-never-consumed values inside live functions).
* Sites in nine distinct functions spanning all lifecycle stages of a purge
  request, from directive parsing to response rendering.
* A cluster that blocks compile-incomplete cleanups (member + seed), a cluster
  that blocks declaration-only cleanups (five forward declarations), and a local
  that shares a declaration with live code (`zones` on the `i, zones` line),
  which defeats line-granular deletion.
* One protocol deliberately affected (fastcgi) out of a four-way copy-paste
  family, mirroring the repository's protocol-by-protocol history.
* Residue names are deliberately ordinary (`marker`, `token`, `zones`,
  `prefix_len`, `matched`, `suffix_len`, `searched`, `reclaimed`), i.e., they name
  values, not the abandoned feature, which is what forces semantic reading of the
  live functions.
