# diff_desc.md — what `smell.diff` adds, and why

## Realistic maintenance motivation

sqlite-vec is a small, fast-moving extension: one developer, one production
translation unit (`sqlite-vec.c`) that textually includes four component
files, and a release cadence that went 0.0.x → 0.1.0 → a stream of 0.1.x
alphas within a single year. That kind of history leaves residue. Features
get rebuilt on new designs in the weeks before a release, prototypes are
tried out beside the code that eventually wins, and the losing copy is
"kept just in case" by a developer moving to the next thing. The motivation
modeled here is exactly that interval: the carry-over from the pre-0.1
rework, plus experiment leftovers from tuning the current behavior, still
present in a 0.1.10-alpha tree because nothing forced the cleanup.

## Normal development evolution being modeled

Before 0.1.0 the extension had earlier implementations of several
behaviors (quantization, JSON rendering, chunk storage) that were replaced
during the rework; the IVF support and DiskANN graph index grew out of
prototypes that had their own maintenance commands and strategy knobs; the
k-means clustering used a plain random seeding before moving to k-means++.
The diff adds back code that plausibly *survives* that evolution: retired
implementations left in the tree, an internal selectability switch that was
meant to become a table option but never did, a recursive rendering
strategy from a preview experiment, an incremental-retention maintenance
walk that lost to a rebuild-on-invalidation design, a random seeding
routine kept during clustering experiments, a chunk-blob encoding abandoned
when it broke chunk updates, and a rebuild-strategy table left behind when
the standalone tooling it served was cut down into the loadable extension.

## Overall design

All additions use only mechanisms the file already uses (`sqlite3_mprintf`
shadow-table SQL, `sqlite3_str` building, file-scope `static` definitions,
`static const` tables, `#define` constants), match the surrounding
formatting and comment voice, and touch every one of the five production C
sources. Deliberate structural variation across the additions:

* one cluster anchors on a **live function** through a guard branch whose
  controlling switch is a file-scope integer initialized to 0 — the branch
  reads as a runtime decision but is effectively a constant;
* two clusters are **mutually recursive cycles** closed over forward
  declarations, so their members reference each other and nothing else;
* one cluster is a **rooted chain**: a top-level draft entry calling two
  private helpers;
* one cluster is a single **superseded routine** sitting beside the routine
  that replaced it;
* one cluster is a **file-local dispatch table** (struct type plus
  function-pointer hooks plus handlers), the way C code usually models
  pluggable strategies;
* supporting paraphernalia is varied too: never-set switch and its error
  spec string, a policy array with an inline-commented meaning, a depth
  macro.

## Cluster A — legacy quantization mode (`sqlite-vec.c`)

**What:** a file-scope selectability switch and spec string, a clamp-step
helper, and a complete alternative quantization implementation, plus a
guard branch inside the live `vec_quantize_int8` entry that routes to the
alternative when the switch is set.

**Why this site:** `vec_quantize_int8` is the live SQL-facing quantizer,
the natural place a "which quantizer are we running" decision would have
grown — and the natural place for the old switch to survive half-wired.

**Why this shape:** quantization was at the center of the pre-0.1 rework,
and the clamp-step variant is a genuinely different mapping (−1..1 folded
to a ±63 clamp then divided by step) rather than filler: it computes real
branches with real values. The never-set switch makes the selection look
data-dependent while being constant in every shipped build.

**Production role:** the retired earlier scalar quantization mode, plus
its 0.0.x-era "spec" identifier and the switch that was meant to let
tables opt into it before the option parser grew in a different direction.

## Cluster B — recursive element serializer (`sqlite-vec.c`)

**What:** three file-scope functions (append-null, emit-one-element,
recurse-to-next-position) declared and defined across each other so the
element emitter walks back into the recursion entry for the next
position, preceded by a forward declaration.

**Why this site:** immediately before the live single-pass JSON builder, in
the block occupied by serialization strategy code.

**Why this shape:** recursion-per-element is how the JSON preview
experiment was originally sketched; it also produces the mutually
referencing cycle shape, where the trio reads as an active subsystem even
though no live path enters it.

