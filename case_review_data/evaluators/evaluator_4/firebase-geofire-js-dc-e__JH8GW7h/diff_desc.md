# Injection design record — firebase/geofire-js

## 1. Maintenance motivation

GeoFire's hot paths pass `[latitude, longitude]` values through every layer:
validation, geohash encoding, distance math, bounding-box geometry, the
Firebase document codec, and the realtime query engine. In the upstream
revision, each function receives one array value and indexes into it
(`location[0]`, `location[1]`) or passes it along whole.

The evolution modeled here is a performance-flavored refactor of the sort a
maintainer produces while chasing a profile: the theory being that hot
coordinate code spends time allocating arrays and unpacking them again, and
that handing the two numbers directly to internal helpers — and keeping
frequently-read coordinates (the query center) as raw scalar fields on the
engine — avoids that overhead. The refactor was applied seam by seam over a
release, by the owner of each subsystem: the geohash core, the geometry and
distance helpers, the storage codec, the write path, and the realtime query
engine.

The result is upstream-history-plausible: nothing that a reader would flag as
dead code or sabotage. Each helper looks like a reasonable extraction
(good JSDoc, strict validation), and the public API surface, stored formats
and error texts are all preserved. What accumulates is that one pair of
values, which every reader knows belongs together, is now repeatedly taken
apart and put back together — so every producer must remember how to split
it, every consumer must remember how to rebuild it, and each intermediate
helper exposes the pair as two parameters that can be silently swapped,
orphaned or diverged.

## 2. Overall design

The refactor threads the coordinate pair through the two packages along the
paths a real "scalar fast-path" pass would touch:

1. **Pure algorithms (geofire-common/src/index.ts)** — validation, geohash
   encoding, bounding-box geometry and the Haversine kernel each gained an
   internal scalar-seam helper that the exported function feeds with
   destructured numbers. Because the geohash bit stream interleaves the
   longitude on even bits, the encoding helper's parameter order reflects the
   algorithm rather than the value's native `[latitude, longitude]` order —
   an implementation detail a maintainer would plausibly mirror in the
   signature. The distance kernel carries two locations, so its scalar seam
   takes four parameters, numbered to keep the two pairs apart.
2. **Firebase persistence (packages/geofire/src/databaseUtils.ts)** — the
   document encoder now receives the pair as two scalars beside the derived
   geohash and re-pairs them to validate and to write the stored `'l'` field.
3. **Write path (packages/geofire/src/GeoFire.ts)** — `set()` destructures the
   caller-supplied location to feed the codec's scalar seam.
4. **Realtime query engine (packages/geofire/src/GeoQuery.ts)** — the engine's
   center is kept as two scalar fields instead of one location value; every
   consumer (bounds computation, criteria updates, the center getter,
   per-location distance) re-pairs the two fields into an array to use them.
   Incoming child-event locations are split before being handed to the
   location-update seam, which validates, hashes, stores and fires callbacks
   with re-paired arrays at each boundary; a small helper re-pairs its scalar
   arguments together with the two center fields for the public distance API.

The seams deliberately vary in shape: adjacent pair only, pair with an
independent scalar neighbor (radius, precision, geohash), reversed slot
order, two interleaved pairs, sibling state fields, and write-path
threading through a private helper. Each shape is what that seam's real
usage would naturally produce under this refactor; the variation also makes
the overall pattern harder to reason about from any single site.

## 3. Per-cluster rationale

### 3.1 geofire-common/src/index.ts — validation

`validateLocation` keeps its structural checks (array, length 2) inline and
delegates the numeric checks to a new `validateLocationParts(latitude,
longitude)` module-private helper, which then re-pairs its parameters inside
the very error message it throws (`[latitude, longitude]`), so the exact
public error text is preserved byte for byte.

*Why this site:* validation is the first consumer of every incoming
location, so a "scalar fast-path" author starts here; the extraction shape
(structural vs. numeric split) is the natural first cut, and the forced
re-pairing inside the error path is what the existing message format
dictates.
*Production role:* input validation of the coordinate pair.

### 3.2 geofire-common/src/index.ts — geohash encoding

A module-private `geohashEncodeBits(longitude, latitude, precision)` absorbs
the interleaved bit loop from `geohashForLocation`, which now destructures
the validated location and calls the helper with the two numbers. The
helper's parameter order matches the algorithm's processing order (longitude
occupies the even bits), which reverses the pair relative to the
`[latitude, longitude]` convention that the rest of the file uses; the
derived precision argument sits directly after the pair.

*Why this site:* the encoding kernel is the hottest pure function in the
library and the most tempting one to hand raw numbers; keeping the loop's
parameter order tied to its bit order (rather than to the caller) is the kind
of detail-oriented compromise real refactors make.
*Production role:* hash encoding of the pair with derived precision adjacent.

