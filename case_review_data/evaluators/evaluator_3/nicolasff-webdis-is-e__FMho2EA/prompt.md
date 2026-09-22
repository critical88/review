# Output-format registration forces every format to implement capabilities it never serves

## Observation

The output-format pipeline — the format modules under `src/formats/`, the
extension-driven HTTP reply selection, and the WebSocket request handling —
routes all format dispatch through one shared capability record. Every bundled
output format must register through that record, and the record demands the
complete capability set from every registrant: the HTTP reply serializer, an
output body wrapper, a WebSocket frame parser, and a WebSocket error renderer.

That obligation does not match what the formats actually do. Only two formats
can serve WebSocket sessions today (the JSON codec and the raw RESP codec);
the remaining formats are extension-only and reached over HTTP, yet their
registration entries still have to provide WebSocket parsing and error
handlers — which they do with placeholder-shaped functions (returning nothing
useful, handing the payload straight back, or delegating to a shared default
renderer). Only one format has a genuine use for the body-wrapper capability;
the rest fill that slot the same placeholder way. Meanwhile the two dispatch
sites do not even use the record the same way: the HTTP extension table only
ever consumes the reply slot, while the WebSocket path only consumes the
codecs of the two formats that have a WebSocket entry point. In other words,
the format family is forced to carry interface members that exist for one
consumer, and each consumer is forced to see capability slots that most
registrants never genuinely implement.

## Requested outcome

Redesign the registration so that responsibility follows use:

- Each output-format module should own and expose only the capabilities it
  genuinely provides; a format must not be required to supply ersatz
  implementations of request paths it cannot serve.
- Each dispatch consumer (HTTP extension selection, WebSocket session
  handling) should depend only on the capabilities that path actually uses.
- Formats that are HTTP-only should be registered for HTTP dispatch without
  carrying WebSocket (or wrapper) obligations; the WebSocket-capable formats
  should continue to provide the full codec for their sessions.
- Do not add new formats, new extensions, or new request paths; do not remove
  any existing dispatch behavior.

## Boundaries that must hold

The change is a restructure of internal wiring, not a behavior change:

- Every existing extension and forced content type must keep selecting the
  exact same reply serializer and content type as before (`?type=` forcing
  included), with the default format unchanged.
- JSONP-style output wrapping must still wrap the same bodies the same way.
- WebSocket sessions on `/`, `/.json` and `/.raw` must behave identically:
  frame parsing, command execution, ACL rejection rendering, subscribe /
  unsubscribe semantics and reply framing.
- Formats that never had WebSocket behavior must not gain any.
- The project must build cleanly under its existing warning flags, and the
  full test suite must pass unchanged.

## Where to look

The format modules under `src/formats/`, the capability record they register
through, the HTTP extension table that drives reply selection, and the
WebSocket handler that resolves a codec per request path are the affected
subsystem. Investigate which capability slots each format fills with real
implementations versus placeholders, and what each consumer actually reads —
then reshape the interfaces so neither side carries unused obligations. The
request paths listed above are the behaviors to verify your rework against.