**Production role:** the abandoned recursive rendering strategy
from the JSON-preview experiments, superseded by the single-pass
builder actually used for `vec_to_json`.

## Cluster C — incremental retraining sweep (`sqlite-vec-ivf.c`)

**What:** a depth macro, a `static const int` policy array (rows-per-
centroid ceiling, nested-sweep allowance, minimum drifted rows), and three
functions: a per-centroid drift score over the IVF cells shadow table, a
single-centroid reseed step, and a sweep that walks every owning centroid —
mutually recursive through forward declarations.

**Why this site:** inside the experimental IVF maintenance code, right
after the clear/rebuild routine that replaced this work; it consumes the
same shadow tables (`VEC0_SHADOW_IVF_CELLS_NAME`) through the same
`sqlite3_mprintf` patterns as live IVF code.

**Why this shape:** an incremental-retrain path is a credible first
implementation of centroid maintenance (resseed drifted centroids inside a
write transaction), and the sweep/chunk/score split with a depth ceiling is
how that code would really be shaped. The rebuild-on-invalidations design
that shipped makes the whole walk obsolete.

**Production role:** the first-build IVF maintenance strategy: keep
centroids incrementally retrained during inserts, before the extension
moved to clearing and rebuilding on invalidations.

## Cluster D — random seeding routine (`sqlite-vec-ivf-kmeans.c`)

**What:** one file-scope function seeding `k` centroids by uniform random
picks from the input vectors through the file's existing xorshift PRNG,
with its comment explaining it as the original initialization kept around
during clustering experiments.

**Why this site:** the clustering helper sits directly beside the k-means++
initializer that superseded it — where a tuning-phase routine would really
have lived and been left.

**Why this shape:** a single routine with an obvious purpose, deliberately
the simplest shape in the batch; it reuses the file's own PRNG and memcpy
style so it reads as a sibling of the live initializer rather than pasted
content.

**Production role:** the original random-split seeding used while the
k-means clustering was being tuned before switching to k-means++ seeding.

## Cluster E — draft chunk pack format (`sqlite-vec-rescore.c`)

**What:** a top-level chunk-blob builder plus two private helpers (a varint
delta fold for the rowid, an int8 scale-and-clamp element encoder) with a
forward declaration, under a section banner describing the first attempt at
encoding rescore chunks.

**Why this site:** in the rescore storage-encoding block, before the insert
path that constructs live chunk blobs.

**Why this shape:** the helpers mirror the file's real encoder conventions
(scale/clamp each float into an int8) and the top-level entry carries the
same signature family as the live chunk writers, so the draft reads as an
abandoned alternative encoding — one that lost because packing rowids into
the stream broke the in-place chunk-update layout the live zeroblob path
needs.

**Production role:** the initial rescore chunk encoding draft (payload plus
delta-coded rowids), retained from the file-format experiments before the
zeroblob layout was settled.

## Cluster F — rebuild strategy hooks (`sqlite-vec-diskann.c`)

**What:** a file-local struct type describing a named rebuild operation
(`zName` plus a function pointer), three handlers — a full sweep over
node neighbor lists, a chunk-partition edge rebuild using a modulo query,
and a shared cost estimate over neighbor-list lengths — and a
`static const` table registering the two strategies by name.

**Why this site:** in the DiskANN maintenance code, ahead of command
dispatch, where a rebuild verb would have found its implementations; the
handlers query the same shadow tables with the same access patterns as
live DiskANN maintenance.

**Why this shape:** a name-plus-function-pointer table is the standard C
shape for swappable strategies ("sweep" vs "chunk_edges"), so the block
reads like a ready-made extension point awaiting the `rebuild=` verb that
was never added to the command parser.

**Production role:** the whole-graph rebuild strategies from the standalone
DiskANN prototype, kept while only the incremental reverse-edge repair was
carried into the loadable extension.

## Why the diff is shaped this way

The six clusters were chosen so that each production component file of the
translation unit is involved, each cluster carries a distinct retired
responsibility, and the structural shapes differ enough that one textual
pattern cannot describe them all: a guarded legacy mode anchored in a live
entry point, two closed recursion cycles from different components, a
rooted helper chain, a simple superseded routine, and an unregistered
strategy table with its struct and handlers.
