# Injection design record — scattered store identity scheme (sorl-thumbnail)

## Realistic maintenance motivation

sorl-thumbnail answers an operational question every deployment eventually
hits: *which* cached thumbnails and metadata rows belong to this installation,
and how do you invalidate all of them at once? The library owns two derived
identifier families for this. The key-value store persists every record under
a composite raw key: the configured key prefix, a record-identity segment
(`image` or `thumbnails`) and the record key, glued with `||`. Independently,
images and thumbnails are identified in the store by a short salt-joined MD5
digest, and the destination filename of a generated thumbnail digests the
source identity, geometry and options so identical requests map to identical
files.

Operations teams periodically need to change something about these schemes:
bump a scheme segment so old metadata and files are not reused after a
configuration migration, isolate one site's rows from another's in a shared
cache, or adjust the digest because of a collision or a hashing policy. Every
such request is *one conceptual change* to one derived-identifier scheme.
The motivation for this case is the situation maintainers keep ending up in
when such a change is attempted: the scheme anatomy turns out to be known
independently by many separate places in the store layer and the identity
layer, so what should be a single edit becomes a hunt through several files
and classes whose copies all have slightly different shapes.

## Normal development evolution being modeled

How does a centralized scheme end up scattered in real projects? Rarely by
design — it accumulates through locally reasonable, well-intentioned commits:

* a hot-path micro-optimization replaces a helper call with the equivalent
  one-liner written where it is used, justified by "keeping the loop
  self-contained";
* a precision fix in one flow (match *exactly* the keys this library owns,
  not anything that merely shares the prefix) bakes the scheme anatomy into
  that flow's own code instead of asking the scheme's owner for a suitable
  match term;
* once the last caller is gone, the now-unused helpers are deleted as "dead
  code";
* each fragment is written by a different context, so they don't even look
  alike — a percent format here, an f-string there, a plain concatenation
  elsewhere.

This is the evolution modeled here: the injected state is what the library
plausibly looks like after such a series of changes, at the point where a
maintainer who wants to touch the scheme finds no single place to edit.

## Overall injection design

The clean checkout concentrates each scheme in exactly one function: the
composite raw key is built and un-built by a pair of module-level helpers in
the key-value store base module, and the identity digest is computed by a
single helper in the helpers module. The injection removes those central
owners and leaves every genuine consumer of the two schemes with its own
private inline copy:

1. the four record-lifecycle operations of the base store driver (read,
   write, delete, scan) each rebuild the composite raw key in their own body —
   in four deliberately different syntactic shapes — and the scan also
   dismantles raw keys itself to return the bare record keys it yielded
   before;
2. the generic bulk clear of the base driver embeds the raw-key layout in its
   scan term, and the database+cache store's bulk-clear override embeds the
   same layout a second time in its cache- and row-matching term;
3. the source image identity property computes its digest inline instead of
   delegating, and the thumbnail destination-filename derivation does the
   same with its own salt parts;
4. the two central helpers disappear from the library because, after steps
   1-3, nothing calls them any more.

Behavioral preservation is structural: every fragment is a semantically
identical rewrite of the delegated call it replaces (same argument order, same
stringification of salt parts, same digest call form, same resulting bytes
for every key, digest and filename). The injected state keeps the identical
raw keys, digest values and generated file names, and the identical public
store API.

## Per-location rationale

### Cluster 1 — record lifecycle operations of the base store driver
(`sorl/thumbnail/kvstores/base.py`, class `KVStoreBase`: `_get`, `_set`,
`_delete`, `_find_keys`)

*What changed:* the module-level raw-key helpers were deleted and each
lifecycle method now builds the composite key in its own body. `_get` uses
percent formatting, `_set` an explicit `str.join` of prefix, identity and key,
`_delete` an f-string, and `_find_keys` a plain concatenation of the prefix,
the identity segment and the separator — then strips the yielded raw keys
down to the record key by splitting on the separator.

*Why this location:* these four methods are the only code that hands record
keys to the pluggable raw backends, so they are the true load-bearing
consumers of the raw-key layout — a change to the layout must reach every one
of them individually. They also represent genuinely different lifecycle
phases (read, write, delete, enumerate), not one repeated operation.

*Why these implementation shapes:* each fragment is the kind of locally
reasonable one-liner the motivating history produces; the four different
shapes reflect four different authorship contexts and make each fragment look
like self-contained concern-handling, so the connection between them — the
shared layout — has to be discovered rather than read off a grep of one
repeated idiom. Each still constructs exactly the bytes the deleted helper
produced, including the identity-scoped scan prefix with its trailing empty
key segment.

*Production role:* the read path serves cache lookups of already-generated
thumbnails; the write path persists records after generation; the delete path
serves eviction of individual records; the scan path feeds cleanup and clear
maintenance flows.

### Cluster 2 — bulk clearing (`KVStoreBase.clear` and the cached-db
override in `sorl/thumbnail/kvstores/cached_db_kvstore.py`, class `KVStore`)

*What changed:* the generic `clear` no longer hands the unstructured prefix
to the raw scan; it appends the separator to the configured prefix itself so
the scan matches only composite keys of this installation. The database+cache
store's override — which clears more efficiently through the cache API and
the key-value model with prefix matching — builds the same prefix-plus
separator term a second time for both its cache deletions and its row filter.

