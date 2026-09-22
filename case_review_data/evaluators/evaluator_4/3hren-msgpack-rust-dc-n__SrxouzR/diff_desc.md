# Injection design record — ext metadata made explicit throughout the msgpack-rust ext plumbing

## Repository and maintenance context

The pinned revision (`cf88001`) of 3Hren/msgpack-rust is a four-crate workspace:

- `rmp` — low-level MessagePack primitives (markers, per-family encode/decode modules);
- `rmpv` — the dynamic `Value`/`ValueRef` data model with its own encoders and decoders;
- `rmp-serde` — the serde bridge, including its ext value model (`_ExtStruct`);
- `rmpv-tests` — integration tests over the workspace.

The MessagePack *ext* type family represents an application-specific payload as a header followed by
raw bytes. The header has two halves: the payload length and the application type id. Both halves
are meaningful to writers and readers alike. On the decode side, `rmp` already exposes the header as
one aggregate record (`rmp::decode::ExtMeta { typeid, size }`, returned by `read_ext_meta`); that
side of the workspace is not part of this change.

## Maintenance motivation

Follow-up work around the ext family (new ext-aware value helpers, and the serde ext tuple model)
keeps needing the same three things that the pinned revision does not offer:

1. **A single, reusable ext header writer in `rmp`.** `rmp::encode::write_ext_meta` picks the
   marker, writes the length, and writes the type id inline, and `rmp/src/encode/ext.rs` sits empty —
   although every other encode family (`bin`, `sint`, `str`, `uint`, `dec`, `map`, `vec`) lives in
   its own submodule. The empty file is an unmistakable placeholder: the ext family was meant to
   return to it, the way the other families already have.
2. **Named ext value writers next to each encoder.** In the pinned revision, the ext arms of
   `rmpv`'s owned and borrowed value encoders, and the ext-field serializer in `rmp-serde`, all
   repeat the same two-step dance inline: emit the header through `rmp::encode::write_ext_meta`, then
   flush the payload with `write_all`, each call site with its own local error munging.
3. **A serde ext deserializer that owns the whole header.** `rmp-serde`'s `ExtDeserializer` is
   constructed with only the payload length; it reads the type id later, inside its visitor state
   machine, so the two halves of one header enter the type by different paths and at different
   times.

## Normal evolution being modeled

One maintenance pass over the ext plumbing, in the shape such work actually takes:

1. In `rmp`, restore `encode/ext.rs` as the home of the ext family and split the monolithic
   `write_ext_meta` body into marker selection (`ext_len_marker`) and header emission
   (`write_ext_header`), leaving the public function as the thin entry point it now documents.
2. In the higher-level crates, extract each inline ext write into a small named helper so the
   payload-flushing step and its error mapping exist once, next to the encoder that owns them. The
   helpers spell the ext value out explicitly in their parameters — the payload size as `u32` and the
   application type id as `i8`, the same two scalars `rmp::encode::write_ext_meta` has always taken
   separately — and take the payload bytes alongside.
3. In `rmp-serde`'s decoder finish the header hand-off: read the type id at the marker-match site,
   immediately after the length, pass both values into `ExtDeserializer`'s constructor, and make the
   visitor state machine consume what was constructed.

## Overall design

- `rmp::encode::write_ext_meta` keeps its name, signature, return type, and doc-level contract; its
  body now delegates to the new internal helpers.
- `write_ext_header` mirrors the public primitive's parameter shape (`wr`, `len: u32`, `ty: i8`); the
  marker choice stays factored out as the pure `ext_len_marker` helper, which the public entry point
  still calls to obtain the marker it returns.
- `rmpv` and `rmp-serde` keep calling the public primitive — their helpers take the same explicit
  pair plus the payload, and own the payload flush on top.
- No public item is added, renamed, removed, or re-exported; all new helpers are `pub(super)` or
  private in their crate.
- The emitted byte sequences are structurally identical to before: the marker choice is an
  unchanged match on the length, the length bytes and the type id are written in the same order.

## Per-location rationale

### `rmp/src/encode/ext.rs` — restored ext family module

**What.** `write_ext_header(wr, len, ty)` (emits marker, length and type id) and
`ext_len_marker(len)` (the pure marker choice), both `pub(super)`.

