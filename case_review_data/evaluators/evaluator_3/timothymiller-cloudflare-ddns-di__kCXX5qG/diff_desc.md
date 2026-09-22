# Injection design record — deeply inlined method in cloudflare-ddns

## Maintenance motivation

The update pipeline in this repository is the latency-sensitive part of the
product: every run detects the machine's public addresses and reconciles the
configured Cloudflare DNS records (and, when enabled, a WAF IP list) before
exiting. A developer profiling a slow batch run attributed the overhead to the
per-call indirection in the Cloudflare client layer: each reconciliation step
hopped through several small methods, and the request lifecycle (build path,
pick HTTP verb, send, translate a failure into a user-facing message) was
spread across four levels of delegation. The motivating itch was not
reusability but *visibility and control*: the developer wanted the request
lifecycle of every hot-path operation to be readable in one place, with no
surprising work hidden behind a chain of tiny helper calls, and they were
willing to trade the helper structure for that while the investigation was
hot.

## Normal development evolution being modeled

The edit sequence that produced this diff is the kind that accretes during a
performance-hardening sprint rather than being designed up front:

1. First the transport-adjacent methods were restructured so the request
   knobs (HTTP verb, optional request body, path text) are bound to named
   locals before use, making the request construction explicit instead of
   buried in call arguments.
2. Riding that restructuring, the lowest layer was then absorbed into its
   callers: the body of the shared request/response primitive was copied into
   the zone-record listing method where the generic plumbing had lived, so the
   listing method builds and sends its request itself.
3. The same recipe was then applied upward and outward: the filtering wrapper
   over listing, the per-domain reconciliation loop (update/create/delete
   sites), the WAF list maintenance flow, the legacy update path (per-IP
   commit and the batch update over it), and finally the local-provider
   detection arm in provider dispatch.
4. The original helper definitions were not deleted: the developer left them
   "until the next cleanup pass", so both the new inlined forms and the old
   delegating definitions coexist.

Each step is a locally reasonable edit that a reviewer might wave through on
its own: rename-free copies of bodies into their call sites, small hoisted
adapter bindings, and unchanged observable behavior. The compounding effect —
several abstraction levels collapsed into orchestration methods — is what the
cumulative diff turns into a hard-to-review change.

## Overall design

The injection spans the three network-facing production layers of the update
pipeline, applying one consistent pattern — *replace a delegation with an
adapted copy of the delegate's body, hoisting the caller-side values the body
expects into named locals around it*:

- In the Cloudflare API client service, a three-level delegation chain
  (request primitive -> zone-record listing -> name-filtered listing ->
  per-domain reconciliation) is unreeled so the reconciliation method ends up
  containing a stacked copy: the filtered-listing body, which contains the
  zone-listing body, which contains the request/response plumbing. The
  reconciliation method additionally embeds copies of the record create,
  update, and delete endpoint operations directly at its loop sites, and the
  conditional WAF cleanup flow embeds the three maintenance steps it depends
  on (list lookup, item query, item removal).
- In the legacy update path, the per-detected-IP commit loop embeds the
  single-entry record commit body, and the legacy batch update embeds the
  modified per-IP loop wholesale (receiver and config-slice references are
  adapted to the caller's bindings).
- In provider dispatch, the local-socket detection implementation is embedded
  into its match arm.

Copies are *lightly adapted rather than verbatim*: local bindings are renamed
where they would collide or mislead (`name` -> `fqdn`, config slices and
stringly idents re-bound through small adapter statements), type annotations
are specialized to the call context, and request parameters are hoisted into
locals — the same adjustments a developer mechanically performs when pasting
a body into a new scope. The inlined blocks are interleaved with the
methods' original control flow (compare/dedup loops, dry-run guards, no-op
suppression, early-return cleanup semantics), so the seams between "copied"
and "original" logic are not visually distinct: this models the real-world
version of the pattern where extraction boundaries have become genuinely
unclear.

## Per-location design decisions

### 1. Request-parameter hoisting in the low-level operation methods

**What changed.** The zone-record listing method and the record deletion
method now bind their path text, HTTP verb, and optional body to named locals
before building their requests; the deletion method's request construction
reads through a hoisted verb local the same way the listing one does.

**Why this shape.** These are the two entry layers the later inlining copies
were derived from, so they received the readability restructure first: the
developer wanted every request's verb and body visible as named locals
before they began copying plumbing anywhere. Keeping the statements in the
same order the callers use them later means the copies could be pasted with
only binding-name adjustments.

**Production role.** They remain the definitions of the record listing and
deletion operations; their production callers are gone after the inlining
passes further up, so they now serve mostly as the explicit record of the
endpoint contracts the embedded copies reproduce.

### 2. Record listing chain: transport and filtering unreeled