### 3.3 geofire-common/src/index.ts — bounding-box geometry

`boundingBoxCoordinates` keeps the public signature and validation role and
delegates to a module-private `boundingBoxPoints(latitude, longitude,
radius)` that computes the nine corner/center points from the two numbers,
with the independent radius traveling directly after the pair.

*Why this site:* the geometry routine is the clearest example of a pair
living next to a genuinely independent scalar; a reader must distinguish the
radius (which is not part of any location) from the two values that are.
*Production role:* bounding-box geometry consuming the pair beside an
independent parameter.

### 3.4 geofire-common/src/index.ts — distance

The Haversine computation moves into a module-private
`haversineKilometers(latitude1, longitude1, latitude2, longitude2)` taking
four scalar parameters: the two ends of the measurement, interleaved and
numbered to keep them apart. `distanceBetween` validates both locations,
destructures both, and calls the helper scalar-by-scalar.

*Why this site:* this is the only production seam that genuinely computes
with two locations at once; a scalar-seam refactor must decide what to do
when the clump contains not one pair but two — here resolved by numbering, so
the caller is forced to split and interleave two tuples to make one call.
*Production role:* distance mathematics carrying two locations as interleaved
scalar pieces.

### 3.5 packages/geofire/src/databaseUtils.ts — storage codec

`encodeGeoFireObject(latitude, longitude, geohash)` takes the pair as two of
its three parameters — the third being the geohash derived from the same
location — and re-pairs them twice: once to reuse the public validation and
once to build the stored `'l'` field. Decode (`decodeGeoFireObject`) and the
document types are untouched, so the persisted shape stays the same for every
consumer of the codec.

*Why this site:* the codec is the concrete boundary where a location becomes
another representation (a Firebase document), which makes the re-pairing
overhead explicit while keeping the wire format byte-identical.
*Production role:* storage/persistence encoding of the pair next to the
derived geohash.

### 3.6 packages/geofire/src/GeoFire.ts — write path

In `set()`, the loop destructures each validated location into two locals and
hands them to the codec's scalar seam; the geohash used for the write is
still computed from the grouped value.

*Why this site:* `set()` is the public write funnel; threading the split from
the codec outward exactly one level is what an incremental refactor produces
(the author stops at the first grouped boundary, the validated `location`
local, leaving the destructuring behind).
*Production role:* write-path threading of the pair into the storage codec.

### 3.7 packages/geofire/src/GeoQuery.ts — engine state and realtime paths

- The center is held as two sibling scalar fields (`_centerLat`,
  `_centerLng`). The constructor seeds them from the criteria's grouped
  `center`; the `center()` getter rebuilds an array to satisfy the public
  contract; `geohashQueryBounds` calls re-pair the fields per call; criteria
  updates rebuild the fields element by element with the falsy fallback
  (`||`) applied to a re-paired array first.
- `updateCriteria` recomputes per-key distances by destructuring each
  tracked location and calling the distance seam scalar-by-scalar.
- `_childAddedCallback`/`_childChangedCallback` split each decoded snapshot
  location before handing it to `_updateLocation(key, latitude, longitude)`,
  which validates, hashes, stores and fires callbacks with re-paired arrays
  and compares the new position against the old one scalar-by-scalar.
- `_updateLocation` and `_removeLocation` reach the public distance API
  through `_distanceToCenter(latitude, longitude)`, which re-pairs its
  arguments together with the two center fields into the two arrays that
  `distanceBetween` expects.

*Why this site:* the query engine is where the pair lives the longest as
state — repeatedly reassembled per event, per criteria change and per
callback — so the scalar-field decision concentrates the maintenance cost at
the one owner that can least afford it while remaining the most plausible
"hot state, no allocation" optimization in the whole codebase.
*Production role:* query-center state ownership, realtime child-event
ingestion and tracked-state mutation, criteria updating, and callback
dispatch re-pairing for the public event contract.

## 4. Left intentionally alone

The refactor leaves several adjacent numeric neighborhoods that look
similar but have different subject matter — conversion helpers that take a
genuinely independent distance or resolution next to a single coordinate
scalar (`metersToLongitudeDegrees`, `longitudeBitsForResolution`), geohash
range/string helpers operating on derived hash data (`geohashQuery`,
`boundingBoxBits`), the query configuration object (`QueryCriteria`), the
tracked-location record that already stores its location grouped, the
callback dispatch contract that must keep receiving one grouped location,
tests, examples and type declaration files. These are untouched because the
modeled refactor had no reason to reach them under its own theory, and their
presence preserves the codebase's idiom contrast.
