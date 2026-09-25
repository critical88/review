# Design record for the connection-policy change in pgsql-http

This record documents the maintenance change contained in `smell.diff` against the
pinned revision of pgsql-http: the motivation that produced it, the development
evolution it models, the overall design, and a per-cluster rationale for what was
written where and why that site and that shape were chosen. It is an auditable
design record, not a review of any subsequent work.

Repository context: pgsql-http is a single C translation unit (`http.c`) compiled
as a PostgreSQL extension. Every SQL-visible entry point (`http_get`, `http_post`,
`http_head`, `http_set_curlopt`, `http_reset_curlopt`, ...) funnels into one C
request function that performs transfers on a process-global libcurl easy handle.
Session-level curl options are managed through a table of settable options that
are mirrored into specially named custom GUCs (`http.CURLOPT_TIMEOUT_MS` and
friends, including the keepalive knob), so a session "opts into" TCP keepalive
the same way it sets a timeout. In the pinned revision, the request function was
the only place that turned the keepalive opt-in into concrete connection
behavior: it chose whether to force connections closed, which connection header
to send along, and whether to release the handle after the transfer.

## Maintenance motivation

Two operational complaints drove the work, both of the kind that arrive as small
issues rather than a feature request:

1. **First-request latency.** Ops noticed that the first outbound request from a
   backend runs measurably slower than the rest, because the global handle does
   not exist until a request needs it. The natural fix in a single-file extension
   is to create the handle eagerly when the module loads, so the first caller
   skips the setup cost. Creating it eagerly also means the handle now exists
   *before* any session state can be read — which immediately raises the question
   of what connection behavior it should carry at that moment.

2. **Connection behavior lagging session changes.** Users who enable keepalive
   mid-session reported that, depending on what they did right before the next
   request (set another option, reset all options), the request did not behave
   as if the session had keepalive on. The handle's configuration is only
   adjusted at transfer time, and any code path that touches the handle in
   between — resets it, or rewinds its stored option values — can leave it in a
   state that does not match what the session last asked for.

The developer's answer to both was to give the module an explicit, always-current
notion of "the session connection policy": a cached module-level flag holding
"does this session want connections held open and re-used", refreshed at every
point the handle is created, prepared, reset, reconfigured or used, plus a
permitted-protocol stamp repeated alongside it. Each of those touch points is a
place someone could observe a stale handle, so each one got its own refresh and
its own stamp.

## Modeled development evolution

The change is shaped the way this kind of code actually grows: as a series of
small, locally-justified steps, each closing a consistency hole found at one
lifecycle stage, none of them big enough to trigger a design review of the
coordination cost being added.

1. Cache the session preference once, in module state, instead of re-scanning
   the settable-option table on every request. The request function — the only
   consumer at this point — reads the flag instead of the session state.
2. Warm up the global handle at module load and stamp the policy on it, so the
   first request in a fresh backend starts from a correctly configured handle
   (complaint 1).
3. Handle acquisition is the next hole: a cached handle carried the preference of
   whatever session state existed when it was configured, and the session can
   have changed its mind. Refresh the flag and re-stamp it when the handle is
   (re)prepared, and stamp the permitted protocols at the same time so the
   handle's connection posture is fully defined at acquisition.
4. `http_reset_curlopt` restores the handle to library defaults, which discards
   whatever the previous steps stamped. Re-derive and re-stamp immediately after
   the reset — by symmetry with step 3, so the reset path cannot serve a handle
   with less configuration than the acquisition path.
5. `http_set_curlopt` is the moment the session's option values — including the
   keepalive input itself — can change. Refresh and re-stamp inside the option
   application branch, before the new value is written.
6. The request function keeps its per-request application (now reading the
   cached flag rather than deriving), and its two policy-dependent consumers —
   connection header selection and post-transfer handle release — follow the
   same flag for consistency.

After step 6 nobody re-reads session state to decide connection behavior at
transfer time; they trust a cache that four other functions keep valid. That is
the state this diff leaves the module in.

## Overall design