**What changed.** The zone-record listing method embeds the whole request/
response plumbing (URL composition from the configured endpoint, auth
application, body-less or body-bearing request, status check, error
reporting, response deserialization) in place of calling the shared request
primitive. The name-filtered listing method embeds this modified listing body
in a wrapped block and keeps only its genuinely local step — the
case-insensitive name filter — as the original logic around it.

**Why this site.** The listing pair sits directly under the reconciliation
hot path and is the natural first absorption target once the transport layer
is restructured: the filtering wrapper is thin, so most of its body is the
call it makes. Wrapping the embedded listing in a block expression (binding
the result into a typed local) is the shape a developer reaches for when they
want the paste to read as "the records, computed inline".

**Production role.** The chain of GET plumbing and name filtering serves
every zone reconciliation and stale-record comparison in the run.

### 3. Per-domain reconciliation method: the depth absorption

**What changed.** The reconciliation method now resolves its existing
records by embedding the modified filtered-listing body (which itself
contains the two deeper levels above), binding the result through an adapter
local; and each of its record mutation loop sites — two update sites, one
create site, two delete sites — embeds a copy of the corresponding endpoint
operation body inline, hoisting per-site adapters (record id as a string view,
payload reference, item id slice) in front of each copy. The surrounding
original logic (compare against managed sets, dry-run guards, message
accumulation, dedup suppression, stale-record sweeps) is untouched and now
reads as interleaved with the embedded plumbing.

**Why this site.** This method is the highest-orchestration point in the
client service, carries the most production roles (create/update/delete/
compare/dedup in one loop nest), and is where the motivating "visibility"
argument was aimed: after the edit, every request the flow makes is visible
in its body. It was also the natural place for the depth effect to
concentrate, because its callees were themselves already restructured
(listing) or thin (endpoint operations). The two updates and two deletes are
separate loop phases (current-record updates vs. duplicate cleanup and stale
deletion phases), each embedded independently — a realistic echo of the way
such edits get applied site by site, and a choice that leaves no single
obvious seam to reverse the change from.

**Production role.** This is the money path of the product: for every
configured domain and record type it reconciles desired against detected
state, and it is the method every DNS-affecting CLI run executes.

### 4. WAF list maintenance flow

**What changed.** The conditional cleanup method embeds the WAF list lookup
as a match scrutinee block (keeping its early-return-when-absent semantics),
the item query in a wrapped block with account/list id adapter locals, and
the item removal with its bulk request-body construction inline.

**Why this site.** The cleanup flow shares the same transport plumbing as the
record flow, so once the record sites were inlined the WAF flow looked
inconsistent to the developer ("half the requests here go through the
primitive, half don't"). Because the lookup result drives an immediate
conditional return, its embedding lands in a match-scrutinee position rather
than a plain block — the same body with a caller-shaped integration.

**Production role.** Optional enabled-list hygiene: after DNS updates the
flow keeps the configured WAF allow-list free of IPs this deployment no
longer manages.

### 5. Legacy update path

**What changed.** The per-detected-IP commit loop embeds the single-entry
Cloudflare record commit body (zone resolution sleeps and retries, subdomain
walk, endpoint builds, dry-run echoes, message accumulation) in a wrapped
block inside its loop. The legacy batch update then embeds the modified
per-IP loop itself, adapting the receiver and the config slice to its own
bindings with small adapter statements in front of the embedded block.

**Why this site.** The legacy path is the compatibility mode this repository
still ships for pre-existing users; the performance investigation covered
both update paths so that measurement comparisons between modes would not be
confounded by one path still paying the indirection. The receiver rename
(`self.` -> named client local) is the one adaptation needed to make the
paste work in the free function; the config-slice and ttl/comment bindings
are the caller's way of providing the embedded body's expected locals without
touching its statements.

**Production role.** Legacy-mode uptime: reconciles legacy-configured
Cloudflare records per detected address family on every scheduled run.

### 6. Provider dispatch, local arm

**What changed.** The local-provider match arm in provider dispatch embeds
the local-socket detection implementation (target resolution per address
family, ephemeral bind, connect, local address extraction with global
filtering, warning emitter on each failure mode).

**Why this site.** Detection runs for both address families on every cycle
before any update work, so the developer treated dispatch-hops as part of
the fixed cost. The arm is also the smallest site where the pattern could
still be applied (a two-statement self-contained routine), and embedding it
there kept the diff "consistent" with the other two layers — the same recipe
applied everywhere the investigation had touched.

**Production role.** Default local address detection feeding the whole
update pipeline when the local provider is selected.

## Where the design leaves the code

The completed diff leaves every duplicated definition in place: the
transport primitive, the listing methods, the endpoint operations, the WAF
steps, the per-IP commit, and the local detection routine all still exist as
methods with their restructured bodies, while production traffic now flows
through the embedded copies inside the orchestration methods. The two update
paths (modern and legacy) and the detection arm all exhibit the same
flattening, and the fixed changeful parts (dry-run behavior, message
formatting, endpoint shapes, no-op suppression) are unchanged observable
behavior everywhere.
