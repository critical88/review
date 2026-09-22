# Refactor the toast pipeline: distribute the centralized manager's responsibilities

## Where this came from

`react-native-toast-message` is a React Native toast library. Over the 2.x releases our toast
pipeline was consolidated into one large manager class under `src/` that everything routes
through: the imperative `Toast.show()` / `Toast.hide()` commands, the current-toast ref
tracking, show-params and defaults resolution, the visibility lifecycle and auto-hide
scheduling, keyboard observation, the layout and animation geometry, the swipe-to-dismiss
gestures, and the renderer selection all live inside that single class, while the rest of the
pipeline around it — the primary hook, the individual hooks, the animated container, and
several small utility exports that used to own this logic — have thinned into
call-forwarding wrappers around it.

Maintainers now pay for that decision whenever they touch anything: an animation tweak, a
timer fix, or a new renderer all land in the same file, review the same wall of code, and
risk the same shared state. The class mixes concerns that have nothing to do with each other
— engineering goals, keyboard events, animation geometry, and React state plumbing all sit
side by side.

## The diagnosis we want you to act on

The manager class centralizes several **unrelated responsibilities** that do not share state
and do not collaborate; it is a god class by accretion, not by design. We want those
responsibilities returned to appropriately scoped, cohesive owners rather than one
coordinator everything flows through:

- **Imperative toast registry and commands** — module-global tracking of the currently
  mounted toast ref plus the show/hide entry points that act on it.
- **Options/defaults resolution** — base data and base options (including animation
  config resolution and the skip-undefined merge policy) and how arbitrary `show()` params
  are merged onto them.
- **Toast lifecycle state machine** — visibility/data/options state transitions, the exact
  show/hide choreography, and auto-hide scheduling driven by the visibility/options effect.
- **Auto-hide timing** — starting, clearing, and querying the pending timer, including
  cancel-on-unmount.
- **Keyboard observation** — iOS-gated listener attach/detach and the resulting visibility
  state.
- **Layout and animation geometry** — extracting dimensions from layout events with
  offsets, deriving slide `translateY` ranges for top/bottom positioned toasts, mapping
  animation types, emitting the animated styles, and imperatively driving the
  entry/exit/return animations.
- **Swipe and pan gesture handling** — the start/move decision predicates, damping
  computation, projecting gesture deltas onto the animated value, deciding when a swipe
  dismisses, and the swipe presentation (fade, spring-restore, dismiss path).
- **Renderer selection** — looking up the renderer for a toast's type, where entries from
  the caller-supplied `config` override the built-in success/error/info renderers and an
  unknown type still fails the same way it does today, and producing the rendered output.

Extract each of these so it lives on its own cohesive owner (a small class, module, or
hook-owned implementation — your choice, named however makes sense), and have the public
modules consume those owners directly instead of routing everything through the manager. If
some responsibilities are genuinely cohesive together, keeping those together is fine — the
goal is that no single class accumulates unrelated clusters like keyboard observation,
animation geometry, half a dozen gesture trampolines, and renderer lookup at once. Do not
simply rename or re-split the manager cosmetically; the distribution should be real, so a
future animation change and a future keyboard fix touch different places.

## What must keep working — behavior boundary

The task is a restructuring-semantics-preserved refactor. The public surface and all
observable behavior must remain exactly as the tests pin it:

1. **Public API stability.** Every exported name and signature that exists today must
   still exist, importable from its current module path: `Toast` (with its static
   `show`/`hide`), `ToastUI`, `useToast` and its `DEFAULT_DATA`/`DEFAULT_OPTIONS`, all
   `src/hooks` exports (`useTimeout`, `useKeyboard` with its state pair, `useSlideAnimation`
   with `translateYOutputRangeFor`, `usePanResponder` with `startShouldSetPanResponder`,
   `moveShouldSetPanResponder`, `shouldDismissView`, `useViewDimensions`), the
   `AnimatedContainer` module's `dampingFor`/`animatedValueFor`, `GestureContext`/`LoggerContext`/
   `useLogger`, and the `src/utils` helpers (`bound`, `lowerBound`, `upperBound`,
   `mergeIfDefined`, `resolveAnimationConfig`, `DEFAULT_ANIMATION_CONFIG`, …).

2. **Toast lifecycle semantics.** `show()` resolves incoming params against data/options
   defaults (skipping `undefined`), logs, applies state, and (re)schedules auto-hide
   according to the visibility/options effect — including the no-auto-hide case and clearing
   pending timers on unmount. `hide()` acts on the most recently mounted toast ref and
   forgets the ref when the component unmounts or re-renders with a different ref. Two
   mounted toast instances must never share timers, listeners, options, or lifecycle state.

3. **Interaction semantics.** Pan start/move decisions, damping for top-positioned vs other
   toasts, gesture-delta-driven animated values, swipe-dismiss thresholds and directions, and
   the precise callback order (for example, the swipe-end callback firing before the
   enabled-check) must match current behavior.

4. **Render semantics.** Slide ranges derived from container/toast geometry, animation
   driver defaults, and the renderer lookup for each toast type (caller-supplied `config`
   entries overriding the built-in success/error/info renderers, with the same failure
   behavior for unknown types) stay identical, and presentational components keep their
   current props contracts.

5. **Freshness.** Anything that calls back into React state must observe the latest render's
   callbacks and values, not first-render captures — beware of extracting bound-in methods
   into longer-lived objects.

## Working rules

- Do not modify tests, jest configuration, or the pinned build setup. The full existing
  suite must pass unmodified after your refactor.
- Keep the codebase TypeScript/React Native idiomatic; dependency structure should reflect
  cohesion (owners own their concern end-to-end) rather than an extra layer of pass-through
  calls.
- The change should be complete: obsolete delegation trampolines that no longer serve API
  stability should be removed, not left behind half-dead.
- Don't introduce new runtime dependencies or suppression comments to quiet static analysis;
  structure alone should carry the fix.
