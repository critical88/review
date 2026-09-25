# Injection design record — glTF cross-reference translation scattering

## Maintenance motivation

cgltf stores every glTF inter-object reference the same way: while JSON is
being parsed, a reference read from the file (a plain object index) is staged
into the target pointer field as `index + 1`, so that the sentinel `0` stays a
valid "no reference" value; after parsing, `cgltf_fixup_pointers` walks the
parsed model and turns each staged value back into a real element of the array
that `cgltf_data` owns, rejecting out-of-range references; when a model is
written back out, every writer recovers the JSON index from the pointer by a
subtraction against the start of the owning array. This representation is a
single small invariant of the whole library. In the pinned revision it is also
the one piece of arithmetic that touches essentially every object family:
meshes, primitives, attributes, skins, joints, nodes, scenes, animations,
accessors, buffer views, textures, images, and the draco/meshopt compression
extensions.

Maintaining this representation means changing it in lockstep. If a maintainer
wants to change the sentinel scheme, support indices wider than a pointer,
tighten or weaken the out-of-range check, make failing references a warning
instead of a parse error, or add debugging to reference resolution, they have to
find every place the bias is applied or removed. The maintenance work is not in
any one location — it is spread across the parse path, the fixup pass and the
serialization path, and a missed copy is a silently wrong reference rather than
a loud failure.

## Modeled development evolution

The pinned revision keeps the translation consolidated behind six
implementation-only definitions: `CGLTF_PTRINDEX` (parse-time staging),
`CGLTF_PTRFIXUP` and `CGLTF_PTRFIXUP_REQ` (post-parse back-translation with
bounds checks), `CGLTF_WRITE_IDXPROP` and `CGLTF_WRITE_IDXARRPROP`
(single-element and per-element-array write-time index emission), with the
three texture-info writer macros layered on top of the write pair.

The evolution modeled here is the ordinary one by which such helper families
erode: each time a new reference-typed property is parsed, the contributor
works inside the local function, and when the helper call does not fit the
statement they are writing — a bounds check that must return early in the
caller's error style, a token scan whose value is needed twice, a loop that
emits one index per element — they inline the expansion "just this once"
because the local copy is obvious and self-contained. Different contributors
spell the local copy differently: some keep the exact expansion, some reorder
the bias operand, some hoist the biased value into a local first, and some
fold the check and the translation into a single line. Over time, new sites
are written by copy-pasting the nearest previous inlined site rather than the
definition. Eventually nothing calls the helper family any more, and since
the library has no other consumer for it, the definitions are deleted — after
which the duplication has no single point of contact left.

The generated diff is the end state of that history, reached by expanding
every remaining call site of the six definitions and removing the definitions
themselves. Each expansion is congruent with the macro expansion it replaces,
so the staged values, the accepted/rejected inputs and the emitted bytes are
unchanged; the change is purely in where the translation is spelled.

## Overall design

Two production files change, matching the library's own layout: the
implementation half of the parser/fixer (`cgltf.h`) and the writer
(`cgltf_write.h`).

* `cgltf.h`: the staging and back-translation definitions are removed and
  their 30 parse-side and 53 fixup-side call sites become hand-written,
  per-site-shaped fragments. Nothing in the public API section is touched;
  the dissolved macros are private to the `CGLTF_IMPLEMENTATION` section.
* `cgltf_write.h`: the two index-emission definitions are removed and their 31
  call sites (28 single-property, 3 per-element array) become hand-written
  fragments inside the writer bodies. The three texture-info emission macros,
  which are themselves shared definitions used by many material fields and
  which invoked the removed definitions from inside their own bodies, are
  updated to spell the arithmetic directly; they remain macros, so each stays
  one shared definition reused by every texture-carrying property.

Per-site drift is deliberate and non-repeating: verbatim copies, operand
reordering (`1 + idx`), staged two-step locals (`cgltf_size staged_ref = …`
followed by the biased cast), pointer-addition forms (`array + offset - 1`
instead of `&array[offset - 1]`), single-line and multi-line statement
granularity, and writer-side locals that name the derived index after the
property being emitted.
The drift reproduces how the same inline operation looks after being written
by different hands at different times, and it means the fragments are not
copies of one template text.

The public `cgltf_*_index` helper family (the accessor, buffer, buffer view,
mesh, node, material, image, texture, sampler, skin, camera, light and scene
index lookups) is deliberately left untouched: it is public API with `cgltf_size`
signatures, it serves applications rather than the parse/fixup/write path, and
mixing it into the same edit would change public behavior rather than
relocate a private responsibility.

