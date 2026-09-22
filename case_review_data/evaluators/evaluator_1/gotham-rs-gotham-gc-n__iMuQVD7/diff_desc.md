# Change description: turning `Router` into the request-lifecycle coordinator

## Motivation

This change is a mid-stream refactor of the gotham HTTP framework, written
from the point of view of a maintainer who got tired of chasing request
metadata and response construction across the crate.

In gotham 0.8.x, the pieces that participate in serving one request are
scattered: request-id handling lives in `state/request_id.rs`, client address
handling in `state/client_addr.rs`, path segment extraction in
`helpers/http/request/path.rs`, query string splitting in
`helpers/http/request/query_string.rs`, and response construction in
`helpers/http/response.rs`. Meanwhile `Router` — the type that actually
drives the request — reaches into all of them, and every error path inside
`Handler::handle` builds its own response inline.

The maintainer's argument is that `Router` sits at the centre of every
request, which makes it the natural home for the helpers that support the
rest of the framework: it already sees the `State`, it already has to
synthesize responses for paths the router cannot match, and it already has
to consult the request id for logging. Rather than have "request lifecycle
knowledge" smeared over half a dozen modules, the refactor gathers metadata
access, request input parsing, and response synthesis next to the dispatch
logic that uses them, and leaves the original modules as thin, stable
forwarders so no caller breaks.

## Evolution modeled

The goal was for the diff to read like accretion over time rather than one
deliberate blob:

1. **First, repetitive code inside dispatch got extracted.** The inline
   construction of 404 / 405 / 500 responses inside `Handler::handle` is
   lifted into private helpers on `Router` (`not_found_response`,
   `non_match_response`, `empty_response`), and the repeated
   `request_id(&state)` calls during dispatch go through a single
   `Router::request_id` accessor. At this stage the change looks like
   ordinary cleanup of the dispatch path.
2. **Second, existing public helpers were re-homed.** The response
   constructors in `helpers/http/response.rs`, the request-id accessors in
   `state/request_id.rs`, the client-addr accessor in
   `state/client_addr.rs`, and the path/query parsers in
   `helpers/http/request/` were moved into the router's impl, with their old
   entry points left in place as one-line forwarders so middleware,
   extractors, and the documentation examples keep compiling untouched.
   Each of these moves would plausibly have been its own PR — "make the
   router own response synthesis", "route correlation metadata access
   through the router" — and the resulting diff retains that issue-sized
   granularity: the moves are interleaved with the router's original logic
   instead of being appended as one block.
3. **Third, the new methods grew conveniences.** Once response synthesis
   lived next to dispatch, a family of small factory functions appeared on
   `Router`: `redirect_response` as the common base, `permanent_redirect`
   and `temporary_redirect` as status specializations, `empty_response` as
   the shared starting point that stamps the common headers, and a generic
   `response` used by the public helpers. This is the classic way a
   "coordinator" type keeps growing: every new need gets a new member
   because the type is already passed everywhere.

The interleaving matters for realism: `new`, `dispatch`, and
`finalize_response` stay in their original position among the newcomers, so
the file reads like a type that was edited many times over months, not
padded once.

## Design

`Router`'s inherent impl grows from three associated functions to sixteen:

| Function | Role | Origin |
|---|---|---|
| `new` | construct the router (unchanged) | original |
| `dispatch` | route selection and secondary-router delegation (now calls `Router::request_id` for logging) | original |
| `finalize_response` | turn handler results into responses (unchanged) | original |
| `ensure_request_id` | generate/honor `X-Request-ID` and store it (was `set_request_id`) | state/request_id.rs |
| `request_id` | fetch the correlation id or panic (was `request_id`) | state/request_id.rs |
| `client_addr` | fetch the client peer address (was `client_addr`) | state/client_addr.rs |
| `path_segments` | split a URI path into percent-decoded segments (was `RequestPathSegments::new`) | helpers/http/request/path.rs |
| `subsegments` | derive a sub-segment view (was `RequestPathSegments::subsegments`) | helpers/http/request/path.rs |
| `split_query_string` | split omitted query strings into a mapping (was free `split`) | helpers/http/request/query_string.rs |
| `empty_response` | construct a response with the router's common headers | new, factored from dispatch |
| `response` | populate a response with content type and non-HEAD body | from `create_response` |
| `redirect_response` | build a redirect with a `Location` header | new factory base |
| `permanent_redirect` / `temporary_redirect` | 308 / 307 specializations | from the response helpers |
| `non_match_response` | deconstruct a `RouteNonMatch` into a status plus `Allow` headers | new, factored from dispatch |
| `not_found_response` | the 404 card produced for unroutable paths | new, factored from dispatch |

