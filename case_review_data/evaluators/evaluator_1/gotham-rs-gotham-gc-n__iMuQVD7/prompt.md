# Maintenance request: the router file has become the framework's dumping ground

Hi — long-time gotham user and occasional contributor here; I maintain a
middleware stack built on gotham 0.8.

## What I'm seeing

`gotham/src/router/mod.rs` used to be the easiest file in the crate to
read: build a route tree, look a path up in it, delegate to the right
handler, wrap up the response. That is no longer the case.

The `impl Router` block now carries sixteen associated functions, and most
of them have nothing to do with routing:

* `Router::request_id` / `Router::ensure_request_id` — generate or honor the
  `X-Request-ID` correlation value and stash it in the state container
  (uses `uuid::Uuid`, the `RequestId` wrapper, and the `X_REQUEST_ID`
  header constant).
* `Router::client_addr` — dig the client's socket address out of the state
  container via the `ClientAddr` wrapper.
* `Router::path_segments` / `Router::subsegments` — parse a URI path into
  percent-decoded segments and hand out sub-segment views.
* `Router::split_query_string` — turn an omitted query string into a
  key-to-values mapping (form-url decoding and all).
* A whole response-construction toolkit: `Router::empty_response` (which
  stamps the router-wide headers such as `X-Request-ID` on an empty
  body), `Router::response` (content type plus a non-`HEAD` body),
  `Router::redirect_response` with its `permanent_redirect` /
  `temporary_redirect` specializations, `Router::not_found_response` for
  404s and `Router::non_match_response`, which deconstructs a
  `RouteNonMatch` into a status plus an accumulated `Allow` header.
* plus the original `new` / `dispatch` / `finalize_response`, now sharing
  the block with all of the above.

The doc comment on the impl presents this as policy — something about the
router sitting "at the centre of every request" and keeping "request
metadata access, request input parsing and response synthesis" in one
place for easy auditing. I'm sure it felt reasonable one pull request at a
time.

Meanwhile, the modules that used to own this work are husks:

* the four public response helpers in `helpers/http/response.rs`
  (`create_response`, `create_empty_response`,
  `create_permanent_redirect`, `create_temporary_redirect`) are one-line
  calls into the router;
* the path parser in `helpers/http/request/path.rs` delegates its
  constructor and its `subsegments` to the router, and its `segments`
  field had to be opened up to `pub(crate)` so the router could construct
  it;
* the query-string splitter in `helpers/http/request/query_string.rs` is a
  forwarder, and its `is_separator` was widened for the router's use;
* request-id handling in `state/request_id.rs` and client-address handling
  in `state/client_addr.rs` forward to the router, their wrapper structs'
  fields and the modules themselves were widened to `pub(crate)`, and
  `form_url_decode` in `helpers/http/mod.rs` got the same visibility bump.

So the router absorbed the logic, and the owners kept only signposts —
plus, because the donors were only ever reached through free functions,
half of the crate now imports its way through the router to get at things
it could have owned directly.

## Why it matters

Every change to request handling now lands in one impl block, and review
diffs mix routing concerns with unrelated parsing or response work. The
donor modules mislead: reading `helpers/http/response.rs` tells you
nothing about where responses are actually built. New contributors attach
yet another convenience function to `Router` because it is already passed
everywhere — that is exactly how the block got to sixteen members.

## What I'd like done

Untangle it. Put each responsibility back in the module that owns it:
request-id generation/lookup with the state's correlation machinery,
client-address access with the state wrapper that stores it, path
segmentation with the request-path helper, query-string splitting with the
query helper, and response synthesis with the response helpers — and let
`Router` be a router: tree construction, path lookup, delegation to
secondary routers, and finalizing responses hands off without synthesizing
them itself.

I don't care exactly how you re-home things as long as:

* the crate still builds with `cargo build -p gotham` and every one of the
  214 tests still passes with `cargo test -p gotham` — the 106 doc tests
  exercise the public helpers, so keep those examples honest;
* nothing changes for application and middleware authors: the public items
  they import today (`create_response` and friends, `request_id`,
  `client_addr`, the `State` and `Router` APIs) remain available under the
  same paths, and request/response behavior — status codes, `Allow`
  headers on 405s, `Location` on redirects, `X-Request-ID` on every
  response, the existing request-id panic when nothing populated it — is
  byte-for-byte what it is today;
* you don't get creative by deleting or weakening tests, re-exporting the
  router's internals to make the numbers look better, or leaving the old
  functions as stack-passing shims that route through `Router` again;
* after your change, no local type in the crate still carries an interface
  like this. Our team's structure criterion, which our automated review run
  applies to this crate, flags a locally defined struct/enum/union whose
  associated functions, aggregated across its inherent impl blocks, number
  ten or more while referencing seven or more distinct crate-local types
  defined in five or more other modules. `Router` currently puts up
  sixteen / nine / eight against those floors, and the review run has to
  come back quiet on the whole crate, not just on `Router`.

There are many acceptable designs — the helpers can be free functions or
methods on their own types, kept private or re-exported exactly as today —
as long as each module owns its logic and the router coordinates rather
than implements. Please also drop the now-inaccurate coordinator
commentary from the router, and the visibility widenings (`pub(crate)`
modules, struct fields, `form_url_decode`) where your design no longer
needs them.

## Stretch asks

If re-homing is straightforward for you, I would also take a pass at the
router's 404/405/500 synthesis so those paths read clearly next to the
modules that define them — but do not trade behavior for tidiness.

Thanks — this is the last structural complaint I have about 0.8, and it
would be great to close it out.
