# Injection design record: native context as device-capability coordinator

## Maintenance motivation

glow hands applications a raw OpenGL handle. Over the 0.18 cycle the most
common support questions all funneled through the same place: what can this
device actually do? Applications building on glow repeatedly had to
re-derive device facts - parse the `GL_VERSION` string themselves, enumerate
extensions, remember label-length limits before calling object-label APIs,
and size pixel buffers from format/type pairs. Each of those mini-projects
duplicated knowledge that the backend already had at creation time.

The natural inclination was to make the native context the one-stop
door for device facts: it is the only value that (a) exists before any GL
call is made, (b) is handed to every helper the crate exposes, and (c)
already owns the loaded function table. Putting the answers on the context
means every consumer can obtain them from the value it already holds
instead of re-deriving them.

## Normal evolution being modeled

This is the ordinary drift of a service type that is convenient to extend:

1. A caching need appears (label limits) and the field moves onto the
   context so queries stop re-asking the driver.
2. Construction gets smarter: instead of a struct built from an ad-hoc
   `Constants` value plus scattered local variables, the builder itself
   performs device discovery and records the answers as context state.
3. Parsing helpers that used to live with the value type migrate toward
   the place the raw string is born, so the context hosts them and the
   value type becomes a plain data holder on the native path.
4. Callback custody that previously lived in a small trait impl gets
   folded into the context, because the context already owns the teardown
   order.
5. A family of convenience accessors grows next to the free helpers in the
   crate root, so sizing answers read uniformly from the same object as
   everything else.

Each step is small, reviewable, and locally defensible; the accumulated
result is a type that is simultaneously loader-registry, parser,
capability registry, cache, callback owner, and math helper.

## Overall design

The change is centered on the native context type and radiates to the two
modules whose knowledge it absorbs:

- `src/native.rs` - the context type. The previous `Constants` helper
  struct is dissolved into direct context fields, and construction
  populates the new state through a chain of small inherent methods that
  each own one slice of device discovery. The existing debug-output impl
  is kept as the public entry point but now delegates its pointer
  bookkeeping to context methods, and teardown consumes the same
  bookkeeping so install/teardown stay symmetric.
- `src/version.rs` - the version value stays, but on the native path its
  parsing entry point now forwards to the context, making the context the
  single authority for how raw driver strings become version data.
- `src/lib.rs` - a second inherent impl block for the context sits next
  to the crate-level pixel helpers, exposing component counts, byte
  widths, and tight buffer sizes through the context handle.

Public API and observable behavior are unchanged: every free helper,
trait method, and test-facing function keeps its signature and results.

## Per-cluster rationale

### Context state fields and construction-time population (`src/native.rs`)

The `Constants` struct held creation-time answers the context needed
anyway. Dissolving it into context fields (`extensions`, `version`,
`max_label_length`, `debug_callback`) removes an indirection that existed
only to pass five numbers between two structures created in the same
function. The builder (`from_loader_function_cstr`) now runs device
discovery directly and records the answers, each step in its own named
method so construction reads as a sequence of capabilities being filled
in. This form was chosen because creation is the only moment all inputs
(the loaded function table, the raw version string) are guaranteed
present, and later queries then never re-derive what was already learned.

### Version string parsing and capability probing (`src/native.rs`, with the entry point shared into `src/version.rs`)

Raw string parsing needs no GL calls, but it needs the raw string, which
only exists during construction. Hosting the parser as a context method
(`parse_version_string`) with a thin state-refresh wrapper
(`refresh_version_from_string`) keeps the two together; a second small
probe (`supports_extension_indexing_queries`) compares the stored version
against the 3.0 thresholds, so the construction path asks the device what
it supports rather than re-testing elsewhere. The rules themselves are the
same lenient grammar the crate has always used; only their home moved.
The value type's parsing entry keeps its signature so existing callers
and the version's own test module observe no difference - on the native
path it now routes through the context, which is where the maintained
implementation lives.

### Extension bookkeeping: two enumeration strategies behind one door (`src/native.rs`)

Desktop GL wants indexed enumeration (`NUM_EXTENSIONS` walk) once the
device is 3.0-era, embedded devices keep the legacy whitespace-split
answer. Both strategies now live on the context:
`collect_indexed_extension_names` and `collect_legacy_extension_names`,
selected at construction by the probe above, with the resulting set
queryable via a membership helper used by the debug-support check.
Co-locating the strategies next to the stored set keeps enumeration and
lookup together, and the capability probe neighboring the enumeration it
gates.

### Cached device label limit (`src/native.rs`)

Object-label queries need a buffer length before they can ask the driver
for a label. The read is gated on debug support and happens once, at
construction (`refresh_device_label_limit`), stored as
`max_label_length`, and every label query asks the context for the cached
limit (`device_label_limit`) instead of each query re-issuing the
parameter read. This form was chosen because the limit cannot change for
the lifetime of the context, and query paths become unconditional in
shape.

### Debug callback custody and teardown symmetry (`src/native.rs`)

The boxed callback pointer previously lived in the context while its
install/unregister logic lived in the debug-output impl. Custody and the
pointer are now in one place: `install_debug_callback` performs the
install (panicking on double-install as before), `release_debug_callback`
hands the pointer to the teardown path, and `Drop` consumes it so the
unregister call fires exactly when a callback was installed. The public
trait method delegates, keeping call sites unchanged while the context
becomes the single owner of callback state.

### Pixel sizing accessors on the context handle (`src/lib.rs`)

The crate-root free helpers (`components_per_format`, `bytes_per_type`,
`compute_size`) answer sizing questions from format/type constants alone.
The second inherent impl block in the crate root adds context-flavored
accessors (`pixel_component_count`, `pixel_component_bytes`,
`tight_pixel_buffer_size`) directly beside those helpers, expressing the
same math through the context handle so sizing reads uniformly with every
other device answer. The free helpers remain - they are public API used
by applications - so nothing about this family narrows existing callers;
it only adds a second, object-shaped way to reach the same numbers.
