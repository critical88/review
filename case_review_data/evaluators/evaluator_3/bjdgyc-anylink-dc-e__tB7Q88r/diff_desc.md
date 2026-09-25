# Injection design record — anylink data-clump threading (`bjdgyc-anylink-dc-e`)

This document records the design rationale behind the injected code changes: the
maintenance story they model, the overall shape of the change, and a separate
rationale for every materially changed location or coherent cluster.

## Repository snapshot

anylink is an OpenConnect-compatible SSL VPN server written in Go. The
`server/` module contains the HTTP/tunnel protocol handlers, the session and
address-pool management, and a small database layer. A client connection moves
through a strict lifecycle:

1. **Login verification** — the HTTP auth handler hands credentials to a
   dbdata-level verification entry, which loads the user's group and dispatches
   to one pluggable provider (local, LDAP, RADIUS) through the `IUserAuth`
   strategy interface.
2. **Admission and ip-lease allocation** — the session layer lifts per-identity
   client state, checks connection limits, and asks the ip pool for a dynamic
   address; the pool consults mac records and, failing those, scans the range.
3. **Teardown accounting** — both an ordinary connection close and a forced
   session logout emit a user act-log record through a shared writer.

## Maintenance motivation being modeled

The repo already carries traces of a normal maintenance pressure: auditors and
support engineers want to know *which client* performed a login or held a lease,
not only which account. The clean tree shows that pressure in scattered,
half-finished forms — session records already hold a remote address, a client
user agent, and device fields; the RADIUS provider alone stuffs the client mac
into an untyped `map[string]interface{}` extension before the record disappears
into the protocol encoder; the act-log records carry device columns that some
call paths fill and others leave empty; the lease tables record who received an
address but not where that client appeared from.

The normal evolution modeled here is the unremarkable next step of that
pressure: a maintainer threads the client-identifying attributes (source
address, client user agent, client mac, plus the identity values already
present) through the flows that need them, so that authentication decisions,
lease allocation and audit records are made with full context. The modeled way
of doing it is also the unremarkable one: widen the existing signatures at
every hop, add one more parameter next to the ones already there, and keep the
group values as plain arguments. That is exactly how a cohesive identity group
ends up traveling through three phases of a connection as loose, individually
named values — the situation a later maintainer recognizes as a data clump once
the group has grown so large that every routine signature change drags a dozen
call sites along.

## Overall design

The change widens or creates parameter lists so that three cohesive groups —
which together describe one client — travel as separate scalar values:

- **Login request identity** — name, password, group, remote address, client
  user agent, mac address. It is assembled from wire-request fields at the
  auth boundary and crosses the provider strategy interface.
- **Lease-client identity** — username, mac address, unique-mac marker, remote
  address, client user agent. It is hydrated from stored session state during
  admission and rides along through every allocation decision.
- **User-event identity** — username, group, connection ip, remote address,
  device type, platform version, client user agent. It is assembled at the
  two teardown owners on the way to the shared audit writer.

The three groups live in the two layers that anylink already uses for these
responsibilities (`dbdata` for verification and `sessdata` for session,
pooling and accounting) plus the HTTP auth handler that fronts both. No new
module or cross-layer dependency was introduced; the extension follows existing
call topology so that every widened signature reads like a locally reasonable
choice made in isolation.

## Cluster A — login request identity (`dbdata`, auth handler)

**What changed.** The verification entry gained three trailing client-context
parameters (remote address, client user agent, mac) after the existing
credential triple; the local verification path, the `IUserAuth` strategy
interface method, and both remote provider implementations (`AuthLdap`,
`AuthRadius`) restate the same widening. At the two production call sites the
group is now assembled by hand: the HTTP auth handler reads the fields straight
off the decoded login request, and the group-settings self-test passes empty
client context because a configuration test has no client. The RADIUS provider
adds the mac to its protocol packet only when it is non-empty, and the untyped
`ext` map that previously smuggled the mac into the provider is gone — the
modeled maintainer replaces the ad-hoc map with explicit parameters.