- **One concept**: the session connection policy, with two attributes — network
  socket re-use, driven by the session's TCP keepalive opt-in, and the set of
  transfer protocols the handle may speak.
- **One channel**: a module-scope cached boolean holding the keepalive decision,
  documented as session connection policy state, refreshed at four sites
  (startup, acquisition, reset, option change) and consumed at request time.
- **Stamps everywhere the handle is touched**: each lifecycle site re-derives the
  flag and applies the policy attributes to the handle directly with plain
  libcurl calls.
- **Behavior preservation throughout**: the stamps are placed so that a request
  always observes the same connection behavior it would have observed before
  the change. Each request passes through handle acquisition before the request
  function reads the flag, so the cache is always fresh at the point of use
  regardless of which lifecycle site ran last. Stamps are written after
  session options are populated in the acquisition path and before the
  user-chosen value is applied in the option path, so a user-set option for an
  option in the settable table is never overridden by a stamp (the two
  attributes are not user-settable anyway). Where the policy attribute is
  `false`, the stamped value equals the libcurl default, and a fresh, unstamped
  handle produced by the reset path is brought back to the same posture by the
  re-stamp; a handle created but never configured (module load with no request
  yet) is inert.
- **Version portability preserved**: the protocol stamp is repeated under the
  same libcurl version guard the request function already uses (string option on
  libcurl >= 7.84.0, bitmask option below), so the acquisition and reset paths
  stay buildable across the supported libcurl range.

## Cluster-by-cluster rationale

What follows is every materially changed region, in file order, with the
reasoning for that location and implementation shape.

### C1 — policy state declaration (module globals)

