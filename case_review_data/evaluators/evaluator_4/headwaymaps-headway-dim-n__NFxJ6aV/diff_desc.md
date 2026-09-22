# Injection design record — headway @ `0fa3e58`

This record documents why the change exists and how each hunk was shaped. It is
written for a reviewer of the diff itself: what a developer on this repository
was plausibly trying to do, why each location looks the way it does, and what
role each piece plays in the running services. It is a design record, not a
debugging transcript.

## The maintenance motivation

The repository is headway: three Rust services that turn transit data (GTFS, a
transitland-derived atlas, OpenTripPlanner) into map-ready routes. Two are API
services — `travelmux` publishes an OSRM-compatible route API in two
generations (`api/v6`, `api/v7`) plus a GraphQL-derived step-by-step plan API —
and one service (`transit-zoner`) renders a zone's feed list as JSON for an
interactive map page. The `gTfsout` workbench crate (`gtfout`) performs the
heavy lifting: downloading and measuring feeds, and rendering an OTP
`router-config.json`.

The diff models a developer in a hurry during a performance-and-preparation
phase. The belief at the center of every hunk is the same classic one: *helper
calls on hot paths cost more than they read for.* Where that belief lands, the
developer pastes the callee's implementation into the call site, leaves the
callee sitting in the file (or orphaned), adds a comment explaining why the
inline form is deliberate, and moves on. Some of the hunks carry a second
motivation — preparing a follow-up feature that "needs the intermediate value
right here" — which is the other maximally common reason real code acquires
paste-ins: the developer wants to keep an intermediate result within reach
while sketching what comes next, and the shortcut is to stop delegating.

## The normal evolution being modeled

Copy-inlining is not an exotic event; it is a routine degeneration that
mid-sized codebases accumulate one unremarkable commit at a time. This diff
compresses that history into one change and deliberately keeps the texture of
each individual act:

- comments that rationalize the inline form in the vocabulary of the day
  ("hot path", "follow-up", "right beside", "instead of delegating");
- glue bindings introduced only to make pasted code compile at its new home
  (`let leg = &otp;`, `let state: &State = &state;`, aliases for call-site
  argument shapes);
- the original functions left in place after being absorbed, sometimes still
  serving neighboring call sites and sometimes newly orphaned;
- a couple of aging markers (`REVIEW`, `TODO`) that ride along with the copied
  regions because the developer copied them too;
- local re-shapings (extra braces around pasted blocks, promptly-invoked
  closures instead of helper calls, an explicit type annotation forced by a
  later consumer) that make the pasted layer drift from the helper it came
  from.

## Overall design

Six production functions, in six files, across all three workspace services,
each absorb the implementation of another function that lives in the same
file. Two of the six go three call-levels deep: the absorbing body contains
not just the first helper's implementation but that helper's own sub-helper
implementations, fully expanded. The absorbed sources vary in kind: inherent
methods, free functions of the module, and a config policy closure. The
absorbing owners vary too: a `From`-trait conversion, an inherent constructor,
an inherent method on a domain type, and free handler functions.

Choice of sites was driven by the repository's real call chains: this
codebase already has the multi-level same-file helper structure that invites
this kind of accident — route translation walks `RouteLeg` → `RouteStep` →
banner assembly; plan assembly walks `Leg` → `Maneuver` → instruction/duration
helpers; feed work walks `verify_feed` → `fetch_feed` → `authenticated_request`.
The diff thickens chains that exist, rather than inventing new structure, so
every changed hunk sits inside a relationship the file already established.

Below, each cluster is described with what changed, why that site and shape
were chosen, and the production role the code plays.

## Cluster 1 — `services/travelmux/src/api/v7/osrm_api.rs` (`RouteLeg::from`)

**What changed.** The middle-step construction inside the current-generation
route translation had delegated each interior maneuver triple to
`RouteStep::from_maneuver`. That call is gone. In its place the conversion
carries the whole step build: banner text assembly, the maneuver type and
direction mapping (every internal `ManeuverType` variant to OSRM's vocabulary,
transit and ferry and building entries included), the banner component list
construction with its delimiter joining, bearing computation, and the `RouteStep`
struct literal. `prev_maneuver` and `next_maneuver` are re-bound as `Option`s so
the pasted body's optional-argument handling stays in reach.