**Why this location and shape.** The verification chain is the one place in the
server where a request's identity is already split across several signatures by
design (credential pair + group + provider record). Growing the client-context
attributes at the end of those same lists is the smallest local change a
maintainer conceivably makes per hop, which is precisely how these lists
accrete: the interface forces every provider — present and future — to restate
the whole trailing group even when, like the local path and most of the LDAP
body, an implementation ignores the client context for its decision. The
strategy boundary is included deliberately: it is the structural reason the
group cannot be shrunk back without touching every provider.

**Production role.** Login verification is the trust decision of the server;
the group carries the evidence needed to log and diagnose it, and the widened
signature documents which part of a login attempt each provider may consume.

## Cluster B — lease-client identity (`sessdata`, admission and ip pool)

**What changed.** During admission, the connection factory now lifts the
remote address and client user agent out of the stored session state alongside
the mac and unique-mac marker it already read, and the portion of admission
that enforces connection limits and obtains an address was factored into a
small gate helper that receives the whole client identity. The lease
allocator, its mac-record recovery paths, the round-robin loop and the
unsigned-range scanner all take the group as a flat parameter list; every lease
decision hop re-forwards it. Diagnostics at each hop consume members of the
group (allocation traces and the exhausted-pool warning now mention source
address and client agent), and lease bookkeeping tables are updated from the
same values.

**Why this location and shape.** The pool admission path was the last place a
support engineer can see *why* a client received a particular address; the
motivation story needs those diagnostics to identify the client, and the
locally smallest way to give the pool that information is again one more
parameter per hop. The gate helper models an ordinary decomposition that falls
out of such work: while wiring the new attributes through, the admission
portion of the connection factory became visually self-contained, so a
maintainer extracts it and passes the identity it needs. The range scanner
carries the group even though only membership bookkeeping uses it — a faithful
echo of how threaded context reaches the deepest helper even when it is only
needed for logging there.

**Production role.** This cluster owns address-lifecycle bookkeeping: limit
enforcement, lease grant, record revalidation, and the wrap-around scan
guarantees for reusing addresses.

## Cluster C — user-event identity (`sessdata`, teardown accounting)

**What changed.** The two teardown paths now assemble their audit event by
hand. An ordinary connection close reads username, group, connection ip,
remote address, device type, platform version and client user agent from the
connection and session objects and passes them as seven scalars, plus the
logout reason code resolved separately. A forced session-level logout does the
same from stored session state with no connection ip, keeping its optional
trailing reason-code channel. Both paths load the reason text and hand the
whole value set to a newly shared writer sink that builds the act-log record.

**Why this location and shape.** The repo's act-log records already have remote
address, device and user-agent columns, but in the clean tree each teardown
path filled them by reaching directly into its own owner object, which left the
record composition implicit and duplicated. The modeled maintainer makes the
first obvious consolidation — route both paths through one writer — and passes
the fields it needs as separate arguments. The slight asymmetry between the
two paths (an explicit connection ip on one, none on the other; the reason code
as a value on one and as an optional channel on the other) is faithful to the
existing teardown semantics and is what keeps the group boundary interesting
for a later consolidation.

**Production role.** Teardown accounting produces the audit trail of who left,
from where, using what device, and why they were disconnected.

## Deliberate structural variation

The three clusters intentionally avoid sharing one canonical shape:

- The login group trails the credential pair across a **strategy interface with
  two remote implementations**, one of which consumes only part of the group
  while the local path re-ignores almost all of it.
- The lease group has **a bool in the middle of two string pairs** and is
  passed **in front of cursor integers** between the loop and the range
  scanner, so the group boundary sits at a different offset in every signature.
- The event group is **interleaved with a value that one path produces and the
  other must omit**, and it shares its call with a resolved reason text plus a
  variadic reason-code channel that stays outside the data group.
- The assembly sites differ by vintage: wire-request fields at the auth
  boundary, mutex-guarded stored-state hydration at admission, hand-picked
  owner fields at each teardown owner, and literal empty context on the
  configuration self-test path — mirroring how groups assembled by different
  people at different times never quite agree.

Diagnostics, comments (in the surrounding code's language and style) and the
two updated test call sites were adjusted the way a developer would touch them
while making this change: tests keep asserting the same externally visible
outcomes, and the touched comments describe the local intent rather than any
cross-cutting plan.