Everything the router now does used to be reachable through free functions
in donor modules; those functions remain, but their bodies are one-line
delegations into `Router`, e.g.:

```rust
pub fn create_response<B>(state: &State, status: StatusCode, mime: Mime, body: B) -> Response<Body>
where
    B: IntoBody,
{
    crate::router::Router::response(state, status, mime, body)
}
```

A doc comment on the impl presents the consolidation as policy ("keeping
request metadata access, request input parsing and response synthesis
alongside the routing logic ... makes the request lifecycle easy to audit in
one place") and never mentions any external quality signal, so the
arrangement reads as a deliberate architecture decision rather than as a
defect. No test, behavior, or public signature changes: all 214 tests,
including the 106 doc tests that exercise the forwarders, pass unchanged.

## Per-location rationale

- **gotham/src/router/mod.rs** (the hub): the impl absorbs the members
  listed above, interleaved with `new` / `dispatch` / `finalize_response`.
  `dispatch` is edited only where it repeated inline work (`request_id`
  lookups and the inline 404/405/500 synthesis), and the `cfg(test)` module
  is left untouched. This file is where the maintainer's argument lives and
  where every later convenience method lands.
- **gotham/src/helpers/http/response.rs**: the four public constructors
  (`create_response`, `create_empty_response`,
  `create_permanent_redirect`, `create_temporary_redirect`) keep their
  names, signatures and documentation examples but delegate to the router.
  Rationale: these helpers exist almost exclusively for code paths the
  router drives, so their logic belongs next to it; the wrappers remain so
  downstream code does not have to name the router type.
- **gotham/src/helpers/http/request/path.rs**:
  `RequestPathSegments::new` and `RequestPathSegments::subsegments`
  delegate to `Router::path_segments` / `Router::subsegments`; the private
  `segments` field is widened to `pub(crate)` because the router now
  constructs the struct. Rationale: path segmentation is derived directly
  from the definition of the dispatch path, so the router should own it.
- **gotham/src/helpers/http/request/query_string.rs**: the free `split`
  function delegates to `Router::split_query_string`, and `is_separator`
  is widened to `pub(crate)` so the router can use it. Rationale: same
  story as the path parser — the router consults the raw query string
  during dispatch, so the split should live with it.
- **gotham/src/helpers/http/mod.rs**: `form_url_decode` (which backs
  `FormUrlDecoded` / `PercentDecoded`) is widened from private to
  `pub(crate)` so the router's parsers can call it. One line, no other
  change to the module.
- **gotham/src/state/request_id.rs**: `set_request_id` and `request_id`
  become forwarders; the `RequestId` wrapper struct's field and the module
  itself are widened to `pub(crate)` so the router can construct it. The
  accessor's panic message is preserved verbatim because a
  `#[should_panic]` unit test pins it.
- **gotham/src/state/client_addr.rs**: `client_addr` becomes a forwarder
  and the `ClientAddr` field is widened. `put_client_addr` stays put — it
  is used by `State::from_request`, not by the router.
- **gotham/src/state/mod.rs**: the `request_id` module becomes
  `pub(crate)` so the router (naming `crate::state::request_id::RequestId`)
  can construct the wrapper. `State::from_request` continues to call the
  free functions as before.

The prop-plausibility rough edges are intentional and limited to exactly
what the maintainer would have needed: `pub(crate)` widenings for the
wrapper struct fields and helpers that the router's new bodies touch, and
nothing else. The donors do not grow any compensating abstraction — they
are strictly thinner after the change, which is what makes the
consolidation look like simplification rather than restructuring.