**Why this site and shape.** This is the deepest existing chain in the
repository (`RouteLeg::from` → `RouteStep::from_maneuver` →
`VisualInstructionBanner::from_maneuver` → component assembly), so the expanded
form naturally goes three levels deep in one paste. The enclosing code is a
`windows(3).map(...)` closure run per interior step — exactly the shape a
developer profiles and then "optimizes" by hand. The first and last step of a
leg still delegate through `RouteStep::from_maneuver`, which is what a real
partial expansion looks like: only the per-triple hot loop was pasted over.

**Production role.** This conversion is the serialization boundary for the
current OSRM-compatible route API. Everything a client sees for an interior
step — its banner instruction, its maneuver type and direction, its bearings
and its distance fields — is assembled by exactly this code, in one place,
inside the v7 response model.

## Cluster 2 — `services/travelmux/src/api/v6/osrm_api.rs` (`RouteLeg::from_leg`)

**What changed.** The same story, one API generation back: v6's route
translation absorbed the identical step-construction pipeline into its own
middle-step closure. The v6 module has its own copy of the same helpers, and
the expansion here threads v6's distance-unit parameter through, which is why
the pasted distance conversions name `from_distance_unit`.

**Why this site and shape.** The v6 module is the compatibility generation:
same pipeline, older types, an extra conversion parameter. Duplicating the
expansion in both generations is what a developer "fixing" the same profile in
both paths does — the v6 and v7 modules are separate parallel implementations
on purpose (they serialize to different schemas), and a developer working
across both naturally repeats the move in each. Keeping the distance-unit
thread intact was required for v6's numeric behavior anyway, so the paste
adapts rather than truncates.

**Production role.** This is the v6-api serialization boundary. Legacy clients
fetch the v6 schema; their interior steps are built by this code with
distance-unit conversion applied at every distance read.

## Cluster 3 — `services/travelmux/src/api/v7/plan.rs` (`Leg::from_otp`)

**What changed.** The v7 step-by-step plan API used to build each interior
plan step by handing the GraphQL-leg's step to `Maneuver::from_otp`, which
itself called three same-file helpers: `maneuver_instruction` (the natural
language turn description), `build_verbal_post_transition_instruction` (the
"Continue for …" tail), and `leg_duration_seconds` (leg duration with the
elapsed-time fallback). The leg loop now builds the entire `Maneuver` itself:
relative-direction handling, the spoken instruction lifted from
`maneuver_instruction`'s match — with its `mode` arm now matching on
`leg.mode.as_ref()` directly, since the paste has the leg in hand — the
post-transition instruction, street-name handling, bearings, duration
apportionment, and the struct literal.

**Why this site and shape.** This site is the turn-by-turn heart of the plan
API, and the chain here (`Leg::from_otp` → `Maneuver::from_otp` → the three
helpers) is the natural second deep chain in the repository. The paste is the
fully expanded form: sub-helper implementations appear with their parameters
substituted, which is why the instruction matcher no longer reads like
`maneuver_instruction`'s own body. A glue binding `let leg = &otp;` exists so
the pasted body, which refers to its caller's leg, resolves at the new site.
`leg_duration_seconds` deliberately survives elsewhere (the transit-leg path
still calls it), while the instruction helpers for the OTP path are left
orphaned in the file.

**Production role.** This loop produces the guidance text people hear and see
in the headway UI for a transit itinerary: each step's instruction, spoken
post-transition continuation (rendered in the requested measurement system),
apportioned duration, and geometry bearings.

## Cluster 4 — `services/gtfs/gtfout/src/measure.rs` (`verify_feed`)

**What changed.** The credential-verification routine used to compose
`authenticated_request(...)` into the shared download helper `fetch_feed` and
wrap the failure in a redaction pass. It now performs the request itself: the
`authenticated_request(...)?` chain, `send`, `error_for_status`, and a
byte-collect, kept in a binding with an explicit result type. The doc comment
gains the reasoning a developer would write: several agencies answer 200 with
an error page, so the verification downloads the entire zip rather than
trusting the status line, and keeping the body local is preparation for a
checksum-based decision about whether a credential is worth keeping.

**Why this site and shape.** `verify_feed` and `fetch_feed` are siblings over
the same request machinery — the classic place a developer "unwraps one level"
to hold on to the intermediate value. The pasted chain mirrors `fetch_feed`'s
implementation exactly, because that is the level being unrolled. The
workbench has no shipped users of these functions at runtime, so the change is
invisible to the services that consume `gtfsout` binaries.

