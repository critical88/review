# Injection design record — decode capability surface on the shared `Decoder` trait

All locations are in the imageflow workspace at commit
`f49ec638d60d06911c7232309bba85578a0812ea`. Paths are repository-relative.

## Maintenance motivation

`imageflow_core` decodes every input through the `Decoder` abstraction in
`imageflow_core/src/codecs/mod.rs`: a codec hands its decoder out as a
`&mut dyn Decoder`, and pipeline code drives the whole decode lifecycle
(`initialize` → image info → `read_frame`/`has_more_frames` → `tell_decoder`)
without knowing the concrete backend. Seven production decoders implement the
trait today: the C-side `JpegDecoder`, `LibPngDecoder`, `MozJpegDecoder`,
`ImagePngDecoder`, and `WebPDecoder`, the `GifDecoder`, and the zencodec
bridge `ZenDecoder` (built under the `bmp`/`zen-codecs` feature family).

Three recurring needs all run into the same wall: the pipeline holds a
generic `dyn Decoder`, but the interesting capability is usually implemented
by exactly one or two concrete decoders.

1. **Animation frame pacing.** The animated-WebP output path in
   `imageflow_core/src/codecs/webp.rs` must know how long the frame it just
   consumed should stay on screen. Today `input_frame_info` sniffs the
   concrete type by downcasting through `as_any()`: it reads the GIF frame
   delay directly, and consults `ZenDecoder` behind a `#[cfg(feature =
   "zen-codecs")]` block. Any other animation-capable input bypasses pacing
   silently, and the function carries decoder-family knowledge that the
   codecs layer already owns.
2. **Playback loop policy.** `input_loop_count` in the same module wants the
   repeat count for the animation header. Today it downcasts to
   `GifDecoder` and reads `gif::Repeat` by hand — the policy lives in the
   consumer, implemented for exactly one backend, with the same
   feature-flagged blind spots.
3. **Decode working-set estimates.** The `/v1/estimate` JSON endpoint
   (`imageflow_core/src/json/endpoints/v1.rs`, `EstimateEncode001`) wants the
   decode-side memory estimate to fold into its response. Today the endpoint
   hand-rolls a `ZenDecoder` downcast and drives the zencodec estimation API
   directly; every other decoder contributes zero, and the endpoint — the
   outermost JSON layer — reaches through two internal layers into a
   concrete backend.

## Modeled evolution