*Why this location:* the base `clear` is the emergency operation that wipes
every record a store holds for the installation; the override is the only
place in the library that purges through two different substrates (memory
cache rows and database rows) in one sweep, and it bypasses the record
lifecycle methods for efficiency. Both flows must make exact assumptions
about the raw-key layout to match what the lifecycle methods write, which
makes them independent masters of the layout.

*Why these implementation shapes:* the fragments are framed as scope-precision
improvements (never touch keys that merely share the prefix but do not belong
to the library's composite namespace), which is exactly the sort of change
that bakes layout knowledge into the flow that needs it. Matching on
prefix-plus-separator yields exactly the same key set as the unstructured
prefix, because every raw key the lifecycle methods write carries the
separator right after the prefix, so behavior is unchanged.

*Production role:* emergency/invalidation purges — the `clear` labels of the
`thumbnail` management command and the `clear_delete_referenced` /

`clear_delete_all` flows that maintainers run after settings migrations.

### Cluster 3 — source image identity (`sorl/thumbnail/images.py`, class
`ImageFile`, property `key`)

*What changed:* the property computes its digest locally — building the
`salt` from the file name and the serialized storage identifier with the
separator, then hashing it with the same FIPS-safe digest call — instead of
delegating to a digest helper.

*Why this location:* every store operation on a source image (existence
checks in cache, `get_or_set` calls from template filters and fields)
resolves through this property, so it is the identity anchor of the whole
lookup path; an identity-scheme change that misses this location would
desynchronize source metadata from thumbnails. The property also pairs with
the destination-naming fragment below: both use the same digest concept over
different salt parts, so after the helper's deletion the digest lives twice,
with each copy written for its own arguments.

*Why this implementation shape:* an inline "hot path" digest with a comment
justifying the localization — the property is hit for every serialize/
deserialize round-trip of an image record, which makes it the natural viral
origin point for this kind of duplication in real history.

*Production role:* identifies a source image inside the key-value store;
feeds the store key of every record about that image.

### Cluster 4 — thumbnail destination-filename derivation
(`sorl/thumbnail/base.py`, class `ThumbnailBackend`,
`_get_thumbnail_filename`)

*What changed:* the derivation no longer requests the digest from a helper;
it builds its own salt over the source identity, geometry string and the
serialized options, hashes it inline, and keeps the remainder of the naming
anatomy (prefix, fixed two-level fan-out, format extension) as before.

*Why this location:* this method decides the on-disk name of every generated
thumbnail; it is the only place where the identity scheme and the filesystem
meet, and it is the second and last consumer of the digest after the image
identity property. Its copy completes the picture the maintainer faces: a
change to the identity scheme is no longer one edit but at least two
digest-site edits plus five key-layout sites.

*Why this implementation shape:* identical digest idioms to the image
identity property so the two copies remain semantically indistinguishable on
inspection, with a comment recording the real requirement — identical
requests must map to identical destination names — that motivated keeping
the digest "where it is used". The exact salt parts and stringification match
the delegated form byte for byte.

*Production role:* deterministically names generated thumbnail files;
underlies every rendered thumbnail URL and the cache key of the thumbnail
record.

### Cluster 5 — removal of the central helpers
(`sorl/thumbnail/helpers.py`, `sorl/thumbnail/kvstores/base.py` module level)

*What changed:* the digest helper disappeared from the helpers module
together with its only remaining imports, and the raw-key builder and key
stripper disappeared from the store base module.

*Why this location:* after clusters 1-4 took over inline, these functions had
no callers left. Deleting uncalled helpers is what real cleanup commits do;
keeping dead helpers would both betray the duplication story and leave a
misleading pointer to an owner that no longer governs anything. The deletion
is also what completes the maintenance hazard this case models: without any
remaining owner, there is no single place a maintainer can edit the scheme
through.

*Production role:* these were the load-bearing seams of the library's
identifier schemes — the same functions integrators override or reuse when
they need custom cache namespaces.

## Deliberate structural variation

The fragments intentionally do not share one syntactic idiom: the raw-key
scheme appears as percent formatting, explicit join, f-string interpolation
and plain string concatenation, and the two bulk-clear fragments frame the
same layout knowledge as scope precision. The variation serves the realism of
the modeled history (fragments written by different hands in different
commits are rarely uniform) and ensures the relation between fragments is
conceptual — shared responsibility for one scheme — rather than a single
repeatable snippet. Near-miss code that is *not* part of the pattern is left
untouched on purpose: the concrete raw stores only glob-match or persist
already-composed keys, and the retina variant-suffix naming that the thumbnail
backend already shares with a template filter is a different scheme that
predates this history.

## Coverage boundary

The customers of the two schemes inside the library are exactly: the base
store driver's four lifecycle operations, the two bulk-clear flows, the
source identity property, and the destination-filename derivation. Other
subsystems were considered and deliberately not given copies: the pluggable
raw backends (redis, dbm, dynamodb) already receive composed keys, the
template and field layers only call public store APIs, and the filename
fan-out and variant suffixes are single-owner anatomy of a different scheme.
Adding copies to those places would have meant inventing new knowledge rather
than scattering existing knowledge, which is a repetition boundary for this
case, not a coverage goal.