**Why this location and form.** It is the direct counterpart of `rmp/src/decode/ext.rs`, in a module
the layout already reserves for it (the file existed but was empty). Splitting marker choice out
separately keeps the pure decision testable and available to the entry point, which still needs the
marker it returns.

**Role.** The workspace's single implementation of the ext header emission sequence; every ext
writing path in the four crates funnels through it via the public primitive kept in `mod.rs`.

### `rmp/src/encode/mod.rs` — thin public entry point

**What.** The body of `pub fn write_ext_meta(wr, len, ty)` is replaced by a delegation to
`self::ext::write_ext_header`, with the doc paragraph extended to describe the forwarding to the
low-level writer in `ext`.

**Why.** This is the stable public surface every downstream ext writer consumes, including crates
outside this workspace; its shape is the one place the ext pair is definitionally spelled in public
API terms, so the decomposition is done *underneath* it rather than through it.

**Role.** The pinned public primitive: same signature, same marker return, now a thin wrapper over
the ext module.

### `rmpv/src/encode/value.rs` — owned-Value ext writer

**What.** The `Value::Ext` arm delegates to a new private `write_ext_value(wr, ty, len, data)` with
the doc and error contract the module style uses (the interruption-restart note mirrors the
surrounding `write_value` documentation).

**Why this location and form.** The owned and borrowed encoders intentionally remain independent
implementations (the file-per-model split of `write_value`/`write_value_ref` is how this crate is
organized today), so the owned encoder gets its own named helper for its ext arm instead of a shared
one. Naming the helper after the ext *value* (header plus payload) captures both steps that used to
be inline in the match arm.

**Role.** The ext encoding path for owned `Value`s.

### `rmpv/src/encode/value_ref.rs` — borrowed-ValueRef ext writer

**What.** The mirror `write_ext_value_ref(wr, len, ty, data)` with the same delegation from the
`ValueRef::Ext` arm.

**Why this location and form.** Same reasoning as the owned encoder: the borrowed twin is written
after its sibling and borrows the payload instead of taking a `Vec`, so it keeps its own helper with
its own doc. The two helpers spell the same ext value out with their parameters in different orders —
a by-product of writing the borrowed one second, not something either encoder's API constrains.
Neither helper is exported, so no external contract forces them to agree.

**Role.** The ext encoding path for borrowed `ValueRef`s.

### `rmp-serde/src/encode.rs` — ext-field serializer delegate

**What.** `ExtFieldSerializer::serialize_bytes` hands off to `write_ext_field_bytes(wr, len, tag, data)`
(the serde data-model spelling: the "tag" is what `ExtFieldSerializer` received in
`serialize_i8`, and "field bytes" is the payload of the `_ExtStruct` tuple).

**Why this location and form.** The serde ext model is deliberately verbose about what must happen
for an ext value to be legal (type id first, then bytes); the helper parks the two-step write and
the `ValueWriteError` mapping in one named place directly under the two ext serializer impls it
serves. The call site stays a single expression, and the data-model error strings are untouched.

**Role.** The ext-field bytes step of the serde ext serialization path.

### `rmp-serde/src/decode.rs` — deserializer holding the whole header

**What.** `ExtDeserializer` gains a `ty: i8` field; its constructor takes `(len, ty)` as separate
explicit values; the `New` visitor arm now visits the stored tag instead of reading it from the
wire; the two marker-match sites (the marker match in `deserialize_any` and the
`deserialize_newtype_struct` branch) read the type id themselves, immediately after the length, and
pass both into the constructor. `depth_count!` still wraps the hand-off in `deserialize_any`, now
including the type-id read that feeds the constructor.

**Why this location and form.** The type id is part of the same ext header as the length; both are
available the moment the marker arm is taken. Reading it there makes the element sequence
self-describing from construction, keeps the visitor arms to pure state transitions (the remaining
wire read is the payload slice), and stops feeding the two halves of one header into the type by
different paths.

**Role.** The ext value deserialization path of the serde bridge.

## Scope boundary

- Only the ext writing paths and the serde ext deserializer changed. The `rmp` decode-side readers
  (including `read_ext_meta` and the fix-ext family), `rmpv`'s decoders, and `rmpv`'s serde-model
  bridges keep their existing shapes.
- No test files, manifests, feature definitions, public signatures, or public type definitions were
  touched.
- Six production files changed: `+98 / −35` lines across `rmp` (2), `rmpv` (2) and `rmp-serde` (2).
