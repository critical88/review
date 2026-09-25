# Injection design record — per-quality playback data handling in griffith

## Maintenance motivation

griffith v1.33 opened the playback pipeline to third parties twice over: the
`customPlayer` plugin slot accepts an arbitrary MP4 MSE plugin, and the
built-in playback components (native, MP4/MSE, HLS) receive their props from
the player core. Around the same time, work on rendering performance convincingly
argued that React's shallow prop comparison behaves best when props are stable
primitives rather than freshly allocated object arrays, because every
re-render of the player core produces new array identities and thereby new
downstream renders.

A maintainer acting on both observations at once reaches for a local
"flattening" of the playback data: stop handing plugin components a list of
per-quality records (`PlaySource` — allocated once per source-map change and
re-allocated in every memo), and hand them the individual columns directly —
one string array for the addresses, one string array for the qualities, one
number array each for width, height and bitrate. Primitive columns can be
built once and reused, and the plugin contract stops mentioning an internal
record type that third-party plugin authors would depend on unnecessarily.
The columns stay "obviously" aligned because they are projected from the same
list in the same helper module, in order.

## Evolution being modeled

This is a normal, plausible refactoring step inside one feature area rather
than a cross-cutting rewrite:

* v1.33 shaped the plugin boundary (`customPlayer`, `customHeaders`).
* Immediately after, the internal data crossing that boundary gets optimized
  for prop stability: the record list is decomposed into member arrays at
  parse time and the column arrays flow through context, the player core and
  every built-in plugin component.
* The utility that synthesizes an adaptive master playlist for HLS follows
  suit: instead of accepting the record list it takes the columns as a flat
  defaulted parameter list and re-zips them by index internally.

The step models a classic way implicit conventions accumulate: a coherent set of
values is decomposed for a plausible local reason, and the grouping knowledge
("these five arrays describe the same things, in the same order") survives
only as an unwritten agreement shared across three packages.

## Overall design

One descriptor is flattened at every site of the pipeline that owns
per-quality playback data, and nothing else is touched:

* **The plump becomes columns.** `parsePlaylist.ts` keeps `getQualities`
  unchanged, drops the record-building helper, and gains one projection helper
  per member: addresses, widths, heights, bitrates. Each helper maps the same
  ordered quality list over the source map, so the columns share their axis.
* **Alignment axis.** The derived column arrays are aligned to the quality
  list *before* the synthetic `auto` entry is prepended for MSE/HLS auto
  quality; the context documents that its quality-axis column deliberately
  excludes `auto`, unlike the published `qualities` list. Consumers therefore
  address columns by index against the stable pre-`auto` axis.
* **Distribution through context.** The context publishes
  `playUrls`/`sourceQualities`/`sourceWidths`/`sourceHeights`/`sourceBitrates`
  as five peers; the current address is derived as
  `playUrls[indexOf(currentQuality)]` with an explicit first-column fallback,
  replacing the previous record lookup ("find by quality, else first entry").
* **Separate handoff per plugin.** The shared prop type of the video core, the
  native plugin, the MP4/MSE plugin and the HLS plugin each declare the five
  columns independently; the handoff in the video core's render passes them
  as unrelated values, and the context-injecting wrapper peels them straight
  from the context value.
* **Flat utility contract.** The HLS master-playlist builder takes the
  columns as four defaulted parameters (`playUrls`, then bitrates, widths,
  heights) and zips them back into playlist entries by index. The per-quality
  record type that only served it is deleted, since no other module in
  `griffith-hls` needs it anymore.
* **Change detection.** The player shell's playlist-change reset effect needs
  an identity signal that changes when the playlist changes; with the record
  list gone it watches the quality-axis column instead.
* **Shape and defaults.** Because the columns are either present or absent
  together, the utility's later parameters default to empty arrays, which
  keeps the call sites short; the synthetic-`auto` detection in the HLS
  component compares against the quality column with an explicit
  index-found test rather than a record search.

## Per-cluster rationale

### Playlist parsing helpers (`packages/griffith/src/contexts/parsePlaylist.ts`)

