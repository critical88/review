# Injection design record — aws-lambda-go-api-proxy conversion pipeline extraction

## Maintenance motivation

The `core` package converts three AWS event flavors into `*http.Request`
objects and back into gateway responses:

- REST API v1 (`events.APIGatewayProxyRequest`),
- HTTP API v2 (`events.APIGatewayV2HTTPRequest`),
- ALB target group (`events.ALBTargetGroupRequest`).

Each `EventToRequest` method grew as a single long procedure: decoding the
body, normalizing the path, resolving the custom host, assembling the URL
with the query string, building the `*http.Request`, and propagating
headers. The same is true of each `GetProxyResponse` method on the response
side, which encodes the body and emits the gateway response. A maintainer
working on readability and on making each conversion stage independently
testable wants each stage to be a named method with a single
responsibility, so that a reader can follow the conversion as a sequence
of well-named steps rather than a wall of inline code.

## Normal development evolution being modeled

The change models a straightforward *extract method* refactoring pass: the
inline conversion stages in each accessor are pulled out into named
helper methods that take the values they need as explicit parameters.
This is the kind of change a maintainer makes when preparing to add a new
event flavor (so the existing stages can be reused as a checklist) or
when giving each stage a doc comment so that the conversion contract is
visible at a glance. The extraction is performed per accessor rather than
across accessors because each flavor has small but real differences in
how a given stage reads its event, and because several stages read
receiver state (the configured base path); the maintainer keeps the
per-accessor shape to preserve those differences and to keep the
behavior identical.

## Overall design

For each of the three request accessors, `EventToRequest` is rewritten to
extract its raw method and path, decode the body, strip the base path,
resolve the custom host, append the query string, assemble the core
`*http.Request`, and copy the headers, each as a call to a named helper
method on the same receiver. For each of the three response writers,
`GetProxyResponse` is rewritten to encode the body and then build the
gateway response, each as a call to a named helper method on the same
receiver. The extracted method signatures carry the values the stage
needs as explicit parameters; the receiver is used only for state that is
genuinely per-accessor (the configured base path and the accumulated
response writer state). No public symbol is renamed, removed, or
re-shaped; adapters and external callers are untouched.

## Per-cluster rationale

### Request assembly — `assembleCoreRequest`

`RequestAccessor.EventToRequest`, `RequestAccessorV2.EventToRequest`, and
`RequestAccessorALB.EventToRequest` each end by building the `*http.Request`
from the upper-cased method, the fully-qualified URL, the decoded body,
and the raw method and path (kept for diagnostics). The maintainer pulls
this terminal step into `assembleCoreRequest(method, requestURL, body,
rawMethod, rawPath)` on each accessor. The five parameters are the values
the assembly step consumes; the raw method and path are passed
explicitly because they are read from different event fields per flavor
and the maintainer wants the assembly step to be flavor-agnostic about
*where* they came from while still receiving them. This site and shape
were selected because the `*http.Request` construction is the most
parameter-heavy terminal step and the most obvious candidate to name
separately.

### Query string assembly — `appendQueryString`

The REST API and ALB accessors assemble the URL query from multi-value and
single-value query parameters. The maintainer extracts
`appendQueryString(requestURL, multiValueQueryParameters,
queryStringParameters)` on `RequestAccessor` and
`RequestAccessorALB`. The two map parameters exist because API Gateway
exposes both representations and the assembler falls back from one to the
other; passing both explicitly preserves that fallback logic. The v2
accessor uses a raw query string and gets its own `appendRawQueryString`
helper instead, so it is intentionally not folded into this extraction.

### Request header propagation — `copyRequestHeaders`

The REST API and ALB accessors propagate event headers onto the
`*http.Request`, preserving both single-value and multi-value
representations. The maintainer extracts
`copyRequestHeaders(httpRequest, headers, multiValueHeaders)` on
`RequestAccessor` and `RequestAccessorALB`. The `httpRequest` parameter
is the request being populated; the two header maps reflect the two
serializations the gateway sends. The v2 accessor receives only
single-value headers and uses a distinct `copyRequestHeadersV2` helper
that splits comma-separated values, so it is intentionally not folded
into this extraction.

### Gateway response assembly — `buildGatewayResponse`

`ProxyResponseWriter.GetProxyResponse`,
`ProxyResponseWriterV2.GetProxyResponse`, and
`ProxyResponseWriterALB.GetProxyResponse` each emit their gateway response
from the status, headers, encoded body, and base64 flag. The maintainer
extracts `buildGatewayResponse(status, headers, body, isBase64Encoded)`
on each writer. The four parameters are exactly the response fields the
gateway expects; the body and flag come from a body-encoding step, and
status and headers come from the writer's accumulated state. This shape
was selected because the terminal response emission is the natural
complement of the request-assembly extraction and reads the same kind of
per-response data in every flavor.

### Flavor-specific and receiver-specific helpers (not folded across accessors)

`decodeRequestBody(rawBody, isBase64Encoded)` is extracted on each
accessor because the decode step is identical in shape but reads from
each event's own body flag; the maintainer keeps one per accessor so the
extraction is uniform across the request pipeline.

`stripBasePathFrom(eventPath)` reads the receiver's configured base path,
so it is necessarily a per-accessor method; it is extracted for
consistency with the rest of the path-handling stages.

`applyCustomHost(eventPath, domainName)` on the two API Gateway accessors
and `applyALBHost(eventPath, hostHeader)` on the ALB accessor resolve the
custom host. They are kept separate because only the API Gateway flavors
honor the `GO_API_HOST` override; the ALB variant intentionally preserves
the commented-out override lookup so the flavor difference stays visible.

`appendRawQueryString`, `copyCookieHeader`, and `copyRequestHeadersV2` are
the v2-specific counterparts of the query and header stages; they are
extracted alongside the v2 `EventToRequest` rewrite but kept separate from
the REST API / ALB variants because the v2 serialization differs.

`responsePayload()` is a small collector introduced on each response
writer so `GetProxyResponse` reads as a single terminal step; it gathers
the writer's accumulated status, headers, and encoded body.

## Production roles served

The extraction gives the conversion pipeline a named stage for each
responsibility — body decoding, path normalization and base-path
stripping, custom-host resolution, query-string assembly,
`*http.Request` assembly, header propagation, response body encoding,
and gateway response assembly — across all three event flavors. Each
stage now has a doc comment stating its contract, and each flavor's
`EventToRequest` / `GetProxyResponse` reads as a short sequence of stage
calls. The change touches only the `core` package; the adapter packages
are unchanged because they are deliberately thin wrappers that delegate
to `core`.
