# Injection design record — ggez draw-path helper inlining

## Maintenance motivation

ggez's 2D and 3D canvas draw paths are, in the pinned revision, deliberately
layered: the public draw methods (`draw_mesh`, `draw_mesh_instances`,
`draw_bounded_text`) only orchestrate, private helpers own one responsibility
each (pipeline selection, image/text bind-group shaping, glyph measurement),
and a lowest layer owns the device work (caches, uniform arena, renderer
queues).

The change recorded in this diff models an evolution that is common in
graphics codebases and went wrong in a recognizable way. While chasing a
frame-time regression suspected to come from WGPU draw-call setup, a
maintainer wants to observe *exactly* which render pipelines, bind groups
and uniform allocations each draw path performs, and which lazily-populated
cache keys are hit or missed. The three-layer indirection makes this painful:
every construction argument is two or three `step into` hops away from the
draw call being profiled. The path of least resistance during the
investigation is to collapse the chain — copy each helper's body directly
into the draw method so that every descriptor, cache key and allocation is
visible in one contiguous listing, tweak a few construction parameters in
place, confirm the regression is elsewhere, and move on. The code keeps
working (the copies are faithful, and the caches still cache), the state
ships with the next release, and the extra flag that surfaced it apparently
came from elsewhere in the render graph, so the flattened version simply
stays. Nothing about this evolution changes behavior; it exclusively removes
delegation boundaries.

## Normal code evolution being modeled

This is the classic "debugging-time inlining that never got factored back":
a developer temporarily dissolves helper calls to get a flat, observable
draw path; the flattened state is behaviorally identical to the original, so
no failing test forces a reversal; the helpers that lost all their callers
are then deleted to keep the module warning-free (leaving them dead would
trip the unused-code warnings this codebase keeps clean), and a handful of
tight privacy declarations get relaxed to crate-internal because the draw
methods now perform the helpers' field access directly. Every one of these
follow-on edits is the mechanical consequence of the flattening decision,
which is what makes the resulting state look like ordinary, if wearying,
development history rather than an artificial edit.

## Overall design

Five draw entry points across the two canvas implementations absorb the
complete bodies of the helpers they used to call:

| Draw entry point | Absorbed helper chain |
| --- | --- |
| `InternalCanvas::draw_mesh` | pipeline selection + dummy empty group + pipeline-layout hashing + uniform-arena bump with growth fallback + image bind-group slow path |
| `InternalCanvas::draw_mesh_instances` | the same chain under instance ordering |
| `InternalCanvas::draw_bounded_text` | glyph measurement/bounds + text section assembly and queueing + text-image bind-group shaping + text-uniform group + queue-time text pipeline selection |
| `InternalCanvas3d::draw_mesh` | 3D pipeline construction (depth-tested, back-face culling) + bind-group/layout caches + image bind-group shaping |
| `InternalCanvas3d::draw_mesh_instances` | the 3D chain under instance ordering, plus the inline uniform-arena bump |

The absorbed fragments are faithful copies: cache keys, descriptor contents,
ordering of queue operations, alignment arithmetic and fallback behavior are
preserved, which keeps the rendering output and the full test suite
untouched. The two 3D entry points and the 2D bounded-text path demonstrate
deliberate structural variation (below) rather than three copies of one
identical region.

Deletion and visibility follow-ons:

* `InternalCanvas::update_pipeline` was retained (the text-flush path still
  calls it), while the 3D twin `InternalCanvas3d::update_pipeline` had no
  remaining caller and was deleted;
* `InternalCanvas::set_image` and `InternalCanvas::set_text_image`, and the
  3D `set_image`, lost all callers and were deleted;
* `TextRenderer::queue` in `src/graphics/gpu/text.rs` and
  `Image::fetch_buffer` in `src/graphics/image.rs` likewise became dead and
  were deleted (the image slow path they served now lives inside the 2D draw
  methods);
* the natural consequence of moving construction down into the draw methods
  is that the former helpers could no longer be re-created mechanically
  without also relaxing who may touch what: `BindGroupCache`,
  `PipelineCache`, `SamplerCache`, `GrowingBufferArena`, and the `Text`
  payload struct now expose their fields `pub(crate)`.

## Per-location rationale

### `src/graphics/internal_canvas.rs` — the 2D monoliths (lines 319-1905)

`draw_mesh` and `draw_mesh_instances` are the two hottest 2D draw paths, so
they are where the flattening described above would realistically start —
they share a full helper chain, and it is natural that both absorbed the same
`update_pipeline`/`set_image` bodies during the same session. The two
methods were kept patterned on each other (which is also why a maintainer can
keep this state compiling and warning-free in one pass).

Each contains, in original call order kept intact:

* shader-type dispatch (`ShaderType::Draw` / `ShaderType::Instance { .. }`)
  pasted from `update_pipeline`, with a local `let ty = ...;` so the copied
  pipeline-matching and blend-state logic reads identically to the original;
* bind-group-layout and bind-group creation driven directly against
  `self.bind_group_cache` (the former `BindGroupLayoutBuilder` /
  `BindGroupBuilder` driver calls), including the empty-entries dummy group
  that the old builder produced for a no-resources pipeline bound to
  slot 0;
