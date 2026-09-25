# Injection design record — jarcoal-httpmock-de-e

Repository: `github.com/jarcoal/httpmock` @ `dbfc29e2b697587dc5fbbbdaa34a3713826a8048`
Assigned smell type: `dead_code_elimination`

This file records the design rationale of `smell.diff`: the maintenance
situation it models, why each location took the shape it did, and the
role each addition plays inside the library. It is an auditable record
of generation decisions, written after the fact; it does not predict or
prescribe any particular outcome or cleanup.

## 1. Realistic maintenance motivation

httpmock routes incoming requests through a deliberately **tolerant**
fallback ladder in `MockTransport.findResponders`: it tries the full
registered URL first, then the URL with sorted query parameters, then
the URL with the query string stripped, then the raw path alone, and a
registration error reports the closest known routes. This tolerance is
what most users want from a mocking library, but it is also the source
of a long-standing class of user friction: a test registers
`/foo/bar?a=1` and the transport happily matches `/foo/bar`, masking a
real bug in the client under test.

The natural maintenance response, seen in many HTTP libraries, is an
opt-in **strict mode**: match only the exact URL the request carries,
never repair query parameters, never fall back to the path, and refuse
ambiguous registrations instead of silently tolerating them. Designing
one touches every core responsibility of the package at once: routing
(transport), matching policy (matchers), bookkeeping (stats/telemetry),
activation (environment switches) and keying (the internal route key).

`smell.diff` models the moment such a project was **drafted but parked**:
the strict engine exists, is wired, and is disabled — and disabled not by
a runtime condition anybody can flip in a supported build, but by a
compile-time decision that can never execute.

## 2. The development evolution being modeled

The additions follow the lifecycle a strict-matching rollout would
realistically leave behind, phase by phase:

1. **Activation is drafted first as an environment knob.** The package
   already owns this idiom: `env.go` defines `envVarName` (`GONOMOCKS`)
   and consults it from `Disabled()`. A strict-mode rollout would start
   the same way — a variable name plus a predicate consulting it — with
   the shape copied from the existing pattern.
2. **The team abandons the env knob mid-rollout.** Deciding an
   environment variable is too dangerous for behavior this drastic (a
   stray variable in CI would silently change every test's matching
   semantics), activation moves behind a compile-time constant in the
   routing core so a production build simply cannot opt in accidentally.
   The env knob is left behind next to the live one.
3. **The engine itself is built beside the live one.** A strict variant
   of the routing walk mirrors the transport's own structure: the
   existing `findForKey` slice of method values — the package's native
   registry idiom — is reproduced as a parallel strict registry, and the
   strict walk iterates it with no tolerant fallback of any kind.
4. **Rollout instrumentation is added to compare engines.** Before
   enabling such a change one would measure it: the strict walk records
   every hit so its results can be diffed against the tolerant ladder
   during a trial. The collector lives with the other responder
   bookkeeping helpers.
5. **Matching policy gets a strict sibling.** The tolerant matcher
   combinators ignore `nil` matchers (`matcherFuncAnd` skips
   them); strictness demands the opposite semantics, so a combination
   helper and a body-matching helper enforcing "every registered
   condition is meaningful" are drafted next to the tolerant ones.
6. **Keying is prepared across the package boundary.** Internal route
   keys carry the method and URL verbatim; strictness also needs a
   canonical form (upper-cased method, fragment stripped) so that
   registrations differing only by case or fragment collide on purpose.
   A wrapper type is added in `internal/` beside `RouteKey` itself.
7. **The rollout is parked.** The compile-time gate is left at the
   disabled value pending benchmarking against the tolerant fallbacks,
   the release ships, the issue goes quiet — and the whole scaffolding
   stays exactly as the diff shows it.

## 3. Overall injection design

The diff is **purely additive** across five production files
(`transport.go`, `match.go`, `response.go`, `env.go`,
`internal/route_key.go`, +208/−0 in 8 hunks). Nothing is renamed,
deleted or re-tuned: every observable behavior of the library — every
exported symbol, the tolerant fallback ladder, the `GONOMOCKS` switch,
the whole test suite — is byte-for-byte what it was at the pinned
commit. The additions vary structurally **on purpose**, because the
roles they serve in the parked rollout differ:

