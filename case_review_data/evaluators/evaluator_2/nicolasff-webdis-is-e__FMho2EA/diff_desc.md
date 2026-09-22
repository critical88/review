# Injection design record — Webdis output-format provider registry

## Maintenance motivation

Webdis supports four output formats: JSON (the default, with JSONP support), raw
RESP, MessagePack (`.msg`), and "custom types" for fixed content types
(`.bin`, `.txt`, `.html`, `.xhtml`, `.xml`, `.png`, `.jpg`, `.jpeg`, `.css`).
Each format lives in its own module under `src/formats/` and exports its own
functions. The two places that dispatch on format — the HTTP extension table in
`cmd_select_format()` and the WebSocket path routing in `ws_execute()` — each
hard-code which functions to call for which format, and the two lists knowledge
are maintained separately.

That split is the realistic pain point being modeled. A maintainer preparing to
add a fifth format, or wanting one uniform place to log which format served a
request, has to touch both dispatch sites, every format header, and keep the
WebSocket list in sync with the HTTP list by hand. The natural refactor to reach
for is the same one many codebases reach for: **one registry record per format**
that holds every capability any format offers, with one table of records in the
HTTP path and one lookup in the WebSocket path. New formats then become a matter
of filling in a record, and the dispatch sites stop mentioning format names.

## Normal evolution being modeled

This is the classic way a "one interface for the whole family" design appears in
a growing codebase:

1. JSON and raw arrive first, and both happen to serve WebSocket sessions
   (`/`, `/.json`, `/.raw`), so their command-extraction, reply and
   error-rendering functions are wired by hand in the WebSocket path.
2. MessagePack and custom types arrive later as HTTP-extension-only formats
   (`.msg`, `.txt`, ...). They never get WebSocket entry points; the HTTP table
   and the WebSocket path drift further apart.
3. Someone unifies the family behind one provider record so registration is
   uniform — without asking which consumer actually needs which capability.
   Every format now has to fill every slot, including capabilities that only
   one or two formats ever had. The obligation becomes part of the design.

The injection produces step 3 directly: the uniform registry exists and all four
formats register through it, with the unused capability slots filled by
placeholders that a conscientious developer writes when forced to provide
something they never exercise.

## Overall design

The injection introduces `src/formats/format.h` with a registry record
(`struct format_provider`) grouping all capabilities any bundled format offers:

- **HTTP output** (`struct format_output`, a nested record, mirroring the
  "output" responsibilities): the reply serializer plus a body-wrapper
  capability (the JSONP wrapping step that previously lived inline in the
  JSON module — a genuinely real capability of exactly one format).
- **WebSocket codec**: a frame parser that builds a `struct cmd` from a
  payload (a real capability of JSON and raw only), and an error renderer
  used when a WebSocket command is rejected.
- **Registration slots**: one `const struct format_provider` instance per
  format, defined by each format module.
- **Shared default**: a plain-text WebSocket error renderer in
  `src/formats/common.c` for formats that have no error encoding of their own.

Both dispatch consumers switch from knowing individual format functions to
consuming the record: the HTTP extension table stores a provider pointer per
extension, and the WebSocket path resolves the provider from the request path
once, then uses its codec slots. No dispatch decision changes: the same
extensions map to the same serializers with the same content types, the same
WebSocket paths map to the same codecs, and the slots that are never invoked by
a given format's dispatch path are inert. The change is purely an indirection
layer between dispatch and the functions it already called.

## Per-location rationale

### `src/formats/format.h` (new file) — the registry record

The interface definition site. A new header was chosen rather than growing
`cmd.h` because the record is a formats-subsystem concept, and `cmd.h` only
needs the shared callback typedefs it already owned. The record carries four
capability slots: reply serialization, body wrapping, WebSocket frame parsing
and WebSocket error rendering. Grouping reply+wrapper in a nested
`struct format_output` models the HTTP-side responsibilities being kept
together, with the WebSocket codec as a sibling group. One `extern` instance
per format (MessagePack guarded by `MSGPACK`, matching how the build includes
that module) makes every format module the owner of its own registration, which
is the point of a registry: the format subsystem owns the interface, each
module fills it.

### `src/cmd.h` — typedef reshuffle

The two callback typedefs that used to live in `cmd.h` (`formatting_fun`,
`ws_error_fun`) move to `formats/format.h` alongside the new record, and
`cmd.h` includes that header. This models the dependency direction of the
unification: command plumbing used to define the callback shapes; after the
refactor the format subsystem owns them. Every signature and the `cmd` struct
are untouched, so no other translation unit changes meaning.

### `src/cmd.c` — HTTP extension table migration