**Production role.** This is the diagnostic path that decides whether a
configured GTFS credential actually fetches bytes. It runs against real agency
endpoints, several of which misbehave, which is why the verification downloads
completely rather than trusting statuses.

## Cluster 5 — `services/gtfs/gtfout/src/transit_zone/router_config.rs` (`RouterConfig::for_zone`)

**What changed.** The OTP router-config renderer resolved each realtime feed's
credential through the module's free function `credential(...)` and named each
stream's updater through `otp_updater(...)`. Both implementations moved into
the render pass: the credit resolution became a local closure taking the same
two arguments, and the updater naming became an inline match over the stream
kind. The free functions stay in the file, now unreferenced.

**Why this site and shape.** The developer's stated reasoning is locality:
zone policy belongs beside the render pass that applies it, and OTP's updater
naming belongs beside the updater build. This cluster shows the absorption
going the *other* direction — from free function to local closure — because
that is the natural shape a developer reaches for when the absorbed logic maps
one-to-one onto loop-local state. The closure's parameters deliberately
re-bind the same `credentials` config the free function took, so pasted
references resolve unchanged.

**Production role.** This renders the `router-config.json` an OTP deployment
consumes at boot: the updater list with feed ids, update frequency, URLs, and
auth headers; plus the record of realtime feeds skipped for lack of a usable
credential. The renderer runs in the OTP init container, where live tokens are
interpolated, and must not land in committed manifests.

## Cluster 6 — `services/gtfs/transit-zoner/src/main.rs` (`list_feeds`)

**What changed.** The bounding-box endpoint used to call `summarize(...)`, the
shared row builder that both endpoints use to assemble `FeedSummary` records.
It now builds the rows itself: the alias bindings for state/found-ids/area
taken from the call site's own values, the same per-feed row assembly
including realtime summaries and relevance, and the same relevance-then-id
sort. `summarize` stays in place for the other endpoint.

**Why this site and shape.** The comment tells the truth about the
motivation: the bbox page hits this endpoint on every pan and zoom, so the
developer wanted row construction "right beside the sort that orders them."
This is the largest single-function handler in the zoner, and the paste
absorbs a parameterized free function - again with glue bindings adopting
the call site's argument shapes (`Some(&area)` becomes a local, the found ids
borrowed as they were passed).

**Production role.** This is the live query path of the map-selection page:
given a bbox, which offerable feeds intersect it, best match first. The
response feeds the interactive map's feed-selection UI, so every pan and zoom
exercises it.

## Deliberate structural variation

The six clusters are intentionally not copies of one editing move:

- two of them expand three levels deep inside per-element closures (clusters 1
  and 2), one expands three levels in a straight-line loop body (cluster 3),
  and three absorb single implementations (clusters 4, 5, 6);
- the absorbed sources are, in turn, an inherent method called nowhere else
  after the change, methods still used by neighbor code, free functions left
  orphaned, and a free function still shared with another endpoint;
- the paste shapes differ: a block-valued `let` with an explicit result type
  (cluster 4), a local closure capturing the config (cluster 5), Optionalized
  re-bindings of arguments (clusters 1, 2), a `&`-reborrow of the enclosing
  parameter (cluster 3), and a block expression feeding a later sort
  (cluster 6);
- the justifying comments vary in register: measured latency (clusters 1–3,
  6), a pending checksum behavior (cluster 4), policy locality (cluster 5).

This variation mirrors how such regions actually accrete in production code:
not one systematic campaign, but six independent decisions that each made
sense to the person making them at the time.

## Behavior and compatibility

Nothing in this change alters the running services' observable outputs: the
pasted implementations implement the same computations as the functions they
replace, with the same fields, conversions, orderings, and error surfaces, and
the change holds the runtime services' external contracts fixed. Nothing about
the change's shape is meant to freeze it in place either - the explicit
result-typed `let`, the unused capture, and the aliased ghosts of the call
sites are exactly the seams a later cleanup would pull back out, and the doc
comments name the follow-ups the developer left half-started (a checksum
consumption for the verification path, and the elimination, in each API
generation, of the very duplication the comments now paper over).

## On what the reviewer will find

Each cluster is self-contained and can be assessed on its own: the pasted
text is in the absorbing function, the callee is in the same file, and the
comments and bindings mark the seams. A reviewer who prefers different
boundaries than the original authors drew - different helper shapes,
different names, different seams per service - will find the commented regions
are the places to look; the key question is whether the services' external
contracts ever change, because for this layout the seams the comments now
cover are exactly the candidate extraction boundaries.
