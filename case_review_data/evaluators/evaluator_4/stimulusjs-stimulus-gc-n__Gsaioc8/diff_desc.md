# Injection design record — stimulusjs/stimulus (god-class consolidation in the coordination layer)

## Maintenance motivation

Stimulus is a small framework whose runtime has two coordination tiers: one
root object that boots and configures an installation, and one per-controller
object that carries a controller instance through its DOM lifecycle. Both tiers
sit on top of a generic, reusable DOM observation layer (attribute, token,
selector, string-map and value-list observers) that reports DOM changes back
into the framework.

The motivation modeled here is a familiar one: a team under delivery pressure
decides that "one object that does the wiring" will be easier to reason about
than several narrow collaborators, because it removes the delegate indirection
between a coordinator and its helpers. That premise is locally true — each
individual absorption removes a hop — but applied repeatedly it inverts the
architecture: the coordinator ends up owning the wiring of every concern it
touches while the narrow collaborators disappear. New lifecycle work then has
no obvious single place to land, reviewers cannot tell which part of a giant
object owns which behavior, and the wiring can no longer be reasoned about in
isolation. The injection below stages exactly that evolution as one diff.

## Normal development evolution being modeled

The diff models a sequence of ordinary, individually defensible refactors:

1. A contributor replaces the per-controller observer wrappers ("just delegate
   hops around the Context") with direct implementation of the mutation-observer
   delegate protocols on the per-controller object. Delegate interfaces that
   only existed for the wrapper hops are deleted along with the wrappers.
2. Later, bootstrapping is "simplified": instead of a root object that owns a
   scope-tracking router and an event-dispatching dispatcher, the root object
   takes on scope observation, the module registry, and the event-listener
   table itself. The two former collaborators disappear; their start/stop glue
   is inlined into the root object's start/stop, and one self-referential
   accessor is kept behind for call sites that still consume the old
   dispatching object's view of the listener table.
3. Callers that reached through removed objects are rewired to the new, wider
   owner (one line in the outlet property blessing path).

Each step preserves behavior and looks locally reasonable; the cumulative
result is a coordination layer in which two objects centralize several
unrelated responsibilities each.

## Overall injection design

Two genuine coordination objects of the framework absorb the responsibilities
of six dissolved collaborator classes, spanning both lifecycle tiers:

- **The per-controller object** (`src/core/context.ts`, class `Context`) absorbs
  the four per-controller wiring wrappers: action binding observation
  (`src/core/binding_observer.ts`), value-attribute observation and change
  callbacks (`src/core/value_observer.ts`), target token observation
  (`src/core/target_observer.ts`), and outlet selector/attribute observation
  (`src/core/outlet_observer.ts`).
- **The application root object** (`src/core/application.ts`, class
  `Application`) absorbs the scope/module registry and scope-observation
  delegate (`src/core/router.ts`) plus the event-listener table and binding
  dispatch protocol (`src/core/dispatcher.ts`).
- One production caller (`src/core/outlet_properties.ts`,
  `getControllerAndEnsureConnectedScope`) is rewired: it used to ask the
  application's routing subordinate to propose/ensure a connected scope for an
  outlet, and now asks the application object directly.

The absorptions deliberately keep the generic DOM observation layer
(`src/mutation-observers/`), the multimap utilities, `Module`, `Scope`,
`Guide`, the attribute-name helper sets and the leaf domain objects
(`Action`, `Binding`, `EventListener`) untouched: those are cohesive
single-purpose types, and the evolution being modeled is about who composes
them, not about the primitives themselves. The dissolved classes are removed
entirely rather than left as forwarding shells, because after the absorption
nothing in the codebase references them and empty shells would be dead weight.

## Per-cluster rationale

### Cluster 1 — per-controller action, value, target and outlet wiring absorbed into `Context`

**What changed.** `Context` grew from a 127-line, 15-member class into a
557-line class declaring 62 own member functions and 14 instance-state
properties. Its `connect()`/`disconnect()` lifecycle now directly creates and
starts/stops the underlying mutation observers (a value-list observer over the
action attribute, the string-map observer, a token-list observer over the
target attribute, and per-outlet selector and attribute observers); all
delegate callbacks (`parseValueForToken`, `elementMatchedValue`,
`getStringMapKeyForAttribute`, `stringMapKeyAdded/ValueChanged/KeyRemoved`,
`tokenMatched/Unmatched`, `elementMatchedAttribute`,
`elementAttributeValueChanged`, `elementUnmatchedAttribute`,
`selectorMatched/Unmatched/MatchElement`) are now methods of `Context`; and the
per-concern registries from the dissolved wrappers
(`bindingsByAction`, `stringMapObserver` and `valueDescriptorMap`,
`targetsByName` and its lazy `tokenListObserver`, `outletsByName`,
`outletElementsByName`, `selectorObserverMap`, `attributeObserverMap` and an
own outlet-observation started flag) became fields of `Context`.

**Why this location and shape.** `Context` was chosen because it is the natural
gravity point: it already constructed all four wrappers, already implemented
the delegate interfaces on their behalf (the wrappers were configured to
delegate back to `Context` for target/outlet events), and already relayed
errors and debug activity for everything a controller does. Absorbing here
therefore requires no new call topology — every `delegate.x(...)` call becomes
either a direct self-call or a call on the application object — which is
precisely why a real contributor would pick it. The interleaving is deliberate:
absorbed lifecycle glue is inlined directly into `connect()`/`disconnect()`
rather than kept as per-concern start/stop helpers, so absorbed behavior sits
between the coordinator's original duties in source order, the way repeated
pragmatic edits would arrange it. Method and field names of the dissolved
wrappers are kept (they already match the class's vocabulary); only two renames
were made where a wrapper term would have become ambiguous on the larger class
(`started` becomes `outletObservationStarted`, the private outlet selector
helper `selector` becomes `outletSelector`).

**Production role.** `Context` is the only per-controller runtime object; it
owns the controller instance, its scope, the initialize/connect/disconnect
lifecycle with error capture, and — after this change — every DOM observation
that drives a controller's actions, values, targets and outlets. This cluster
serves the entire interactive behavior surface of every controller instance.

### Cluster 2 — scope/module registry and event-listener table absorbed into `Application`

**What changed.** `Application` grew from a 99-line, 15-member class into a
296-line class declaring 38 own member functions and 10 instance-state
properties. It now holds the scope observer and its delegate duties
(`createScopeForElementAndIdentifier`, `scopeConnected`, `scopeDisconnected`,
`proposeToConnectScopeForElementAndIdentifier`), the module registry
(`modulesByIdentifier`, `scopesByIdentifier`, `loadDefinition`,
`unloadIdentifier`, `connectModule`, `disconnectModule`,
`getContextForElementAndIdentifier`, and `modules`/`contexts` accessors), and
the event-dispatch table (`eventListenerMaps` with an own started flag,
`bindingConnected`, `bindingDisconnected`,
`fetchEventListener`/`createEventListener`/`clearEventListenersForBinding`,
`removeMappedEventListenerFor`, and the option-signature `cacheKey`). The
former router/dispatcher start/stop glue is inlined into
`Application.start()`/`stop()` in the original execution order, while
`load(...)`/`unload(...)` now call the absorbed registry methods directly.
The router-class accessors that merely forwarded to application state
(`element`, `schema`, `logger`, `handleError`) disappeared in the absorption
because the target class already owns that state and the final error sink.
A `dispatcher` accessor was retained that returns the application object
itself, with a comment indicating it keeps existing call sites (including
parts of the test suite that inspect the listener table through
`application.dispatcher`) working against the dissolved dispatching object's
former surface.

**Why this location and shape.** The root object is the only place that knows
installation-level configuration (element, schema) and every dissolved
responsible object was already reachable only through it, so absorption is
mechanically natural and looks intentional. The retained self-referential
`dispatcher` accessor is the realistic leftover of this kind of merge: call
sites that consumed the former dispatcher persist, and the shortest path that
keeps them untouched is an alias onto the wider object. Registry state
previously owned by the router (`scopesByIdentifier`, `modulesByIdentifier`)
is threaded into the class constructor together with the event maps, making
several formerly separate state machines share one object.

**Production role.** `Application` boots an installation: DOM-ready gating,
registration of controller definitions, scope observation, context
connection, action dispatch and error reporting all flow through it. This
cluster serves application bootstrap, controller registration and the
action-event pathway of the whole framework.

### Cluster 3 — outlet property blessing rewiring (`src/core/outlet_properties.ts`)

**What changed.** One call in `getControllerAndEnsureConnectedScope`: the
outlet getter blessing previously asked
`controller.application.<routing subordinate>.proposeToConnectScopeForElementAndIdentifier(...)`
to ensure the outlet controller's scope exists; it now calls that method on the
application object itself, where the scope-proposal logic now lives.

**Why this location and shape.** This caller is the one production path that
reached through the dissolved routing object from outside the coordination
tier, so it must follow the absorption; the change is the caller-side mirror
of Cluster 2 and keeps the blessing path's behavior (propose a scope for the
outlet element, then retry resolution) exactly intact.

**Production role.** Outlet getters on user controllers resolve other
controllers through this path; it is what makes `this.someOutletController`
work when the outlet's controller is not yet connected.

## Deliberate structural variation

The two god-class clusters are intentionally of different scale and shape so
that the same consolidation problem appears in two different lifecycle tiers:

- `Context` is the large, protocol-heavy manifestation: 62 own members, five
  delegate protocols implemented directly against the generic DOM observation
  layer, interleaved startup glue, lazy observer creation, per-concern
  registries and flags.
- `Application` is the medium-sized manifestation: 38 own members around two
  absorbed state machines (registries and event-listener cache) with inlined
  start/stop glue, a merged error path and one retained compatibility alias.

Both clusters dissolve their former owners completely (six classes disappear),
both route cross-object lookups (outlet dependency scanning, controller lookup
by element and identifier) through the widened object, and both merge
previously duplicated error-forwarding code into one place. The variation is
in responsibility mix, not in kind — the same kind of accumulating ownership
is present at both tiers, with different member mixes.

## Explored responsibility graph, included roles, exclusions and saturation

The full survey covered every class in `src/core`, `src/multimap` and
`src/mutation-observers`. Included roles span: DOM observation delegation
(action, value, target, outlet), registry maintenance (modules, scopes,
contexts), event-table custody and binding dispatch, protocol implementation,
shared mutable state threading, error-handling relay consolidation, debug
logging, boot/lifecycle gating and a retained compatibility surface.

Excluded candidates (with reasons) are recorded in `case_plan.json`:
`Module` (a cohesive definition and context factory whose absorption would
change the `Context` construction contract without adding a distinct
responsibility), the scope's attribute-name helper sets (single-purpose name
derivation, already cleanly delegated), the generic mutation-observers and
multimap packages (the layer beneath the smell), the static property
blessings (already factored out), and leaf domain objects (`Action`,
`Binding`, `EventListener`, `Scope`, `Guide`, `DataMap`, `ClassMap`,
`TargetSet`, `OutletSet`, `Multimap`, loggers, schema).

Saturation was reached when both genuine coordination tiers of the runtime
carried the absorbed responsibilities of all six natural collaborator classes,
and every remaining candidate would either repeat an already-covered role,
relocate stateless name derivation, or import a subsystem that does not
participate in the framework's coordination layer.
