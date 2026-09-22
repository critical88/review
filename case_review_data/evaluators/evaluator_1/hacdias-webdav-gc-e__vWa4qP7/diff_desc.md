# Injection design record — hacdias/webdav

## Maintenance motivation

This server grew from one directory served to one set of users into a
multi-user WebDAV server with per-user directories, several mount
backings, WebDAV-class locking, the sabredav-style partial update
protocol, per-request logging and proxy-aware client IPs. Every one of
those features lands somewhere on the life of a request: credentials have
to be admitted, paths parsed and normalized, rules consulted for the
request and its subtree, locks confirmed, capabilities advertised,
partial updates applied and the exchange logged.

The natural place a maintainer reaches for each of those steps is the
request coordinator, because it is the one place that already knows what
the rest of the pipeline needs: the configured users, the proxy flag, the
per-user prefix, and the per-user pair of filtered file system and
WebDAV handler. Folding a new step in as a method there means it can read
that state without threading a dozen parameters, which is exactly what
happens over a few releases of feature work: request handling migrates
into the coordinator, the small helper units around it shrink to pure
data or pure functions, and the one growing unit ends up owning every
decision a request can trigger.

## Normal evolution being modeled

The diff models the cumulative effect of that development pattern rather
than one deliberate rewrite:

1. **Per-user pipeline preparation.** Serving several users means each
   user needs a backing file system, a lock namespace shaped like their
   mounts, and a `webdav.Handler` wired to both. That preparation is
   construction-time, but it is per-user, so it lives with the code that
   holds the user table — free helpers become coordinator methods
   (`newUser`, `newUserFileSystem`, `newUserWebdavHandler`).
2. **Request lifecycle steps.** Steps that every request needs — resolve
   the user, parse the request, authorize it, log it — were free
   functions or inline blocks reading what they needed from arguments.
   As methods they read the coordinator's own fields instead, and pick
   up the naming convention of their new owner.
3. **Protocol features that extend the request lifecycle.** OPTIONS
   capabilities and partial updates had been staged on the per-user
   record because that is where the file system and lock system were
   reachable. Keeping them next to the remaining pipeline steps puts
   every reachable procedure behind one receiver.
4. **Cross-cutting wiring.** Request logging and remote IP resolution
   depend on a server-wide flag and a logger, both of which construction
   already holds.

## Overall design

The end state is a coordinator where the type doc comment reads as a
tour of the whole pipeline, and each stage is a method with its behavior
carried over verbatim. The guiding shapes:

- **State is read, not passed.** `parseRequest` reads the per-user prefix
  from the coordinator's own default user record instead of receiving
  it; `requestLogger` and `realRemoteIP` consult the proxy flag there.
  This is what makes the stages feel native to their new owner.
- **Behavior as a field where a helper must keep its own unit.** The
  filtered listing wrapper stays a `webdav.FileSystem` — it must, since
  the backing handler consumes it — but now receives normalization from
  the coordinator at construction time
  (`permissionsFS.normalizePath`), which mirrors how the lock system
  wrapper already takes a resolve function.
- **The entry point becomes an orchestrator.** `ServeHTTP` now reads as
  a list of coordinator calls: authenticate, parse, authorize, authorize
  the subtree, dispatch — including the OPTIONS and partial update arms
  that were previously reached through the default user record.
- **Pure policy stays pure behind the coordinator.** Range parsing, If
  header parsing, entity tag matching and body spooling remain free
  helpers; what moved is the concern of serving a request, not the
  byte-level policy.

## Per-location record

### `lib/handler.go` — the coordinator (grew from 255 to 676 lines)

**Type and entry point.** `Handler` documents itself as owning the whole
life of a request, and `ServeHTTP` was rewritten to call the coordinator's
own stages (`authenticateRequest`, `parseRequest`, `authorizeRequest`,
`authorizeSubtree`, `serveOptions`, `servePartialUpdate`) before handing
the request to the per-user WebDAV handler. Production role: this is the
HTTP-facing entry point `cmd` wires to `http.ListenAndServe`; its shape
is what makes every request-visible behavior readable in one place.

**Per-user preparation.** `newUser` (née `newHandlerUser`) builds the
per-user record: the unfiltered file system selected by
`newUserFileSystem` (née `buildFileSystem`), the lock namespace chosen by
`newUserWebdavHandler` (née `buildWebdavHandler`) matching how that
user's files are mounted, and the filtered wrapper that listings go
through — handed the coordinator's normalization. `NewHandler` stays the
sole construction surface for `cmd`; it now also closes over a
per-request logger built by the coordinator. Production role: everything
a served user depends on is prepared here once, at startup.