* the uniform-arena bump: alignment rounding, the cursor scan over existing
  buffers, and the new-buffer fallback that used to be
  `GrowingBufferArena::allocate`/`grow` — inlined as a loop plus a
  fresh-buffer `push` that lands the new cursor exactly where the old
  grow-and-retry left it, expressed once per drawing method that takes its
  uniforms from the arena;
* the image-side setup: sampler cache lookup, the format/usage descriptor,
  and the per-image bind-group slow path previously owned by `set_image`
  (which the deleted `Image::fetch_buffer` also served).

`draw_bounded_text` is the text-mode sibling and absorbs a different chain,
so the text path stays a distinct responsibility rather than a copy of the
mesh path: glyph-layout measurement and bounded-section assembly (previously
one delegation), queue-side section construction, the text-image bind-group
shaping previously owned by `set_text_image`, the text-uniform bind group
against `self.text_uniforms`, and the render-pass driving for the text draw.
Its pipeline selection for the `ShaderType::Text` variant now happens at
queue time (just before the text-flush sentinel turns green) rather than at
flush time; because every canvas-state setter flushes queued text first and
the flush re-runs selection whenever the pipeline is dirty, the observable
sequence of pipelines and draws is unchanged. This is also the file whose
`finish`/`finalize` tail moved, sitting after the now-much-larger drawing
methods.

A maintainer flattening one path during a profiling session realistically
leaves explanatory comments claiming helpful structure, so the inlined
regions carry comments that describe the former layering ("textures here",
"arena bump", "dummy group for a resources-pass") — comments assert
decomposition that the contiguous body no longer has. That mismatch is
deliberate: it reproduces how such states read in the wild.

### `src/graphics/internal_canvas3d.rs` — the 3D twin monoliths (lines 293-1241)

The 3D canvas is behind the `3d` cargo feature and mirrors the 2D paths, so
a developer who flattens the 2D paths in one sitting realistically repeats
the operation on the twin, and the same chain of deletions follows
(`update_pipeline`, `set_image`). Two structural differences are kept real
and observable, because the 3D code is genuinely shaped differently:

* the 3D pipeline construction carries depth-comparison, triangle-list
  topology, 3D vertex layout and back-face culling, and the shader dispatch
  has only `Draw`/`Instance` arms (the 3D canvas has no text path);
* the plain mesh variant continues to take its uniform allocation through the
  pre-existing `uniform_alloc` reference, so only the instanced variant
  performs the arena scan/bump inline — mirroring that the 3D paths do not
  share the 2D arena owner.

Leaving these differences intact preserves the 2D/3D split's actual
semantics, and the duplication between siblings of the absorbed construction
is a direct consequence of the flattening, not an addition.

### `src/graphics/gpu/bind_group.rs`, `gpu/pipeline.rs`, `sampler.rs`, `gpu/growing.rs` — relaxed owners (4 one-line edits)

These are the mechanical fall-out of the flattened state: the draw methods
now perform the former helpers' work against `bind_group_cache`,
`pipeline_cache`, `sampler_cache`, and the uniform-arena fields directly, so
their struct fields open to `pub(crate)`. No behavior or layout changes.

### `src/graphics/gpu/text.rs` and `src/graphics/text.rs` — queue removal, payload relaxation

`gpu/text.rs` lost `queue` to the text draw path and thus exposes only what
it still owns; the `queue` body was absorbed into `draw_bounded_text` so the
text path could construct its sections inline. `src/graphics/text.rs` is the
user-facing `Text` wrapper whose matching payload fields became `pub(crate)`
so the draw path can read them during measurement. Both edits are
consequence-only: nothing else in the module changed.

### `src/graphics/image.rs` — fetch_buffer removal

The slow-path construction of image bind groups used to have two consumers
(the deleted builder pressure plate in this module and the 2D helpers). With
the 2D draw paths inlining the slow path, `fetch_buffer` lost its caller and
was deleted along with its now-unused builder import. The retained
`to_pixels`/`encode` device work was left untouched: it is pre-existing
inline device use in the pinned revision, and no part of this evolution
relates to it.

## Structural variation, deliberately kept

* 2D vs 3D capability shapes: depth/cull vs blend/scissor attributes,
  different vertex layouts, a text arm only in 2D.
* Cache-access idioms vary between the absorbed regions the way the original
  helpers differed: a read-then-write fast path with the write guard closed
  before the match on the image caches (a lock-discipline detail the original
  helper had), versus open-coded `entry(...).or_insert_with_key(...)`
  population elsewhere.
* Three of the five entry points inline the arena bump with its growth
  fallback; the plain 3D mesh path keeps its `GrowingBufferArena::allocate`
  call, and the bounded-text path never bumps the arena at all (its uniforms
  live in the persistent text-side allocation).
* The text entry point performs one stage of its pipeline selection at queue
  time rather than draw time, so the five absorbed regions are not one
  repeated template but five related-but-sitespecific copies — each still
  big, broad, and mutually overlapping in call structure.

These variations are what an organic flattening session leaves behind, and
they are the reason the five expanded methods cannot be factored back by any
single uniform, location-independent rewrite: every site carries its own
descriptor identity, state references, and naming, so the restoration of a
decomposed structure at each site needs boundary- and naming decisions that
depend on the specific method it is performed in.