| shape | role in the rollout story |
| --- | --- |
| compile-time `const` gate | release-safety switch for the strict engine |
| guarded branch inside a live method | the routing handover point to the strict walk |
| package-level slice of method values | the registry idiom the transport already uses for its finder strategies |
| struct with mutex + methods | per-hit instrumentation of the strict engine |
| variable initializer chain (var → constructor → type → method call) | the wiring that keeps the collector reachable-looking |
| env var name + predicate pair | superseded activation knob, drafted first and abandoned second |
| embedded wrapper type + method + converter in `internal/` | canonical keying for strict collisions, across the package boundary |

That variety is the point of choosing this scenario for a
dead-code case: whether any of these declarations or branches can
execute under supported configurations is precisely the analysis a
maintainer must perform — it is not answered by reading any single
call site.

## 4. Changed locations and per-cluster rationale

### Cluster A — routing core, `transport.go` (+80)

- `strictMatchingMode` — a compile-time gate mirroring how real
  projects park risky engines (the Go compiler then proof-eliminates
  the disabled path in every supported build). It sits with the other
  transport-level declarations so the reader sees it as the engine's
  master switch. It replaces the environment activation drafted in
  Cluster D — the story's mid-course correction.
  Production role: the configuration constant under which the strict
  engine was to be enabled. As written there is a draft migration
  decision encoded here, and the surrounding comments say so.
- A branch in `MockTransport.findResponders` testing that constant —
  the handover from tolerant to strict routing. It was given a first
  frame at the entrance of the walk, before any fallback is attempted,
  because that is where a strict engine must take over to make its
  guarantees meaningful. The base method is left untouched otherwise:
  the ladder below the gate keeps its exact semantics.
  Production role: the alternate dispatch path of the live routing
  entry point — the single point from which the whole strict engine
  hangs.
- `findRespondersStrict` — the strict walk counterpart of the live
  method: build the key from the request's verbatim URL and consult the
  strict registries in order, recording each hit for the rollout
  comparison, with no sorted-query, stripped-query or path-only retry.
  Production role: the engine's dispatcher. Maintaining the live
  method's naming shape (`findResponders` vs `findRespondersStrict`)
  keeps the mirroring obvious to a reader.
- `exactStrictResponders`, `strictRegexpResponders` — the two lookup
  strategies of the strict engine, strict siblings of the live
  `respondersForKey` and `regexpRespondersForKey`. The exact variant
  reads the responder map under the read lock; the regexp variant scans
  compiled patterns, keeping only requests whose raw URL matches
  verbatim. They are appended after the live strategies so both idioms
  sit side by side, and they deliberately reuse the transport's own
  locking conventions rather than inventing new ones.
- `strictRouteFinders` — the package-level slice of method values
  gathering those two strategies in consultation order. The transport
  already uses exactly this idiom for its tolerant strategies
  (`findForKey`); a strict engine written by the same maintainers
  would reproduce it rather than invent a new dispatch mechanism.
  Production role: strategy registry, consultable in order, exactly
  one pointer away from being live.
- `strictRouteStats` — the transport's handle on the instrumentation
  collector of Cluster C. It is a package-level variable initialized
  with the collector constructor, sitting beside the registry it feeds.
  Production role: rollout telemetry destination for the strict walk.

The deliberate variation inside one file: a constant, a guarded branch
in an otherwise live method, a registry of method values binding three
methods, and a state variable binding a cross-file cluster — four
structurally different forms of the same "wired but driveable only from
a disabled path" idea.

### Cluster B — matching policy, `match.go` (+44)

- `matcherFuncAll` — strict sibling of the live `matcherFuncAnd`.
  The tolerant combinator quietly skips `nil` matchers (a backdrop
  that makes registrations succeed even when a condition is ambiguous);
  a strict engine requires the opposite: one `nil` matcher and the
  whole combination fails. It was placed directly under the tolerant
  sibling so the policy contrast is legible.
- `strictRequestBodyMatcher` — the strict condition builder: the
  request body must be readable and non-empty, and every condition in
  the combination must hold, a `nil` one forbidding the route. It uses
  the package's existing body discipline (`rearmBody`, `io.ReadAll`)
  exactly like the tolerant helpers do, on purpose: what would betray
  it as the strict engine's own is the failure mode it selects
  (forbid rather than skip), not new infrastructure.
  Production role: the strict engine's request-matching condition
  factory — the natural counterpart of what `BodyContainsBytes` and
  friends are to the tolerant engine.