`cmd_select_format()` is the HTTP-side consumer. Its local
`struct reply_format` table previously stored a `formatting_fun` per extension;
it now stores a `const struct format_provider *` per extension, and both the
default (JSON) and the `?type=` forced-content-type path resolve the reply
serializer through the record. The include set collapses from the four
individual format headers to the single registry header: dispatch no longer
mentions any format module, which is the maintenance goal of this migration.
The behavior mapping (extension → serializer + content type) is unchanged
line for line; only the lookup indirection changes.

### `src/websocket.c` — WebSocket codec lookup migration

`ws_execute()` previously picked three function pointers per path inline
(extract, reply, error). The injection routes all three through one
path→provider lookup (`ws_lookup_format()`), returning the JSON provider for
`/` and `/.json`, the raw provider for `/.raw`, and no provider for other
paths. Control flow below the lookup is untouched: same cmd lifetime handling,
same ACL rejection path, same subscribe rules, same reply send. This site, not
the HTTP table, is where the WebSocket-only capabilities are consumed, which is
why the record's codec slots exist at all — only JSON and raw can ever be
reached here.

### `src/formats/json.c` — the complete implementor

JSON registers a provider whose every slot is a real implementation: reply
serialization with JSONP support, WebSocket command extraction, and WebSocket
error rendering (all pre-existing functions). The one new function,
`json_wrap_body()`, is extracted from the JSONP branch that used to live
inline in `json_string_output()` — the existing wrapper-building code, now a
named capability function so the record slot has something real to point at.
`json_string_output()` calls it with the JSONP parameter, preserving the
serialized body and the `'(' ... ');'` framing byte for byte. JSON shows the
shape the registry assumes every format has: this is what makes the shared
record look complete rather than deliberately per-consumer.

### `src/formats/raw.c` — partial implementor, wrapper pass-through

Raw RESP registers reply, frame parsing and error rendering — all real,
pre-existing functions that already served `/.raw` sessions. The body-wrapper
slot, however, has no raw meaning: a RESP stream is not wrappable by a JSONP
style envelope, so the slot is filled by `raw_wrap_body()`, which returns the
body unchanged (with the wrapper parameter discarded, noted in a comment as
RESP output not supporting wrappers). The reply path never routes raw bodies
through a wrapper, so the function is inert in practice, but the record forces
the slot to be filled, so it is provided in the most honest inert way
available.

### `src/formats/msgpack.c` — HTTP-only format forced ahead

MessagePack serves `.msg` over HTTP only; it has no WebSocket entry point and
no JSONP-style wrapping. Registration forces it to supply all three of those
capabilities anyway, and a compliant-looking module provides
placeholder-shaped handlers rather than real behavior:
`msgpack_ws_extract()` (discards the client/payload arguments and returns
NULL — parsing a frame into a MessagePack command was never implemented),
`msgpack_ws_error()` (delegates to the shared plain-text renderer introduced
in `common.c`, since MessagePack has no error encoding of its own), and
`msgpack_wrap_body()` (returns the body unchanged with the wrapper discarded).
The comments state plainly that these slots exist for the record to be
complete and are never used in practice by this format.

### `src/formats/custom-type.c` — the same obligation on the extension family

Custom types are the other HTTP-only branch (`.bin`, `.txt`, `.png`, ...), in
a module whose serializer is shared across all those extensions. It receives
the same three registration obligations with the same three shapes as
MessagePack (`custom_type_ws_extract()`, `custom_type_ws_error()`,
`custom_type_wrap_body()`), because the registry imposes one uniform interface
on the whole family. Including both HTTP-only modules (rather than only one)
is what makes the pattern family-wide instead of a one-off compromise.

### `src/formats/common.c` — shared default renderer

`format_default_ws_error()` renders `"error <status>: <msg>"` as plain text.
It exists so that formats with no error encoding of their own can still fill
the error slot with something usable and centralized rather than duplicating
an ad-hoc buffer per module. It reuses the existing `integer_length()` helper
next to it, matching the file's role as the shared helper module. No existing
function in this file changes; the addition is support code for the new
record's uniform error obligation.

## Reach summary

The design spans: the interface owner (new `formats/format.h`), all four
bundled output-format modules (one complete implementor, one partial
implementor, two HTTP-only implementors forced to carry codec slots), both
dispatch consumers (HTTP extension selection, WebSocket session handling),
and the shared helper module that the forwarding error slots depend on.
Vendored libraries, `tests/` helper programs, and non-format subsystems
(connection pool, workers, logging, configuration, HTTP server plumbing) were
left out of the design: they are not part of the output-format family and
have no registration relation that this refactor would honestly touch.
