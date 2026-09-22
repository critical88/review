# Cleanup task: retire unreachable internal helpers before the 3.0.0 GA

## Repository

`wired-elements` — a hand-drawn-look web-component library on lit 2 and roughjs, with one
custom element per component module, a shared base element, and a shared module of
drawing helpers. The tree is on the 3.0.0 release-candidate line; the recent pre-GA
reworks include the slider's larger bar/knob geometry, the restated progress label and
fill handling, the fully slot-driven selection in the combo and listbox components, and
the spinner's retimed animation loop.

## Observation

While preparing the GA, we audited the library's internal surface and found leftover
helpers from those reworks: protected members and module-level functions whose execution
paths the reworks removed, but that were never deleted. Nothing that actually runs ever
reaches them anymore. A few of them look busy — some reference helpers on the shared base
element, some are private helpers called by other stranded members, and two module-level
geometry functions call only each other — but those references all live inside code that
is itself never invoked. Several of them carry "kept for compatibility" comments about
older event names, embedding hosts, or earlier layout passes.

## The responsibility rule

In a component library, an internal member or a non-exported module function earns its
place only if a real execution path reaches it: a lit/custom-element lifecycle entry
point (`render`, `updated`, `firstUpdated`, connect/disconnect, attribute changes), a
decorated property or query consumed by the framework, a live event listener or template
binding, a call chain rooted in one of those, or the exported public API. References that
exist only inside other unreachable helpers do not count. Public members and exported
symbols are potential API for downstream users and are not in scope.

Please apply that criterion with the whole library in view, not file by file: the
plausible leftovers concentrate in the shared base element, the shared drawing-helper
module, the two slot-driven selection components (the combo dropdown and the listbox),
the slider's knob geometry, the progress label computation, and the spinner's animation
pacing — but the same rule should be swept across every component's internals.

Be careful about the live look-alikes: internal plumbing that is genuinely reached — the
keyboard-driven selection chains in both selection components, their item-click handling,
the resize/relayout fallbacks some components keep, the live notification dispatchers,
the spinner's frame loop, the progress label rendering — must stay exactly as it is.

## Desired outcome

- Remove every unreachable internal helper identified under the rule, wherever it sits
  in the library, together with any state fields, private helpers, geometry/pacing
  computations, or module-level functions that exist only to serve them.
- Remove, do not disable: no emptied-out method bodies, visibility downgrades, or
  synthetic calls added just to make a leftover look referenced.
- The cleanup must be complete in both directions: deleting a helper must also take with
  it every other helper that was only reachable through it, and must not leave any
  remaining code in another module referencing a deleted member.
- The package must still compile cleanly under the project's strict settings (the
  build treats an unused private member as an error, so stranded private helpers will
  surface mid-cleanup — use that as a signal, but not as the only one: protected members
  and mutually referencing module functions that the build tolerates are exactly the
  residues this pass exists to remove).
- Do this purely as an internal cleanup of the production sources; the demo pages,
  documentation, build config, and any test scaffolding are out of scope.

## Compatibility boundary (must remain unchanged)

- The public API: exported classes and types, custom element names, their attributes and
  properties, CSS custom properties, and static styles.
- The event contract: every event live code fires today still fires, with the same name
  and payload — including the slider's value-change event and the selection components'
  item-selected notifications. A retired event name from an older release must not be
  resurrected.
- Behavior of live flows: template rendering and the roughjs-drawn geometry, keyboard
  navigation in the selection components, the drop-panel show/hide cycle, the spinner's
  animation pacing, and the progress percentage label and fill computation.
