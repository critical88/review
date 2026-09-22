# Injection design record — foldable expansion state machine (tweakpane)

## Maintenance motivation

Components in this codebase fold and animate panels with a shared reactive
state object, `Foldable` (`packages/core/src/blade/common/model/foldable.ts`),
which models five keys: `completed`, `expanded`, `expandedHeight`,
`shouldFixHeight`, and `temporaryExpanded`. Three components consume it:
folder blades (a folder is a collapsible container of blades), the color
input binding's inline swatch picker, and the point-2d input binding's inline
coordinate pad picker. The inline pickers are laid out under their trigger
element and animate their height, so they reuse exactly the same transition
bookkeeping the folder needs: measure a fixed height before expanding,
temporarily flip an internal key to measure, clean up when the CSS transition
ends, and mark the state machine completed again.

A believable maintenance pressure drives this code towards consumer-owned
logic: each component wants slightly different treatment of its expansion
surface. The folder's container element holds child blades and must finish
its transition when the rack contents change; the color picker applies
expansion classes to its own wrapper element and only animates a height in
the inline layout; the point-2d picker animates its pad element and must also
restore focus after expanding. When a change comes in, the path of least
resistance during a feature cycle is to handle the foldable's keys right
where the element lives instead of extending the shared model, because
touching the shared model affects all three components at once.

## Normal development evolution being modeled

The upstream design keeps the state interpretation with the data: `Foldable`
offers derived accessors (`styleExpanded`, `styleHeight`), a class-application
helper (`bindExpandedClass`), and completion bookkeeping
(`cleanUpTransition`), and the module depends on it. The injected change
models the organic outcome of several component-driven maintenance cycles:
each consumer took over interpreting the raw keys for its own element, and
the shared model was eventually reduced to a plain key-value container. The
per-consumer copies then drifted apart in shape even though they encode the
same rules. This is a natural lifecycle for reactive-model codebases in this
domain: a state-normalization cleanup ("make the model a dumb container"), a
round of per-component refinement, and no follow-up pass that would pull the
behavior back down into the model.

## Overall design

The changes reduce `Foldable` to its raw key set and distribute the state
interpretation to each consumer skeleton (class constructors plus trailing
methods), plus supporting commentary explaining the measurement flow. In
detail, the expansion relation keeps its three-step contract with each
consumer: before-change measurement choreography (temporarily expanded,
transitions disabled, forced reflow, measure, restore, re-enable), on-change
presentation sync (height derived from `shouldFixHeight` and
`expandedHeight`, combined CSS class application), and explicit completion on
transition end or on rack mutation. Modules consume the foldable in place of
the removed model-owned helpers.