A module-scope `static bool` serving as the cached connection policy flag, with
a comment explaining the session-connection-policy meaning and the refresh
contract ("refreshed at every point the handle is created, prepared,
re-configured or reset").

*Why this shape*: single-file extension code passes request-scoped state through
module globals already (`g_http_handle` sits right above it), so a plain module
boolean is the culturally consistent channel. It replaces a scan of the
settable-option table (which iterates all entries and asks the custom-GUC
machinery for each) with one boolean read on the request path.
*Production role*: single shared value through which four refresh sites and
three request-path consumers stay synchronized.

### C2 — startup warm-up (module initialization)

Creates the global curl handle at module load, derives the policy for the
startup state, and stamps the socket re-use attribute on the fresh handle.

*Why this site*: complaint 1 is strictly a first-use problem, and the module
initializer is the only code that runs before first use. Creating the handle
here also forces the startup state question: the new handle must not sit
configured contrary to the session policy, hence the stamp.
*Why this shape*: three plain statements with a short comment. The eager
creation knowingly changes the resource lifetime (a handle exists in backends
that never issue a request); the developer accepted that as the cost of
removing the first-request stall.
*Production role*: lifecycle warm-up — first request in a backend starts from a
configured handle instead of paying creation cost inline.

### C3 — handle acquisition (per-request preparation)

After the shared handle has been created or found, and session option values
have been (re)populated into it from the settable-option table: refresh the
cached flag, stamp socket re-use, stamp the keepalive opt-in knob itself, and
stamp the permitted protocols under the libcurl version guard.

*Why this site*: acquisition is the funnel every request passes through, and the
only place where "what the session currently wants" is known at the same moment
a handle is at hand. Any cached-but-stale configuration is observable exactly
here, so this is the natural home of the authoritative refresh.
*Why the protocol stamp too*: confirming that a handle carries the connection
posture the session asked for is easiest to reason about when every freshly
prepared handle is re-stamped in full (posture = re-use + protocols), rather
than trusting that whoever produced the handle did the same.
*Why the keepalive knob stamp*: the TCP-level keepalive knob (idle probes) and
the derived re-use decision are conceptually one setting in this module; the
stamp re-asserts the knob so that the libcurl-level probe setting and the cached
decision cannot disagree. This is a static, unconditional re-assert whose value
tracks the cached flag — on this libcurl it is not the error-checked call form,
which also matches the plain-call style of the surrounding default statements in
this function.
*Production role*: request preparation — makes "handle ready" and "handle
reflects current session preference" one and the same.

### C4 — option reset (session option plumbing)

The reset entry point restores the handle to library defaults, then must put the
connection posture back: refresh the flag, stamp socket re-use, stamp permitted
protocols under the version guard.

*Why this site*: the library-level reset is the one call that wholesale erases
the work done by C3, so the same reasoning that stamps at acquisition applies
symmetrically here — a handle that has just been reset should not be in a less
defined state than one that has just been acquired and prepared.
*Why this shape*: a verbatim copy of the C3 stamp block, computationally
redundant whenever the next request runs acquisition anyway, but locally defensible
as "leave the handle in a known state the moment we disturb it".
*Production role*: reset — keeps the exported reset behavior (options cleared)
from silently extending to "connection posture undefined until next request".

### C5 — option application (session option plumbing)

In the reset-plumbing branch where a user-set option value is stored, just
before applying the new value to the handle: refresh the flag and stamp socket
re-use.

*Why this site*: setting `http.keepalive` (or anything else) on a session
changes the policy input, and this branch is where session option values get
written; the next read of the cached flag should not race the option table. The
stamp accompanying the refresh reflects the "disturb the state, re-stamp the
state" habit by now established.
*Why this shape*: refresh plus one stamp inside the existing match branch; the
protocols are not re-stamped here because nothing in this branch disturbs them —
at this point the pattern is well established, which is itself why nothing
forced the author to reconsider it.
*Production role*: reconfiguration — keeps the cached policy in phase with the
session option table at the exact moment a session value changes.

### C6 — the pair of SQL option functions' surrounding shape (declarations)

A forward declaration of the option-table predicate was added to the
declarations block.

*Why*: the predicate that answers "is this session's keepalive opt-in set" is
defined later in the file than the code bodies that now call it — the startup
warm-up (C2) and the refresh sites (C3/C4/C5) all sit above the definition, and C
requires a declaration before use.
*Production role*: mechanical enablement for calling the input predicate from
earlier lifecycle code; the definition itself is untouched.

### C7 — request execution (per-request transfer)

The request function previously derived the connection decision directly from
session state at transfer time. That derivation is replaced by:

- a per-request socket re-use stamp reading the cached flag, replacing the
  direct-derivation form;
- connection header selection (keep-alive versus close) keyed on the cached
  flag, unchanged in outcome;
- the post-transfer handle release decision keyed on the cached flag,
  unchanged in outcome.

*Why this site*: the request function is the consumer the cache was created
for; once the cache exists and is maintained elsewhere, this function has no
reason to re-derive anything. The header and release consumers read the same
flag so that all three per-request connection behaviors cannot fall out of
phase with the stamp.
*Why the application stays here*: the meaningful application point of the
policy is the moment a transfer is configured; the author kept that as-is and
only swapped the *source* of the decision.
*Production role*: policy consumption and per-request application — the part of
the pipeline the user's request actually hits.

## Properties of the resulting shape

A consequence of the C1–C7 arrangement, stated as design observation:

- The concept "what connection behavior does this session get" is now derived
  in four places (C2, C3, C4, C5) from the same input, stored in one mutable
  module-scope flag (C1), and applied in five places along the handle
  lifecycle; two further request-path behaviors (C7) key on the cached value.
- Any conceptual change to this decision — deriving it, changing what it keys
  on, adding a third attribute (say, connection timeout as a policy knob), or
  changing how sessions opt in — must be traced through every lifecycle site,
  each of which carries its own copy of the derivation and its own reason for
  being there, plus the cached state and its refresh points. Missing any one
  leaves some path of the lifecycle (startup, first use, post-reset,
  mid-session option change, mid-transfer) silently serving different
  connection behavior than the others.
- Every lifecycle stamp and every refresh is redundant with, but consistent
  with, what the acquisition path would do on the next request anyway, so the
  addition is defensible one step at a time, and no intermediate state is
  observably wrong for a request. That pattern — consistent redundancy
  accumulated for local reasons — is precisely what the two complaints and the
  fix steps above produced.
