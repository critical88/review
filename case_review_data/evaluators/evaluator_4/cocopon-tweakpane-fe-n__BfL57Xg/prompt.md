# Refactoring task: expansion animation logic is scattered and drifting

## Where this came from

I was fixing an inconsistency in how collapsible regions animate and kept
finding the same logic in several places. Panels in this library can fold
and unfold — folder blades collapse their child-blade container, and the
color and point-2d inputs can render their pickers inline and collapse them
under the trigger element. All of these run on the same shared reactive
state object that tracks whether a region is expanded, what height it was
measured at, and a couple of internal flags the animation flow depends on.

Somewhere along the way that state object became a dumb key-value holder,
and each component that renders an expanding region now reads and writes
those keys directly: it derives the `0px`/measured/`auto` height itself,
juggles a temporary-expansion flag to measure the target height during the
before-change flow, decides when the animation is complete, and toggles the
expanded/completion CSS classes on its element. Because every component
re-derives the same rules from the raw keys, the copies have drifted: one
component applies classes in its view, another in its controller; one
guards for a missing element because of its alternate popup layout; one
forces completion when its child contents change because the height
transition can't fire callbacks in that case. I wanted to tweak what
"animate to measured height" means and had to edit three different
components to do it, and I'm still not confident I found every copy.

## What I'd like

Please track down where the interpretation of this expansion state is
implemented across the folder and inline-picker code paths and consolidate
it so the rules have one authoritative home instead of being re-implemented
per component against the raw keys — placed with whichever part of the layering
genuinely owns responsibility for that state. Each consuming component should
end up with only the wiring that is genuinely local to it — subscribing to
the shared object, passing in its DOM elements, and handling events. If a
component-specific behavior (the rack-contents completion case,
popup-versus-inline layout differences) makes a rule genuinely differ, that
difference should be expressed inside the shared logic rather than by
duplicating it elsewhere. Clean up code that becomes dead once the rules are
properly placed. I care that the result reads like something the next
maintainer can reason about, not about a specific method layout.

## What must not change

- The visible behavior: clicking a folder title toggles it and animates the
  container height, including the temporary-measure flow during expansion;
  folders whose height doesn't change (think empty folders, or children
  added/removed mid-animation) still end up in the completed state; the
  expanded and completed CSS classes are applied and removed exactly as
  before, including on initial render; inline color and point-2d pickers
  keep animating their height inline, while popup pickers keep mirroring the
  expanded state without inline height animation; expanding a picker focuses
  its first control, and Escape returns focus to the trigger button.
- The package's public API: the folder `expanded` property, the
  `expanded` and `pickerLayout` params of the color and point-2d bindings,
  and folder state import/export continue to work as they do today.
- The build and tests: run `npm ci` at the repository root and
  `cd packages/core && npm run test:ts:dynamic`. The suite passes today and
  must pass with the same set of results after your change.

Please investigate the whole subsystem rather than the first component you
happen to find, and address every instance of this pattern you can locate —
the point is that I shouldn't have to chase these rules down again.