The folder blade illustrates: its controller wires the same beforechange and
change emitters directly, computes the height, and owns the transitionend
vocabulary; the view applies the corresponding classes from the same shared
model. The color and point-2d pickers each work similarly from their own
layout element, inlined next to their config branches. Structural variation
crossects components and tiers: the folder and point-2d pair have their
height logic in a controller (with the point-2d consuming a nullable picker
element), the color view combines class and height application in a single
view method (for the inline layout only), and the color controller runs the
measurement choreography as a separate unit so the view can subscribe
independently. The layer where the interpretation lives is not uniform, which
matches how such drift accumulates over time: the folder view's relation to
the state is through a plain class-name helper call with no derived state
(where the color view's is derived), and the point-2d controller guards
against a null element since its popup layout doesn't have one.

## Per-location rationale

### `packages/core/src/blade/common/model/foldable.ts`

The model's derived accessors (`styleExpanded`, `styleHeight`), the
class-application helper (`bindExpandedClass`), the completion helper
(`cleanUpTransition`), and the module-level binding helpers
(`computeExpandedFolderHeight`, `applyHeight`, `bindFoldable`) are dropped
from the file, and `Foldable` becomes `ValueMap<FoldableObject>` plus `static
create`. This is the site that creates the vacuum that makes the envy
readable across the codebase: with the model reduced to a raw key container,
no consumer can import the expansion rules, and each has to invent its own
substitute. The shape fits the natural evolution story (a "state-only model"
cleanup) and follows the existing TypeMap constructor pattern, so the file
still reads as if the original author intended it.

### `packages/core/src/blade/folder/controller/folder.ts`

`FolderController` adds the folder's copy of the state interpretation:
`applyExpandedHeight_` derives the container height
(`0px` when collapsed, the measured height under `shouldFixHeight`, `auto`
otherwise), `beginExpansionTransition_` performs the beforechange measurement
choreography, `finishExpansionTransition_` restores completion flags, and
`onContainerTransitionEnd_` completes on the `transitionend` event's
`height` property. The constructor wires subscriptions to the foldable's
emitters, the container element, and a `finishExpansionTransition_` on
rack add/remove to cover the empty-folder case where the container height
never changes and `transitionend` does not fire. This site is the canonical
manifestation because the folder is the heaviest foldable consumer and the
rack element is only accessible between the controller's view and rack
controller; interpretation in the controller keeps the element integration
in one place, which is exactly where a maintainer would put it during this
cleanup.

### `packages/core/src/blade/folder/view/folder.ts`

`FolderView` now holds the foldable and applies the `expanded` and `cpl`
(expanded, completed) classes itself via `syncExpansionClasses_`, driven by
the foldable's change emitter and an initial call at construction. This
makes the presentation layer interpret the same raw keys independently of
the folder controller, mirroring the class-application helper that used to
come with the model but scoped per view. Keeping the folder's class
application in the view (not the controller) reflects the natural split in
this codebase where classes are a view concern, and it means the folder
relation shows up in both tiers of one component.

### `packages/core/src/input-binding/color/controller/color.ts`

The inline-picker branch of `ColorController` wires a beforechange
choreography (`beginPickerTransition_`), a transitionend completion handler
(`onPickerTransitionEnd_`), and completion untouched on `finishPickerTransition_`
onto its picker element, guarding on the element's presence. The popup
branch stays as it was, because popup pickers do not animate an inline
height; expansion for them mirrors through `connectValues` onto the popup's
`shows` flag. Interleaving the inline branch with the existing
`connectValues` popup branch matches how the surrounding code already
forks on `pickerLayout`, and gives the choreography role a second,
component-specific manifestation.

### `packages/core/src/input-binding/color/view/color.ts`

`ColorView` applies the expansion classes to its own wrapper element and,
when the inline layout is active, the gated picker height (`applyFoldable_`),
all derived from the raw foldable keys, plus focus-neutral listeners wired in
its constructor. This is a third distinct implementation site for the same
rules: classes plus inline height in one view method, whereas the folder
split that responsibility across a controller method and a view method.
Inlining both was the natural step for this smaller view, again matching
how each drift repeats the rules rather than extracting them.

### `packages/core/src/input-binding/point-2d/controller/point-2d.ts`

`Point2dController` adds `applyExpandedHeight_` (deriving picker-element
height, including the `auto` fallback), `beginPopupTransition_` (the
beforechange choreography), `finishPopupTransition_`, and
`onPopupTransitionEnd_`, attached in the inline-pad branch of its
constructor, alongside a null-element guard in each. As with the color
binding, the popup branch keeps using `connectValues` alone. This gives a
third consumer-side manifestation with its own selection of roles (height
and choreography in the controller, no view involvement), extending the
same interpretation pattern to a component whose view is Value-driven.

## Structural variation across manifestations

The envied relation is implemented in three components and four structural
variants: controller-owned height plus view-owned classes (folder),
view-owned classes plus view-owned height combined (color), and
controller-owned height with controllers also owning the choreography
(point-2d). The height application targets three different elements (folder
container, picker wrapper, pad container), two of which are optionally
present per layout mode, so the conversation about what "the foldable
rules" mean now has to happen per component. This diversity is what makes
the drift realistic rather than a single copy-pasted unit, and it prevents
the case from bottoming out at a repeated syntactic edit.

## Boundary of the change

All edits sit inside the foldable expansion machinery and its consumers
inside `packages/core/src/blade/folder`, `packages/core/src/input-binding/color`,
and `packages/core/src/input-binding/point-2d`, along with the shared model
file. Test files, styles, the popup-layout behavior, and every other
binding keep their original shape, and the public API (folder `expanded`,
input binding `expanded`/`pickerLayout` params, import/export payloads)
is unchanged, so a solver can verify their work against the existing
behavior contract.