The modeled work is the unglamorous version of feature accretion: each of
the three needs is real, each is taken to "the interface should expose it",
and each lands as one more required method on the shared trait that every
decoder must now carry. The pattern follows the precedent the trait already
has: `get_exif_rotation_flag(&mut self, c: &Context) -> Result<Option<i32>>`
is an existing capability probe whose value is only meaningful for a couple
of JPEG-family decoders, and whose other implementors all answer a fixed
"nothing here" (`Ok(None)`). New code that reaches for probe-shaped members
naturally copies the house style rather than inventing new plumbing, so the
three capabilities declared below all use the `Result<Option<..>>`
report-or-disclaim shape, become *required* trait members (matching the
trait's existing all-required convention), and get filled in by every
implementor — genuinely where metadata exists, and with inert
`Ok(None)` bodies plus a one-line reason where it does not.

## Overall design

Three capability members were added to `pub trait Decoder`:

- `last_frame_delay(&mut self) -> Result<Option<u16>>` — delay of the most
  recently produced frame, in centiseconds, for animation pacing;
- `get_loop_count(&mut self) -> Result<Option<u32>>` — additional playback
  loops requested by the input (0 = loop forever);
- `estimate_decode_resources(&mut self, w: u32, h: u32) ->
  Result<Option<(u64, u64)>>` — peak decode working-set estimate in bytes as
  `(typical, conservative)`, mirroring the zencodec
  `peak_memory_bytes_est`/`peak_memory_bytes_max` pair the estimate endpoint
  already reports.

The two animation consumers in `webp.rs` and the estimate endpoint in
`v1.rs` were rewired from concrete-type downcasts onto the new generic
probes, and each of the seven implementors filled the members in. The
endpoint edit also required regenerating
`imageflow_core/src/json/endpoints/openapi_schema_v1.json.hash`, the
schema-source fingerprint that the endpoint sources are checked against,
because `v1.rs` participates in that hash.

## Per-cluster design decisions

### The trait surface — `imageflow_core/src/codecs/mod.rs`

The three members were appended to the trait right after `as_any`, the last
of the pre-existing members, so the trait remains one flat block of
required methods. Members are required rather than defaulted: every current
method on `Decoder` is required without a body, and the decoders which
*cannot* support a capability are exactly the ones that must answer
`Ok(None)` explicitly, with a reason, rather than inheriting a silent
default — matching how `get_exif_rotation_flag` is handled today. Each
member carries a doc comment stating the unit and the disclaimer contract
(`None` = this format/backend has no such metadata), because unit
conventions (centiseconds vs milliseconds) are precisely what sinks
animation glue when undocumented.

### The GIF decoder — `imageflow_core/src/codecs/gif/mod.rs`

`GifDecoder` is the one C-side decoder with real frame metadata, so its
fill-ins do real work: `last_frame_delay` forwards
`self.current_frame().map(|frame| frame.delay)` (the `gif` crate already
stores delays as centisecond `u16`s, which matches the trait's unit), and
`get_loop_count` maps the existing `get_repeat()` — `Some(Repeat::Finite(n))`
to `Ok(Some(u32::from(n)))`, anything else to `Ok(Some(0))` — reproducing
the value mapping the old WebP-side consumer performed, now at the owner.
Its `estimate_decode_resources` is the inert body ("The gif backend does
not expose a memory model"), because the gif crate genuinely publishes no
memory model.

### The zencodec bridge — `imageflow_core/src/codecs/zen_decoder.rs`

`ZenDecoder` is the modern-codecs bridge, and the estimates capability is
the reason it participates: `estimate_decode_resources` builds a
`zc::estimate::ComputeEnvironment`, carries the decode thread budget,
calls the existing inherent `ZenDecoder::estimate_decode`, and packs the
est/max pair into the trait's tuple. The `Context` is not available at call
time, so the estimate needs the thread budget another way: the struct
caches `ExecutionSecurity.max_threads` at construction in a new field, and
fills it in `new_zencodec` from `c.security.max_threads`. This keeps the
produced numbers identical to what the endpoint computed on its own, which
is what makes rewiring the endpoint safe. `last_frame_delay` simply
forwards to the inherent accessor of the same name (`Self::last_frame_delay`),
which already returns centiseconds. `get_loop_count` stays inert with an
explicit comment: `ZenDecoder` genuinely has multiple animation
capabilities, but this generic probe keeps reporting "unknown" because the
loop-policy value it exposes over the API surface is already consumed via
the encoder-side animation glue that talks to the concrete decoder —
reporting through the probe here would change which input the animated
output consults.

### The still-image and libwebp decoders — `jpeg_decoder.rs`, `libpng_decoder.rs`, `mozjpeg_decoder.rs`, `image_png_decoder.rs`, `webp.rs`

Two of these files pair naturally: `JpegDecoder` and `LibPngDecoder` are the
libjpeg/libpng-backed decoders, `MozJpegDecoder` is the mozjpeg variant of
the JPEG path, and `ImagePngDecoder` is the pure-Rust png backend. None of
them can produce a second frame or a loop policy, so those two members are
inert bodies with the shared reason line "Still-image decoder; no frame
pacing metadata exists" and "Single-image format; looping does not apply" —
the same two sentences everywhere because the underlying fact (a single
image has no timing) is identical, and inventing five different phrasings
would be worse prose, not better design. The memory-estimate reason is
where the files genuinely differ: each names its own backend ("The jpeg
backend does not expose a memory model", "The libpng backend …", "The
mozjpeg backend …", "The png backend …"), because that claim is a claim
about a specific C library and a reader checking it goes to that library.

`WebPDecoder` (C libwebp) is the one non-single-image case: WebP *does*
have an animation container, but this decoder's frame iteration has not
been wired to the animated-WebP API yet, so its two animation members say
exactly that ("Not wired to the animated WebP API yet (see
has_more_frames)") instead of claiming the format is single-image. Its
estimate member uses the "backend does not expose a memory model" reason.

These fill-ins are the interface-completion bookkeeping of the change:
they exist so the trait stays implemented, and their reason comments are
the only substance they carry. The production role of that bookkeeping is
real — a future maintainer touching the trait sees where each backend
stands — but they also deliberately expose the cost of the approach: five
production decoders now carry six-line tombstones apiece, none of which
does anything at runtime.

### The generic consumers — `imageflow_core/src/codecs/webp.rs`, `imageflow_core/src/json/endpoints/v1.rs`

`input_frame_info` keeps its structure — first configured decoder wins,
default 100 ms — but the per-format arms collapse into one generic probe:
`decoder.last_frame_delay()` feeds the same `delay_ms = centiseconds * 10`
computation, so GIF and zen inputs keep their exact previous values while
any future animation-capable input works without new downcasts.
`input_loop_count` likewise takes the first decoder reporting
`get_loop_count()` (dropping the GIF-only downcast and its `gif::Repeat`
handling, which moved into `GifDecoder`). Both functions' doc comments were
updated to describe capability reporting instead of GIF specifics.

In `v1.rs`, the `estimate` endpoint's `#[cfg(feature = "bmp")]` block drops
its hardcoded `ZenDecoder` downcast and the locally re-created
`ComputeEnvironment` in favor of `decoder.estimate_decode_resources(w, h)`,
falling back to `(0, 0)` exactly as before when the probe reports no
capability. The endpoint keeps returning the same decode-only
`EncodeEstimate` on this path. Because `v1.rs` participates in the
OpenAPI-source fingerprint, the regenerated hash is part of the same edit.

## Behavior notes

The two rewired consumers produce byte-identical results on all in-tree
inputs: GIF frame delays and repeat counts come from the same accessors,
zen delays forward through the same inherent method, the zen estimate
pair is computed from the same thread budget and the same estimation API,
and every previously-unhandled decoder still contributes the same defaults
(100 ms, loop count 0, no memory estimate). The zen loop-policy probe
deliberately reports `Ok(None)` so the animated-output path consults the
same inputs it did before. No test, telemetry, or downstream artifact was
touched by this work.