**Credential admission.** The Basic block left `ServeHTTP` and became
`authenticateRequest`: it resolves which user serves the request, writes
its own failure responses, and logs invalid usernames, invalid passwords
and successes. Production role: the gate through which every request
acquires an identity; lifting it out of the entry point kept that
sequence visible while letting the entry point shrink to orchestration.

**Request parsing.** The `request` record and its parsing moved in from
`lib/request.go`, which is gone. `parseRequest` (née `newRequest`)
converts the HTTP request to the internal record, stripping the prefix
both paths were served under; now it reads that prefix from the
coordinator's default user rather than receiving it. `cleanRequestPath`
(née `cleanPath`) resolves dot segments the way the backing file system
will, preserving the collection-marking trailing slash; this
normalization is exactly what the listing wrapper is later handed.
Production role: authorization and the file system must see the same
path — the same values parsed here decide every later stage.

**Authorization.** `authorizeRequest` composes the rule resolution
(`allowedAt`) with the per-method permission predicates, checking the
destination first for COPY and MOVE. `authorizeSubtree` walks the
unwrapped file system of the per-user record and consults the rules for
every descendant when MOVE or DELETE acts on a collection in one call.
Production role: the security decision for the request and its subtree;
keeping both steps as coordinator methods puts the whole authorization
story where the request's identity already lives.

**Protocol serving.** `serveOptions` (née `handleOptions`) advertises
capabilities per resource; `servePartialUpdate` (née
`handlePartialUpdate`) applies a byte-range PATCH or a ranged PUT;
`confirmRequestLocks` (née `confirmPartialUpdateLocks`) mirrors the
upstream lock confirmation so partial updates honor WebDAV locks, and
`checkPartialUpdatePreconditions` negotiates If-Match and If-None-Match
against the resource's entity tag. All four previously lived on the
per-user record; they read the same `user` they are now handed. Their
byte-level helpers — range, If header and content type parsing, entity
tag matching, bounded body spooling — stayed in `lib/partial_update.go`
as pure functions. Production role: the DAV-class protocol surface;
moving the serving concern onto the request pipeline keeps every
write-class request path on one owner while the policy stays testable on
its own.

**Logging and remote IP.** `requestLogger` (née `getRequestLogger`) and
`realRemoteIP` (née `getRealRemoteIP`) read the proxy flag from the
coordinator. Production role: the per-request access trail operators
grep for; the coordinator is where the server-wide proxy setting lives.

### `lib/permissions.go` — rule model, one method lighter

The change removes `UserPermissions.Allowed`, the method that resolved
the rules and applied the per-method predicate to the winning
permission set. Its resolution core (`allowedAt`,
with the rule walk and collection handling) stays on the rule model;
the composition of resolution with per-method predicates moved into the
coordinator, which now writes those predicates inline where the request
is authorized — the same `p.Allowed(req, fileExists)` and
`p.AllowedDestination(req, fileExists)` calls, made at the call site.
Production role unchanged: the rule model is the tested heart of the
permission system; the per-request decision now reads as pipeline logic.

### `lib/permissions_fs.go` — filtering wrapper, normalization incoming

`permissionsFS` loses `allowedThroughout` — the subtree decision moved
onto the coordinator, which walks the record's unwrapped file system
directly — and gains a `normalizePath func(string) string` field. Its
`OpenFile` normalizes the name it serves with the very normalization the
request was parsed with, received at construction time. The struct's doc
comment explains why the field exists. Production role: the wrapper is
what makes `PROPFIND` and `COPY` listings hide unreadable entries,
including mount roots; casting its name into the request's shape is what
keeps rules and listings in agreement.

### `lib/partial_update.go` — policy stays, serving leaves

The four per-request methods moved out (see above); `net/url` and `time`
imports went with them. What remains is the protocol's stable policy:
the content type contract, update-range and Content-Range parsing, the
If-header parser kept in sync with upstream, entity tag matching and
bounded body spooling — all free functions with their behavior and tests
untouched. Production role: the byte-level half of the partial update
protocol, shared by the coordinator's serving methods and reusable
independently.

### `lib/request.go` — removed

Its record and its two helpers moved into the coordinator; nothing
remains of the file.
