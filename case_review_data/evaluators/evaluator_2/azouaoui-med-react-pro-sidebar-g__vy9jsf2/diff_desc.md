# Change record — central decision layer for the sidebar package

## Maintenance motivation

react-pro-sidebar ships responsive-layout, menu-tree and floating-flyout
behavior to a wide user base, and over successive releases the same categories
of questions kept coming back in the issue tracker: RTL collapse bugs, layout
drift between the CSS media query and the runtime `broken` state, popper
flyouts mis-positioned after collapse transitions, keyboard activation
regressions on the overlay backdrop, and accordion groups opening two submenus
at once. Each of those fixes kept grazing more than one module because the
decision it depended on — breakpoint pixel values, off-canvas layout CSS, the
button css shared by menu items and submenus, popper options, open/active
transitions — physically lived in the component or hook that happened to host
it. Contributors asked for "one obvious place" where those rules live so that a
fix does not have to be chased through half the tree, and reviewers repeatedly
had to reconstruct the whole circuit to decide whether a change was safe.

This change consolidates those scattered runtime decisions into a
coordinator module that the shell, the menu tree and the hooks can consult.
It is an architectural grooming step on a stable, well-tested package: no new
user-facing feature is added, and behavior is meant to be interchangeable with
the previous tree.

## Development evolution modeled

The consolidation looks like a series of ordinary maintenance PRs that all
landed in the same place:

1. **Styling first.** The button css shared by `MenuItem` and `SubMenu`
   triggers was the earliest annoyance (two copies of the same template with a
   padding-level quirk). Keeping it next to the style-model types that
   `Menu` exposes moved naturally once the memoization question came up —
   re-computing an identical css string on every item render was the original
   profiling complaint.
2. **Responsive next.** The breakpoint table, the media-query derivation and
   the off-canvas layout CSS were split between the sidebar shell and its
   media-query hook; an RTL fix had to be made twice. The listener body
   followed so the "who subscribes, who synchronizes" policy could not drift
   from the query that produced it.
3. **Flyouts third.** Popper options, the mount/readiness condition, the
   resize/settle refresh and the document-level visibility decision were
   repeatedly touched together during the portal work (flyouts clipped by
   `overflow`, hydration-ordering constraints). They moved as one unit.
4. **Policies and coordination last.** Keyboard key policies and the
   accordion/active-descendant transitions are the newest hardening work; each
   moved to sit beside the rest of the decisions it depends on, leaving the
   components with their scheduling (handlers, effects, callbacks).

Each step left the React control flow in place and moved only the decision
itself, which is why the components still read as components and the hooks
still read as hooks after the change.

## Overall design

The tree gains `src/managers/ProSidebarManager.ts`: an exported class
instantiated once as `proSidebarManager`, holding a breakpoint table, a css
memo cache, and the pure decision functions for styles, breakpoints, layout
css, listener lifecycle, key policies, portal host, popper mount/options/
observation/visibility, menu-tree transitions, registration-with-warning,
and the inert-content props. The shared style-model and context **types**
it needs travel with it, and `Menu` re-exports them so existing import paths
keep resolving under the package's strict, isolatedModules setup. Consumers
import the singleton and delegate — components keep their JSX, effects,
context wiring and callbacks; hooks keep their State/effect skeletons and
timing.

Design constraints deliberately honored while moving code:

- **Effects are not downgraded.** Hooks stayed hooks: the media-query hook
  keeps its `useState`/`useLayoutEffect` shell and subscription timing (so
  SSR agreement and the layout-effect fallback behave identically); the
  popper hook keeps its instance state and effect dependencies (so creation,
  destroy and repose timing are untouched).
- **Module boundaries that package consumers or the build rely on are
  untouched.** The shell still resolves its break-point state through the
  same named media-query hook import; the popper factory is still imported
  from the core package; `Menu` continues to export the style-model contracts
  downstream code builds on.
- **The coordinator must be safe to import in any render environment.** It
  touches no browser API at construction time and every method it offers is
  either pure, lazily resource-allocating, or explicitly cleanup-returning.

## What changed, where, and why

### `src/managers/ProSidebarManager.ts` (new)

A single exported class plus a `proSidebarManager` singleton, and the shared
types (`PredefinedBreakPoint`, `BreakPoint`, the element/level style-model
types, the button style params, and the internal accordion and active-cascade
context contracts) that formerly lived with their consuming components.

What it holds, and the production role of each member family:

- **Breakpoint table + media-query derivation + listener registration +
  change-announcement policy.** Product decisions about device sizes sit next
  to the code consuming them; the registration helper reproduces the exact
  subscribe/sync/cleanup order the hook previously inlined, and the
  announcement policy keeps `onBreakPoint` from emitting spurious initial
  values.