### Cluster C — rollout instrumentation, `response.go` (+37)

- `routeStats` — a small hit-collector structure (a `sync.Mutex` over a
  label→count map plus a total), placed with the other bookkeeping
  helpers at the end of the file, where the responder bookkeeping
  utilities live. A rollout comparison needs somewhere to accumulate
  "route X matched N times through the strict engine".
- `newRouteStats` — the constructor used by the registry-side variable
  of Cluster A (`strictRouteStats = newRouteStats()`), the same
  constructor-with-state idiom the package uses elsewhere.
- `record` — the hit recorder invoked from the strict walk, keying on
  the responder route label.
- `snapshot` — an accessor reading out count-per-route (the totals map)
  for the comparison step; kept as the collector's read-out port even
  though nothing queries it yet.

The chain `var → constructor → type → record → snapshot` is the longest
reference dependency of the diff; it was placed in a different file
from its consumer and producer on purpose, because in the real story
telemetry helpers live with telemetry, not with the dispatcher.

### Cluster D — superseded activation knob, `env.go` (+14)

- `strictEnvVarName` — the name of the strict-mode environment
  variable, drafted as the first activation approach and left behind
  when activation moved to the compile-time constant.
- `strictModeEnabled` — the predicate consulting it, the exact shape of
  the live `Disabled()` predicate that consults `GONOMOCKS` beside it.

This is the smallest cluster and exactly one: it models the abandoned
draft decision, phase 1 of the rollout before the mid-course
correction to the compile-time gate. It was kept very deliberate
(minimum noise) but pointed in shape: two declarations, mirroring the
precedent that makes them believable as former production material.

### Cluster E — canonical route keys, `internal/route_key.go` (+33)

- `routeKeyV2` — an embedded wrapper of `RouteKey` carrying a
  canonicalization marker, designed to be the key of the strict
  registry. It is the internal package participating in the rollout
  because registration/lookup keys belong to it.
- `routeKeyV2.strictKey` — the normalizing method: upper-cased method,
  fragment stripped, so two registrations differing only by case or
  fragment collide on purpose under the strict engine.
- `canonicalRouteKey` — the converter a strict registration path would
  call; `strings` is the file's only new import, used by it.

Cross-package on purpose: the story walks to the key representation,
not merely its consumers. The name `routeKeyV2` (not `strictRouteKey`)
reads as a revision in the key family — the naming decision a real
author would have defended in review because unlike the transport-side
names it describes a data-model revision, not an engine.

## 5. Repository precedents grounding the story

Every cluster is downwind of an existing idiom of the pinned commit.
`findForKey` (transport) is the model of registry; `envVarName`→`Disabled()`
(env) is the model of the knob pair; `matcherFuncAnd` (match) is the
model of the policy sibling; bookkeeping helpers at the end of
`response.go`; and `RouteKey` itself is the model for the v2 key. A
reviewer familiar with httpmock reads the additions as native work,
because they borrow shapes the repository already owns. The feedback
that motivates this story is the one a mocking library actually
receives: users register `/foo/bar?a=1`, get matched on `/foo/bar`,
and file the issue asking for a way to match only what was registered.

## 6. Saturation and excluded candidates

The injection stopped where the repository's natural owners end. The
live options considered and rejected: unused exported API (unreachable
from the entry surface by definition, and it would apologize for
itself in review), dead helpers parked inside `_test.go` files (tests
are the supported behavior owners, not dead weight), replicating the
collector or registry (would repeat an already covered role or become
structural padding), weakening a live tolerant fallback (would change
observable behavior), sequestering everything in a brand-new dead file
(reads as a dump, not as parked work within an existing organization),
or activating the engine at runtime (changes behavior the suite pins
down). At that point each further insertion repeated a covered role
with no new structure: the responsibility graph had run out of
genuinely distinct owners.

## 7. Boundary and behavior notes

- The diff touches five production files and nothing else: no test
  file, no example, no exported declaration, no `go.mod` change, no
  data or fixture.
- All additions are self-wired — they reference each other and the
  existing API — and none changed a live line's behavior, since the
  one place the strict engine touches a live method is behind the
  gated branch.
- Whether every one of the additions shown here can in fact never
  execute in supported builds — separately, jointly, or only in part —
  is the determination this case asks its reviewers and maintainers
  to make from the code itself. This record deliberately does not
  make it for them.
