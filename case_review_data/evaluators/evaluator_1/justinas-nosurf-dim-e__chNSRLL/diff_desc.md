# Injection design record — nosurf deeply-inlined request/token pipeline

Repository: `github.com/justinas/nosurf` at commit `ec9bb77` (CSRF middleware for
Go's `net/http`). Assigned smell: **deeply_inlined_method** — "a method whose
sub-method implementations are copied into itself… Depth 3 inlining makes the
method exponentially hard to understand and refactor."

## Maintenance motivation

The library routes every request through a layered call tree: the pipeline
method (`ServeHTTP`) delegates to per-concern steps (token bootstrap, exemption
routing, origin adjudication, credential extraction and verification), the
public issuance method (`RegenerateToken`) delegates to the mint/publication/cookie
steps, and the public string-form verification helper (`VerifyToken`) delegates to
the token primitives. Each leaf is small and named after one idea.

A maintainer chasing a load-generation report becomes convinced that the
middleware's per-request call chain — five layers deep in the worst path —
costs measurable overhead on the hot path and in front-line APIs. They respond
the way hot-path folklore recommends: stop "bouncing between layers" and
squeeze each whole chain into the entry function, manually unrolling every
 callee below it. The benchmarks never showed a win, but the flattened code
passed the suite byte-for-byte in behavior and shipped. Six months later the
pipeline is a wall of inline arithmetic: the pieces are all still on the shelf
as named functions (`generateToken`, `maskToken`, `unmaskToken`, `oneTimePad`,
`b64encode`, `b64decode`, `tokensEqual`, `sameOrigin`, `sContains`,
`setTokenCookie`, …), but no request-flow owner calls them anymore — each
entry point re-derives their bodies locally, twice or three times over, with
different variable names each time.

## Normal development evolution being modeled

This is the ordinary lifecycle of a defensive-performance refactoring that
missed review:

1. A deadline-driven "cleanup" flattens the deepest chain in the per-request
   path (`ServeHTTP`), absorbing the unexported steps only it consumed.
2. The same recipe is then applied by habit to the two other call-chain owners
   an integrator would profile (`RegenerateToken`, `VerifyToken`), each copied
   by hand rather than shared, so the collaborating entry points slowly diverge.
3. The originals of the small primitives survive untouched — the tests call
   them directly and the public API surface depends on them — so the codebase
   permanently carries both the decomposed form and its inline shadows.

## Overall design

The transformation is behavior-preserving and confined to production control
flow: two files change, `handler.go` and `token.go`, while `crypto.go`,
`utils.go`, `exempt.go` and `context.go` are left as the untouched shelf of
named primitives. Three entry points are inflated into monoliths whose bodies
re-derive the primitives inline; the unexported pipeline/issuance steps that
only those entry points consume (`extractToken`, `ensureSameOrigin`,
`checkOrigin`, `checkReferer`, `setTokenCookie`) are deleted along the way, so
the compact decomposition they embodied no longer exists in the tree. What
survives is: every exported symbol and constant, every primitive the tests
import, the context plumbing on the `go1.7`/`!go1.7` build-tag seam, and the
58-case behavioral ledger of the suite.

Deliberate structural variation, applied per cluster: the inlined code is not
a paste of the original bodies. Variable names diverge per site (`fresh`,
`seed`, `prospective`, `submitted`, `pad`, `hidden`, `outCookie`…), loop idioms
differ (`for i := range` versus the original index loops), the safe-method and
exemption decisions are folded into flag variables (`safeVerb`, `exempt`)
rather than helper results, the origin/Referer ladder becomes guard-clause
blocks with staged error variables rather than returned sentinels, and
phase-styled comments (`intake`, `mint/bootstrap`, `admission routing`,
`origin adjudication`, `credential comparison`, `publication`, `distribution`)
suggest a pipeline slicing that does not line up with the former method
boundaries.

## Cluster-by-cluster rationale

### Cluster 1 — `CSRFHandler.ServeHTTP` (`handler.go`)

**What changed.** The 50-line pipeline method grew into a ~135-line monolith
that performs, in one body: bearer-cookie lookup with the cookie-name default
re-derived inline; raw-token decode via `base64.StdEncoding` inline; the
valid-token bootstrap (masking + context publication) and the regeneration
branch (fresh entropy via `io.ReadFull(rand.Reader, …)`, cookie construction,
`http.SetCookie`) fully unrolled; the safe-verb membership scan and the whole
exemption cascade (custom func, exact paths, globs, compiled regexps) written
out as inline loops; the `Sec-Fetch-Site`/`Origin`/`Referer` adjudication with
inline `scheme`/`host` comparisons and allowed-origin escapes; credential
extraction (header, then parsed form, then multipart) inline; and the
credential verdict itself — a `tokenLength`-split XOR fold of the submitted
mask followed by a `subtle.ConstantTimeCompare` — inline. The unexported
steps it alone consumed (`extractToken`, `ensureSameOrigin`, `checkOrigin`,
`checkReferer`) and the shared cookie step (`setTokenCookie`) were then
deleted from the tree.

**Why this location and shape.** `ServeHTTP` is the root of the deepest chain
in the package (`ServeHTTP -> RegenerateToken -> setTokenCookie -> ctxSetToken
-> maskToken -> oneTimePad`, five call layers crossing five files), which is
precisely the chain a hot-path complaint names. Absorbing the whole chain in
one body at depth 5 is what makes this instance "deeply" inlined rather than
merely long. The deleted middle steps are exactly the ones with a single
consumer — deleting them is what an uncritical flattening does, and it is
what makes the monolith the only remaining home of that control flow. The
order of phases mirrors the original request flow exactly (intake → bootstrap
→ admission → origin → comparison → dispatch) so every observable decision,
short-circuit order, sentinel reason and side-effect sequence in the ledger
is preserved.

**Production role.** This is the middleware knot an integrator's `http mux`
calls for every request: it decides cookie issuance, admission, origin trust
and token validity, and it is the only code that runs on the critical path
for every request.

### Cluster 2 — `CSRFHandler.RegenerateToken` (`handler.go`)

**What changed.** The public re-issuance method stops delegating: the mint
(`io.ReadFull(rand.Reader, seed)`), the masked-form publication via the
context seam, the cookie name default and the raw-form `base64` cookie value,
and the `http.SetCookie` emission are all written out in its body, with its
own variable names (`seed`, `cookie`) shaped differently from cluster 1's
(`fresh`, `outCookie`).

**Why this location and shape.** This is the second call-chain owner of the
same issuance recipe, and the one visible to integrators outside the request
loop. Duplicating the unrolling here — independently rather than by sharing —
models the honest consequence of hand-inlining: the same recipe now exists as
several locally-diverging copies plus the untouched named primitives, and an
integrator profiling "token refresh costs" would have been told to flatten
exactly this method. Writing the copy out with different names and comments
keeps the two copies from being textually identical (and from being
mechanically collapsible by any single find-and-replace). The context
publication is deliberately kept behind the build-tag seam helper rather than
inlined, so the `!go1.7` variant of the package keeps compiling.

**Production role.** Session-refresh API: mint a token mid-session, publish
the presentation form for templates, deliver the raw form via cookie, return
the form-value token to the caller.

### Cluster 3 — `VerifyToken` (`token.go`)

**What changed.** The exported string-form verification never leaves its own
frame: both operands are decoded with `base64.StdEncoding` inline, a masked
operand is folded back to its raw shape with a `tokenLength`-split XOR loop
(the `unmaskToken`→`oneTimePad` recipe re-derived in place, applied
symmetrically to either operand), and the verdict is reached with length
checks plus `subtle.ConstantTimeCompare` — the `tokensEqual` recipe inline.
The byte-form sibling `verifyToken` and the underlying primitives are left
untouched.

**Why this location and shape.** `VerifyToken` is the leaf-most independent
chain owner (verify → unmask → pad, and verify → compare), spanning two
files (`token.go`, `crypto.go`). It is the API integrators call when they
verify tokens outside the middleware loop, so it complements the two
request-flow clusters with a different execution path and caller basis. Its
copy is deliberately asymmetrical in naming and control flow from cluster 1's
credential block (`prospective`/`submitted` versus `realToken`/`sent`,
separate guard folds per operand versus one combined gate), so recognizing
that both sites implement the same recipe requires reading, not diffing.
Keeping `verifyToken`, `tokensEqual`, `unmaskToken` and `oneTimePad`
themselves intact is what leaves the "original methods still existing
elsewhere" that the smell definition presupposes.

**Production role.** Standalone trust decision over string operands:
integrators use it to validate form- or header-supplied tokens themselves.

### What was deliberately not touched

- The primitives (`crypto.go`, `utils.go`, `token.go` primitives,
  `exempt.go`): the suite imports them directly and the smell's defining
  property is that the owners still exist while the monoliths re-derive them.
- `IsExempt` and the exemption machinery: publicly owned query API whose
  callee layer is flat; its matcher cascade still participates as an inlined
  fragment inside cluster 1.
- The context helpers `ctxSetToken`/`ctxSetReason`/`addNosurfContext`/
  `ctxClear`: these sit on the `go1.7`/`!go1.7` build-tag seam; inlining their
  bodies into portable code would break the legacy context implementation, so
  even a careless flattening spares them.
- `handleSuccess`/`handleFailure`, the setters, `New`/`NewPure`,
  `defaultFailureHandler`, `StaticOrigins`: one-line indirections and factory
  code with no multi-level chain to absorb.
- All tests, the `!go1.7` legacy files, docs and module metadata.

## Saturation note

After the three chain-owners were inflated, no further site in this repository
offers the same relation without restating an already-covered role: the
remaining functions are leaf primitives, build-tag seams, trivial dispatch
points, or unexported steps already absorbed into the monoliths.