- **Off-canvas layout CSS.** One template is used by both the CSS media query
  and the runtime `broken` class so the two layouts cannot drift apart — the
  structural reason the rule lived in the shell, and the reason the RTL
  variants (left/right offsets) are decided here.
- **Style model + button css + per-params memo.** The shared
  element-resolution used by menu items and submenus, and the css template
  duplicated by menu item and submenu triggers, with a cache that absorbs the
  repeated identical computations from every rendered item.
- **Portal host lookup, popper mount condition, options, instance creation,
  observation and the document-level visibility decision.** The positioning
  recipe (placement/strategy/offset), the readiness constraints (popper mode
  AND mounted with resolvable refs), the resize/settle refresh plan and the
  open/close outside-click/closeOnClick branching all sit together, because
  those are the pieces reviewed together whenever flyout placement regressed.
- **Keyboard policies.** Backdrop activation (native-button contract:
  Enter/Space), the shared escape policy, and the document-level popper
  toggle key. Naming the policies was a reviewer request after an accidental
  `Keydown-vs-KeyPress` regression shipped.
- **Menu-tree transitions and `defaultOpen` registration.** Single-open
  (accordion) id transitions, the active-descendant set transitions used to
  cascade active state up the tree, and the registration routine that keeps
  the "last-rendered wins" registration with its single explanatory warning.
- **Inert content props.** The React-version-sensitive representation of the
  `inert` attribute (boolean for React >= 19, string for React 18).

### `src/components/Sidebar.tsx`

The shell keeps its context, props plumbing, ref strategy, overlay focus
management and listener wiring. Its breakpoint table, media-query derivation,
layout CSS, activation/dismiss key equalities and change-announcement
condition now delegate. Why here: the shell is the composed border of the
package — it consumed these rules but never owned them alone (the media hook
shared half of them), which is precisely why both the CSS and the runtime
half pointed at the same single source after the move.

### `src/hooks/useMediaQuery.tsx`

The hook keeps the isomorphic layout effect choice, the always-false initial
state (the SSR agreement guarantee) and its effect dependency list; the
subscribe/sync/cleanup body now comes from the shared registration helper.
Why this shape: the hook is the package's only subscription timing seam —
keeping its skeleton intact preserves mount semantics while letting the
subscription policy live with the rest of the responsive decisions.

### `src/components/Menu.tsx`

The menu keeps its context providers, level propagation and open-state
ownership. The style-model **types** moved with the decision code and are
re-exported from here (preserving the public import path for style-model
consumers and downstream theme tooling). The single-open transition logic in
its `setActive` reducer is one of the absorbed rules; the provider wiring
that applies it did not move.

### `src/components/MenuButton.tsx` and `src/components/MenuItem.tsx`

The button keeps its public helper signature (`menuButtonStyles`) but the css
template and memoization now come from the central module — the two file
copies of the css recipe (button + item) were the historical reason padding
levels drifted between item and submenu rendering. The item keeps its
per-element style resolution call flow; only the underlying resolution and
level css moved. These sites delegate because they are pure presentation
bindings: the css they render is decided, not computed by local policy.

### `src/components/SubMenu.tsx`

The menu tree's most policy-dense consumer keeps its portal-into-render
ordering (SSR-safe inline first paint), its popper-mode state, its document
listeners and its context cascade. Delegations: the styled template pulls the
shared button css; the mount effect resolves the portal host; the
`defaultOpen` registration (with its warning) and both tree transitions
(accordion open id, active-descendant set) route through the central rules;
keydown/keyup equalities use the copied policies; the document-level
open/close/leave-alone branching uses the central visibility decision while
the component still owns applying the result (`nextOpen !== null`). Why this
shape: the component owns *when* transitions apply (effect ordering, state
commits) — only the *decision* moved.

### `src/hooks/usePopper.tsx`

The hook keeps instance state, the context read, and both effect dependency
arrays. The mount/readiness condition, options object, instance
creation/destroy flow and the resize/settle observation plan are delegated.
The mount effect still owns re-render signaling (state-held instance) and
cleanup destroy, so the instance lifecycle the outside world observes is
unchanged. This hook was the riskiest relocation and was deliberately kept
shell-complete for that reason.

### `src/components/SubMenuContent.tsx`

The content keeps its slide/popover presentation and visibility precedence;
only the inert attribute representation (the React 18/19 string-vs-boolean
attribute question) moved — a decision unrelated to the component's layout
mode, which is why it stopped living inline.

## Compatibility boundary

No public component signature, exported type, context contract, class-name or
data-testid changes; the props and type re-export surface the package
documents continue to resolve; the conflict between "collapse as CSS" and
"collapse as state" keeps running through the same media hook boundary; and
the module graph can still be imported and first-rendered without a DOM. The
canonical place the runtime behavior is verified against — the package's
published command suite — is exactly what this change is intended to be inert
under.