The parse site is where the flattening must originate: the source map is the
only place the members exist together. One projection helper per member was
chosen (rather than one helper returning a tuple) because each column then
has a single named producer with a self-describing type, which reads as a
local optimization and keeps the quality axis as an explicit argument. The
module keeps the existing quality-ordering helper untouched so the columns
inherit exactly the previous ordering semantics. `getQualities` and its
mobile/desktop trimming rules are untouched.

### Context distribution (`packages/griffith/src/contexts/VideoSourceContext.ts`, `VideoSourceProvider.tsx`)

The context is the pipeline's spine, so both its published type and its
producing provider participate. The provider snapshots the quality axis
before the synthetic `auto` entry is unshifted, which preserves the previous
"the published record list never contained `auto`" property in column form,
and derives all columns in the existing memo so they re-derive exactly when
the previous record list did. Deriving the current address by index against
the quality column (with a first-column fallback) mirrors the previous
find-by-quality-or-first-entry semantics, including for `auto`; the columns
become memo dependencies just as the list was.

### Player shell change-detection axis (`packages/griffith/src/components/Player.tsx`)

Only the playlist-change reset effect participates: it consumed the record
list purely as an identity signal. The quality-axis column is recomputed by
the same memo on the same inputs, so it carries the same identity semantics.
No other behavior of the player shell is involved.

### Video core and context wrapper (`packages/griffith/src/components/Video.tsx`)

The internal prop type declares the five columns as peers of `format`,
`currentQuality` etc.; the render destructures them and hands them to the
message-wrapped video component as unrelated values; the context-injecting
wrapper — which exists to make the core context-free and forward-ref
compatible — peels the same five columns off the context. This site exists in
two layers (wrapper and core) because the wrapper is the producer of props
and the core is their consumer; both must speak the flattened dialect for the
layer boundary to type-check. The injected-props union (`Omit` keys) gains the
new names for the same reason.

### Native plugin (`packages/griffith/src/components/NormalVideo.tsx`)

The lowest-effort consumer: it needs none of the content, only a stable prop
surface, so the record prop becomes the five columns and the columns land in
the strip list alongside `paused`/`currentQuality` so nothing non-DOM leaks
onto the `<video>` element (previously the single record prop was stripped
the same way). `willHandleSrcChange: false` and the rest of the plugin
descriptor are untouched.

### MP4/MSE plugin (`packages/griffith-mp4/src/VideoComponent.tsx`)

Only the prop type participates: the MSE engine feeds off the selected media
address (`src`), never the list itself, and the previous record prop was
merely forwarded with the other unknown props. The five columns now flow the
same way; the class body needs no logic change, which is exactly why this
site is realistic — for this plugin the flattening is pure contract plumbing.

### HLS plugin (`packages/griffith-hls/src/VideoComponent.tsx`, `utils/getMasterM3U8Blob.ts`, `types.ts`)

This is the only consumer reading content: quality switches resolve the next
address by index against the quality column, and auto-quality detection
asks the same column whether an `auto` entry came from the playlist. The
master-playlist builder is the deepest site: its flat defaulted parameter
list plus internal index zip is the shape the flattening makes most
convenient (callers on one site, projection helpers on the other). The
module-local record type `Source` that existed only for the old shape is
deleted with the flattening. The synthetic-`auto` fallback index (`-1`) and
the destroy/rebuild quality-switch dance (kept behind the same
"no smooth API for switching sources" comment) are untouched.

## What deliberately did not change

* The public playlist map (`PlaySourceMap`) and all exported types of the
  workspace packages: third parties still pass a keyed record map to `Player`.
* Quality ordering/menu behavior, playback-rate handling, event names and
  payloads, buffer/progress plumbing.
* The address derivation semantics: same ordering, same synthetic-`auto`
  fallback, same first-entry behavior on the m3u8 remount path.
* `createMasterM3U8`'s input contract: the builder still feeds it exactly the
  per-entry fields (address, bandwidth, resolution) it always produced.
* All messages, helpers, and icons; nothing outside the described pipeline was
  touched so the flattening stays a coherent, reviewable step.
