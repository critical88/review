# Injection design record — wired-elements internal-surface leftovers

## Maintenance motivation

The repository sits on the `3.0.0-rc` line of wired-elements: a hand-drawn web-component
library built on lit 2 with roughjs geometry, one custom element per source module plus
a shared `wired-base` element and a shared `wired-lib` drawing-helper module. The release
branch has just absorbed a cluster of internal reworks — the slider had its bar and knob
geometry enlarged, the progress element had its label/fill handling restated, the selection
family (combo and listbox) was moved fully onto slot-driven selection, and the spinner
was retimed around the frame clock. The natural pre-GA chore this models is the cleanup
pass maintainers actually do before a major GA: retiring the internal helpers those
reworks displaced.

Real reworks of this shape routinely strand helpers at both ends of a call chain. A
component-side pass is deleted in one commit while the base-element hook it drove stays
behind "because other components use it"; an index-based selection entry survives "for
hosts that drive the component from outside"; an old event-name dispatcher is kept "for
backwards compatibility"; a duplicated pacing or geometry helper is kept "for the next
pass". Because the library's public API is a component contract, the stranded material
concentrates in the internal surface — protected members read like extension points, and
cross-file references into a shared base class look like usage.

## Modeled evolution

Every insertion is written as the residue of a plausible historical layer, not as new
feature work. The comments describe why the site was "kept" — measurement sweeps, external
hosts, older event names — which is the story such leftovers tell in a real component
library. No new imports, public API, or external behavior are introduced; every added line
sits below the public surface. The result should read as if the current HEAD is a tree
partway through that pre-GA cleanup, where some components have already had their
redundant paths removed and others have not yet had the follow-up pass.

## Overall design

The additions spread over three tiers of the library so that the residue is not a
single-file phenomenon:

- the shared tiers: a protected size-recording pair on `wired-base`, and two
  non-exported geometry functions at the tail of `wired-lib`;
- the selection family: the same leftover pattern stated twice, once in `wired-combo`
  and once in `wired-listbox`, with each component's own notification idiom
  (`fireEvent` vs. the `this.fire` wrapper);
- four single-component paths: slider knob geometry, progress label math,
  spinner animation pacing.

Two plausibility constraints shaped the code. First, the project compiles with
`noUnusedLocals`/`noUnusedParameters`, under which the compiler rejects a private member
with no textual use; so leftover private members are written as consumers of other
leftovers (a private dispatcher called by a stranded protected entry, a private pivot
computation fed by a stranded field) rather than as naked singletons. Second, protected
members are not compiler-checked, so the protected surface carries isolated fields and
entries with zero references, exactly as an un-removed "extension point" would look.
Comments in the surrounding code style give each site the tribal-knowledge flavor
maintainers leave on members they hesitate to delete.

## Per-cluster explanations

### `src/wired-base.ts` — size-recording pair on the shared base element

Added a `protected lastPanelSize` cache plus a `protected syncPanelSize(size)` hook that
compares, records, and force-re-renders. Site chosen because a measurement hook on a
shared base class is the classic survivor of "component-side caller removed, base-side
hook stayed": it sits next to the real re-measure loop (`updated` -> `wiredRender`) and
mimics its vocabulary. It is also the anchor of a cross-file story: two other components'
leftovers (below) route through it, which is precisely how base-class members accumulate
references that look like usage.

### `src/wired-lib.ts` — module-level geometry pair

Added two non-exported module functions, `viewportPadding` (inline panel insets with a
collapse-to-raw-scale branch) and `scaleForViewport` (scale factors of the padded
viewport). Appended at the tail of the drawing-helper module, adjacent to real geometry
helpers, imitating the module's exported signatures and object-return style. Each
function's only textual caller is the other — the shape of two halves of one retired draw
path where removing one felt riskier than keeping both. The pair needed mutual references
also because the compiler's unused-local check would otherwise reject the file outright.

### `src/wired-slider.ts` — knob geometry trio

Added `protected knobSize` (a nominal thumb radius whose comment explains that current
sizes no longer derive from it), `protected recenterKnob(size)` (a manual
synchronize-then-move pass: record through the base hook, compute a pivot, translate the
knob element), and `private knobPivot(size)` reading the radius field. This component was
selected because its geometry really was just enlarged upstream, making a retained older
sizing pass the most plausible leftover narrative in the tree; the method bodies reuse
the component's own `this.knob` element and transform idiom.

### `src/wired-combo.ts` — index selection plus retired event name

Added `protected selectByIndex(index)`— programmatic selection by position that mirrors
the live keyboard stepping, with a comment about embedding hosts — plus `private
emitSelectEvent()` re-firing the retired `select` event name alongside the live
`selected` notification. The combo already has the real slot-driven selection flow next
to the drop-card popup; the additions copy its idiom (`itemNodes`, value sync,
`refreshSelection`) so they read as the code the slot rework replaced.

### `src/wired-listbox.ts` — the selection family's second instance

Added `protected stepToIndex(index)` plus `private emitSelectEvent()`, deliberately
parallel to the combo's pair. The listbox implemented with the same slot-driven
selection rework, so the same leftover pair plausibly exists on both sides of the family —
its dispatcher duals the combo's through the component's own `this.fire` wrapper. Duplicating
the pattern across two components models a family-wide change whose cleanup was only half
finished, and forces any removal decision to be applied consistently rather than once.

### `src/wired-progress.ts` — label-span helper

Added `protected labelSpan(size)` returning a percentage-of-px label string while routing
through the base hook and reading its cache. Its comment invokes a fixed-width label
panel from an earlier layout. This component was chosen because its label handling was
restated upstream; a helper that "synchronizes then measures" belongs to the same retired
measurement story as the base hook and would plausibly have been its second caller.

### `src/wired-spinner.ts` — pacing helper

Added `protected tickInterval()`; a duration-over-60th interval implying the evenly spaced
timing of the pre-rework spinner, with a subtle permanently-false shape: it reads the
`frame` handle in a way that only looks live. The component's animation loop really was
rewritten around `requestAnimationFrame`, making an old interval helper the most credible
residue here; the body's reuse of `spinning`/`duration`/`timerstart` keeps it native to
the file's own state vocabulary.

## Consistency notes

All insertions are additions below the public surface: no export, decorator, custom
element registration, property, event, or CSS contract is touched, and the library's
public behavior is unchanged. Every body reuses the host file's own types (`Point`), state
vocabulary, and call idioms so the additions cannot be separated from the surrounding
work by style. Reference counts were deliberately balanced per strategy described above:
zero-reference protected singletons for pure survivors, and consumer chains for the
private material the strict compiler would otherwise flag.
