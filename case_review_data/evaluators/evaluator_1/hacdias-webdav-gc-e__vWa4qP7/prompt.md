# Re-home the request pipeline stages in the webdav server

This repository is a WebDAV server written in Go: a small command wires
a configuration into the server, and the core library package serves
WebDAV requests — files and collections over HTTP, with users,
per-user directories or mounts, per-method permission rules, WebDAV
locking, and a sabredav-style partial update protocol.

## What we're seeing

The request-serving code has, over several rounds of feature work,
become the home of every stage in a request's life. Credentials are
admitted there, the request-target and destination headers are parsed
and normalized there, per-method rule evaluation and whole-subtree
authorization are performed there, lock admission happens there, the
OPTIONS capabilities response and the partial update protocol are
served there, per-user file systems and lock namespaces are composed
there, and the per-request access log lines and proxy-aware client IPs
are produced there.

The supporting units that used to own parts of that work no longer do:
the internal request record and its parsing helpers have lost a home of
their own; the rule model no longer answers request-shaped questions,
so per-request authorization is now composed — and partly re-implemented
— at the call site; the filtered directory-listing wrapper no longer
carries out subtree authorization and no longer normalizes the names it
serves on its own terms; the protocol stages have moved away from their
pure policy helpers. Every pipeline concern is reached through the same
shared state, reads the same fields, and is tested through the same
entry point.

The consequences we care about operationally:

- a change to any one concern — an authentication detail, a header
  normalization rule, a lock-admission edge case, a logging field —
  lands in request-serving code that also carries all the others, so
  diffs are hard to review and easy to get subtly wrong;
- the same path normalization logic exists in request-serving code and
  is handed to the listing wrapper as a callback, so the two layers now
  agree only as long as that coupling is remembered;
- concerns that were independently testable in isolation are no longer;
  exercising one pipeline stage drags in the rest.

## Scope

The core library's request pipeline and the units it composes. In
particular, these responsibility areas need cohesive homes again:

- **credential admission** — resolving which configured user a request
  is served for, rejecting and logging bad credentials;
- **request parsing and normalization** — the internal request record,
  prefix stripping, dot-segment resolution, and the distinction between
  a collection and a resource inside it;
- **request and subtree authorization** — per-method rule evaluation,
  destination handling for COPY and MOVE, and the descendants a single
  MOVE or DELETE call acts on;
- **per-user pipeline preparation** — the per-user backing file system,
  lock namespace, and WebDAV handler a request is dispatched through;
- **protocol stages** — OPTIONS capabilities and the partial update
  protocol (ranges, content type, preconditions, lock confirmation);
- **request logging and remote IP** — per-request log lines and the
  proxy-aware client address;
- **directory-listing filtering** — rejecting entries the user cannot
  read at listing time, including mount roots;
- the **pure partial update policy helpers** that the protocol stages
  rely on.

Out of scope: configuration parsing and defaults, the command-line
wiring, case folding, and the underlying mount/file system machinery —
they are cohesive already.

## What we need

Re-establish distinct, cohesive owners for these responsibilities, and
wire the pipeline back together as composition rather than
accumulation. Concretely:

1. Identify the distinct responsibilities currently carried by
   request-serving code, and give each a home of its own — a type or a
   set of functions with a single purpose and a small surface. More than
   one decomposition can be a good one; you decide the units and their
   names.
2. Parsing owns its product: whatever consumes the parsed request —
   authorization, protocol stages, dispatch — should receive normalized
   values rather than recompute or re-normalize them, and the
   normalization should not need to be threaded to consumers as a
   callback.
3. Permission decisions should be asked of the rule model, not
   re-implemented beside it: per-request and per-subtree checks should
   compose the model's resolution instead of duplicating its walk.
4. The listing wrapper should stand on its own in enforcing read rules,
   and its view of names should agree with the pipeline's by
   construction.
5. Protocol stages should live with their policy helpers, scheduled from
   the pipeline, not inside the per-user record or the request-serving
   orchestrator.
6. Whatever request-serving entry point remains should orchestrate:
   authenticate, parse, authorize, dispatch, log — as calls to the
   cohesive units, not as ownership of each step.

## Behavior that must not change

Everything observable to the HTTP test suite and its helpers must stay
identical:

- basic authentication including its realm, its 401 bodies, and the
  no-users and no-password serving modes, including their warnings at
  startup;
- per-method rule evaluation, including rules written for collections
  (the trailing-slash sense) and how they restrict a collection itself,
  permission checks against COPY and MOVE destinations, and refusals of
  whole-subtree MOVE and DELETE operations when a descendant's rule
  denies them;
- OPTIONS responses with their Allow, DAV, MS-Author-Via and
  Accept-Patch headers per resource kind;
- partial updates: content-type and X-Update-Range / Content-Range
  semantics, lock admission and confirmation, If-Match and
  If-None-Match preconditions, status codes, and created/no-content
  outcomes;
- GET and HEAD on collections being served as PROPFIND with a default
  depth, and HEAD responses carrying no body;
- directory listings hiding entries the user cannot read, on every read
  listing path;
- per-request access log fields and the proxy-aware remote address
  behavior behind an explicit flag.

## Constraints

- The package's exported construction surface that the command wiring
  uses must remain source-compatible: no changes should be needed in the
  command package.
- The existing tests must pass unmodified: do not edit, weaken, or
  delete test files, and do not add version-gated or environment-gated
  expectations.
- No new third-party dependencies; the standard library and the
  existing dependency set only.
- Build and test with:
  `GOTOOLCHAIN=auto go build ./...` and `GOTOOLCHAIN=auto go test ./...`.

## Definition of done

- Both commands above pass.
- Each responsibility area listed in the scope has a cohesive owner with
  a single purpose, and request-serving code orchestrates the pipeline
  instead of implementing each stage.
- Nothing observable by the test suite has changed.
