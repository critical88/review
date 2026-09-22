# Refactor request: the sidebar package's cross-cutting decisions all route through one coordinator

Repository: `react-pro-sidebar` (TypeScript, React 18, emotion styled-components,
popperjs). Install and build: `yarn install --frozen-lockfile --ignore-engines`
then `yarn build`; the full suite runs with `yarn test`. Please make your
changes in the production source under `src/` so that everything keeps
building, type-checking and passing the suite as-is.

## Maintainer observation

The last several maintenance rounds in this package converged on one pattern we
did not fully intend. We kept asking "where should this rule live?", and the
practical answer slowly became "wherever the coordinator lives". Today the
responsive sidebar shell, the whole menu tree (menu, menu button, menu item,
submenu, submenu content), and two of our hooks ((a) the media-query hook that
drives responsiveness and (b) the popper hook for floating submenus) all import
and consult a single coordinator object — a singleton class instance sitting in
its own module — for most of their runtime decisions: breakpoint policy and
media-query strings, off-canvas overlay layout CSS, the menu style model and
the shared menu-button CSS, popper positioning and flyout visibility, keyboard
activation/dismiss policies, the accordion and active-trail menu-tree
transitions, and the inert handling of hidden submenu content.

The architectural diagnosis is a responsibility-concentration defect: one
declaration now owns many unrelated responsibility areas that the rest of the
tree delegates into. A fix anywhere in the package lands in that one place, every
consumer couples to it, and understanding a single behavior (say, what happens
when the sidebar collapses) requires reading a module disconnected from the
components it drives.

## What we want

Dissolve the concentration. Each responsibility area should live in a focused,
cohesive home — inside the component/hook that owns the behavior, or in a
small single-purpose module beside it — so that a change to one area (styling,
responsiveness, positioning, keyboard, menu-tree state) touches only the code
that genuinely owns that area. Redistributing the members into *new* broadly
mixed hubs, or keeping a pass-through shape where consumers still call into one
shared object for unrelated concerns, does not resolve the complaint: what we
need is for each concern to have a home whose scope is that concern.

Treat this as a behavior-preserving architectural refactor. In particular:

- Re-home the decisions, keep the behavior. The runtime effects the package
  documents and tests must be indistinguishable from today's.
- A cohesive repair is expected to be real relocation of responsibility, not
  renaming, hiding, or mechanically reshaping the central module.
- Keep the code style and structure of the receiving modules: components stay
  components (JSX, effects, context wiring), hooks stay hooks (state and effect
  skeletons with today's timing).

## Behavior and compatibility that must hold

1. The complete published suite (`yarn test`) passes without touching any test
   file or test utility. Do not modify anything under the test paths.
2. The public API and type surface keep working for consumers: the exported
   components and their props, the style-model exports (the per-element style
   resolution contract, the menu-item styles types), the menu-button style
   helper, and the internal context contracts that the menu tree exchanges.
   The package compiles under strict TypeScript with `isolatedModules`.
3. Responsive semantics are unchanged: predefined breakpoints keep their pixel
   values, custom lengths and `'all'` map to the same media queries, the
   `broken` state starts `false` on first render (SSR-safe, hydration agrees),
   it is synchronized once subscribed and on change, subscriptions are cleaned
   up, and `onBreakPoint` keeps announcing only actual changes.
4. The sidebar must keep resolving its break-point state through the same
   media-query hook export it imports today — module-boundary instrumentation
   in the ecosystem targets that hook, so rerouting the lookup behind an
   indirection or inlining it into the shell is a compatibility break.
5. Floating-submenu positioning is unchanged in observable effect: instances
   appear only for popper-mode submenus once mounted with resolvable refs,
   they are destroyed on teardown, observers and timers are cleaned up,
   placement/strategy/offset behavior stays as shipped, repositioning still
   happens after the sidebar transition settles and on content/trigger resize,
   and the flyout still portals to the same hosts (sidebar root, body
   fallback) with the same outside-click / close-on-click / keep-open
   decisions.
6. Keyboard semantics are unchanged: Enter/Space activates the overlay
   backdrop (a native-button contract, including `preventDefault`), Escape
   closes the overlay from anywhere and dismisses open flyouts, Enter toggles
   a submenu in popper mode.
7. Menu-tree semantics are unchanged: accordion groups keep a single open
   submenu and preserve the currently open one if the accordion prop flips,
   a controlled `open` prop always wins, colliding `defaultOpen`s in one
   accordion group still produce the exact single warning message we ship
   today, and active-state still cascades up the menu tree on child
   activation and deactivation.
8. Hidden submenu content keeps its inert handling for the running React
   version (React 18 renders the string form, React 19 the boolean), and the
   emotion-generated CSS for default and prop-configured cases is unchanged
   in effect.
9. Server-side rendering safety holds: no browser API is touched during
   module import or first render.
10. Do not add runtime dependencies or change build configuration.

## Where to look first

Recent activity is the guide: inspect the imports the sidebar shell, the menu
tree components and the two responsive/positioning hooks reach through, and
audit what each of those calls actually decides. Every responsibility that
reached a home outside this module's scope is a candidate for relocation. You
do not need to preserve internal helper names, member sets, signatures, or the
central module itself — only the behavior described above.