## Design by location

### A. Parse-side reference staging — `cgltf.h`, 30 sites in 17 functions

The staging sites are where JSON indices first become pointers. Each was
inlined in the shape that fits its local parsing loop, grouped by the object
families the library itself groups them into.

* **Accessor family** — `cgltf_parse_json_accessor` (1 site:
  `buffer_view`), `cgltf_parse_json_accessor_sparse` (2 sites:
  `indices_buffer_view`, `values_buffer_view`). Sparse accessors are parsed
  two levels deep inside an optional sub-object, so the expanded copies there
  use the most compact single-line form; the accessor's own `buffer_view`
  uses the staged local form, separating the token scan from the cast.
* **Primitive / attribute family** — `cgltf_parse_json_attribute_list`
  (1 site: the attribute's `data` accessor), `cgltf_parse_json_primitive`
  (2 sites: `indices` and `material`), `cgltf_parse_json_draco_mesh_compression`
  (1 site: the compression `buffer_view`), `cgltf_parse_json_meshopt_compression`
  (1 site: the compression `buffer`), `cgltf_parse_json_material_mapping_data`
  (1 site: per-variant `material`, written inside an offset-indexed mapping
  table). These sit inside tight per-JSON-object loops with local counters,
  the classic place a contributor inlines rather than threading a helper
  through `variant`-flavored object plumbing.
* **Texture / image family** — `cgltf_parse_json_texture_view` (1 site:
  `texture`), `cgltf_parse_json_texture` (4 sites: `sampler`, `image`,
  `basisu_image`, `webp_image`), `cgltf_parse_json_image` (1 site:
  `buffer_view`). All four texture sites inline distinct spellings, one per
  extension aisle (`KHR_texture_basisu`, `EXT_texture_webp`) — in real code
  these aisles are added one at a time, each copying the nearest prior site.
* **Skin / node / scene family** — `cgltf_parse_json_skin` (3 sites:
  per-joint node, `skeleton`, `inverse_bind_matrices`),
  `cgltf_parse_json_node` (5 sites: per-child node, `mesh`, `skin`, `camera`,
  `light`), `cgltf_parse_json_scene` (1 site: per-root node),
  `cgltf_parse_json_root` (1 site: the default `scene`). The children, joints
  and scene-nodes sites stage one reference per array element inside
  `for` loops; the node sites are spread across an `else if` chain matching
  the primitive's JSON property name, so each copy sits at a different
  nesting depth.
* **Animation family** — `cgltf_parse_json_animation_sampler` (2 sites:
  `input`, `output`), `cgltf_parse_json_animation_channel` (2 sites:
  `sampler`, `target_node`). Animation parsers handle inter-object references
  exclusively (a sampler is nothing but two accessor references), which is why
  they justify inlining: there is no non-reference work to share a helper with.
* **Buffer view** — `cgltf_parse_json_buffer_view` (1 site: `buffer`), the
  entry that every other reference eventually points through.

Role served: these functions are the only place indices enter the in-memory
model, so they are the parse-life half of the representation's lifecycle.

### B. Post-parse back-translation — `cgltf_fixup_pointers`, 53 fragments

`cgltf_fixup_pointers` is the single pass that converts staged values into
owned-array elements after JSON decoding; in the pinned revision every
conversion goes through the two `CGLTF_PTRFIXUP` variants and reads as one
uniform block. In the end state each of the 53 conversions carries its own
inlined bounds check and subtraction, drifting between a compact one-line
form, a pointer-addition form, and a multi-line form with the check on its own
line. The plain/required split is preserved everywhere: optional references
are guarded by `if (ref)` and rejected when above the array count, required
references use a single `if (!ref || (cgltf_size)ref > count)` rejection.

Grouped by the arrays they walk, mirroring the function's own block structure:

* mesh primitives: `indices`, `material`, per-attribute `data`, morph-target
  attributes (checked per element), draco `buffer_view` and its attributes,
  variant `mappings` (7 fragments);
* accessors: `buffer_view`, sparse `indices_buffer_view` and
  `values_buffer_view` (3, all required-style guards);
* textures and images: `image`, `basisu_image`, `webp_image` and `sampler` on
  textures; `buffer_view` on images (5);
* materials: 21 texture-view references spanning the metallic-roughness,
  specular-glossiness, clearcoat, specular, transmission, volume, sheen,
  iridescence, diffuse-transmission and anisotropy sub-objects — the densest
  single block, because material extensions are added incrementally and each
  one carries exactly one new texture reference to fix up;
* buffers and views: `buffer` on buffer views and
  `meshopt_compression.buffer` (2, required-style);
* skins: per-joint node arrays, `skeleton`, `inverse_bind_matrices` (3);
* nodes: per-element `children`, `mesh`, `skin`, `camera`, `light`,
  and the `mesh_gpu_instancing` attribute block (6);
* scenes and the default scene pointer (2);
* animations: per-sampler `input`/`output`, per-channel `sampler` and
  `target_node` (4).

Role served: this pass is the representational heart of the decode life; each
fragment also locally couples a validation decision (which bound is checked,
which references are optional) to the arithmetic, which is exactly why the
copy at each site has to be hand-shaped rather than textually shared.

### C. Write-time index emission — `cgltf_write.h`, 31 sites in 11 functions

The writers mirror the parse path property for property. The 28
single-property sites collapse each to `if (ref) { indent;
CGLTF_SPRINTF("\"%s\": %d", label, (int)(ref - start)); needs_comma = 1; }`
in one of three shapes: all on one line matching the file's dense single-line
style, spread over a brace block, or with the subtraction named in a
site-local variable declared inside the guard (`if (ref) { int light_index =
(int)(ref - start); … }`, with each site naming its temporary after the
property it translates) — the form that reads naturally when the emitted
index is also the value a contributor wants to glance at in a debugger.

* `cgltf_write_primitive` (7): primitive `indices`/`material`, per-attribute
  `data` in two separate loop bodies (attributes and morph targets), draco
  `bufferView`, and variant mapping materials.
* `cgltf_write_node` (6): per-element `children` (array shape), `mesh`,
  `skin`, `light`, per-attribute `data`, `camera`.
* `cgltf_write_texture` (4): `source`, `sampler`, and the `basisu`/`webp`
  alternate sources.
* `cgltf_write_accessor` (3): `bufferView` plus both sparse views, the third
  hoisted into a local.
* `cgltf_write_skin` (3): `skeleton`, `inverseBindMatrices` (local form), and
  per-element `joints` (array shape).
* `cgltf_write_animation_sampler` (2) and `cgltf_write_animation_channel` (2):
  sampler `input`/`output` and channel `sampler`/`target_node`.
* `cgltf_write_buffer_view` (1: `buffer`), `cgltf_write_image` (1:
  `bufferView`, local form), `cgltf_write_scene` (1: per-element `nodes`
  array shape), `cgltf_write` (1: the document-level `scene` pointer).

The three array-shaped sites (`children`, `joints`, scene `nodes`) each emit a
JSON array of indices with a bounded loop and per-element separator handling;
the others emit a single index property. Both array shapes and both label
styles (`"bufferView"` at two different nesting depths, attribute names taken
from parsed attribute strings) appear across the sites.

Role served: these functions are the serialize life of the representation;
each fragment re-derives the index from the pointer, and each embeds its own
label and own array pair, so the emitting code path depends on the pointer
layout knowledge locally instead of obtaining it from one place.

### D. Texture-info emission macros — `cgltf_write.h`, 3 bodies

`CGLTF_WRITE_TEXTURE_INFO`, `CGLTF_WRITE_TEXTURE_INFO_NORMAL` and
`CGLTF_WRITE_TEXTURE_INFO_OCCLUSION` are shared emission bodies reused by
every texture-carrying material property; they used the removed
index-emission definition from inside their own macro bodies. Because those
bodies had to change with the removal, they now spell the
property-plus-index emission directly. Each remains one shared definition
serving all its call sites — these are kept as macros deliberately, since
consolidated shared definitions at this file's granularity take exactly this
form. This also leaves the file with two visibly different conventions for
the same emission sitting next to each other, which is the natural residue of
the same history: shared bodies for the material extensions that were all
added together, hand-inlined copies everywhere else.

### E. Removed definitions

`CGLTF_PTRINDEX`, `CGLTF_PTRFIXUP`, `CGLTF_PTRFIXUP_REQ` (in `cgltf.h`) and
`CGLTF_WRITE_IDXPROP`, `CGLTF_WRITE_IDXARRPROP` (in `cgltf_write.h`) are
deleted once their last call sites are inlined. All five live in the
implementation-only section and never appear in the public interface, so the
removal is invisible to downstream compilation units. The remaining
implementation macros (`CGLTF_ERROR_JSON`, `CGLTF_SPRINTF`, the
texture-info trio) are untouched.
