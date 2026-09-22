# Injection design record — react-native-toast-message

## Maintenance motivation

This library began as a small declarative toast component: render a `Toast` with data and
options, animate it in and out, let the consuming app decide when to hide it. Across the 2.x
releases the feature surface grew in ways that cut across the original module boundaries:

- an imperative API (`Toast.show()` / `Toast.hide()`) that needs a module-level registry of the
  currently mounted toast ref, so callers can fire toasts without holding React state;
- auto-hide scheduling driven by the visibility/options effect;
- keyboard-avoidance for iOS;
- entry/exit slide animations whose geometry depends on measured container and toast heights;
- swipe-to-dismiss pan gestures with damping and velocity physics;
- pluggable toast renderers resolved by type;
- layout measurement math with spacing constraints.

Each of those features landed in its own pull request, and the natural reviewer conversation
was the same every time: "where should this live?" The published API could not change (apps
import `useToast`, `usePanResponder`, `bound`, `resolveAnimationConfig`, etc. by name), so
whatever answer was chosen, the existing modules had to keep exporting the same names anyway.
The tempting answer — adopted by many real React Native libraries — is to route everything
through one pipeline object: it gives maintainers a single place to read when tracing a toast's
journey, a single seam for the next feature, and it does not require touching the public
surface. The cost appears later, when that object absorbs registry state, options resolution,
timer scheduling, keyboard listeners, animation geometry, gesture physics, and renderer
selection, and every change to any of them lands in the same file with the same merge
conflicts.

## Modeled evolution

The injection models that history as a sequence of maintenance steps, each individually
defensible:

1. The imperative show/hide surface needs ref tracking, so the registry plus the command
   entry points move into a central manager class as static members, and the `Toast`
   component registers into it from its ref callback.
2. Params/defaults resolution (base data, base options, animation config, merge-undefined
   policy) follows, because `show()` needs it and the manager already resolves everything
   else about a toast.
3. The visibility lifecycle (`present`/`dismiss`/auto-hide scheduling) moves in as instance
   logic; the driving hook binds its state appliers into the manager each render.
4. The `useTimeout` hook's timer logic is absorbed the same way, since auto-hide is the same
   scheduling concern.
5. Keyboard attach/detach moves in because the manager already orchestrates the toast
   lifecycle that must react to keyboard visibility.
6. The pure calculation helpers (layout math, translateY ranges, animation mapping, damping
   and gesture projection) are absorbed as static methods — "so the geometry and the
   lifecycle that uses it live together."
7. The pan-responder decision predicates, the swipe visuals and the responder construction
   follow, unifying "everything about gestures."
8. Renderer resolution and render production are absorbed so the manager can produce the
   final toast tree.
9. The utility modules (`obj`, `array`, `animationConfig`) become stable-API shims that
   delegate into the manager, because their old implementations were absorbed.

Step by step, no single step looks wrong; the end state is one class owning nine unrelated
responsibility clusters, surrounded by modules that only forward calls.

## Overall design

- **New coordinator file.** `src/ToastManager.tsx` declares one `ToastManager` class with
  static members (registry, defaults, pure calculations, renderer selection — the parts whose
  original semantics were module-global or stateless) and instance members (lifecycle, timers,
  keyboard, gesture, swipe sessions — the parts whose original semantics were per-hook-call).
- **Seam rewiring, not API change.** Every pre-existing module keeps its exported names,
  arities and signatures; its implementation now delegates into the manager. The published
  API surface and every existing test's import surface stay valid unchanged.
- **Per-render session binding.** The absorbed logic must keep seeing fresh React renditions
  of `log`, `options`, `panning`, `animate`, the state appliers and timer cancels — exactly
  what the replaced inline closures captured on every render. Each hook calls a
  `bind{Toast,Timer,Gesture,Swipe}` entry per render, handing the manager the latest
  callbacks. This preserves closure freshness without the manager holding React state.
- **Instance vs static split.** Each hook instance constructs its own manager instance
  (`React.useMemo`), so timers, listeners and sessions stay per-mount; registry and defaults
  are static, matching the original module-global semantics. Two mounted toasts never share
  mutable lifecycle state.
- **Ordering preservation.** Every absorbed routine keeps its original operation order
  (logging before state application, timer-clear before hide callbacks, swipe-end callback
  before the enabled-check in release/terminate, keyboard listeners only on iOS, auto-hide
  restart keyed on `[isVisible, options]`), so observable sequences are identical.
- **Compatibility shims.** `utils/obj.ts`, `utils/array.ts` and `utils/animationConfig.ts`
  keep their exported helpers, now thin delegations, because consumer code imports them
  directly and the shims are part of the stable surface.

## Per-cluster explanations

The absorption sites and the production role each serves in the injected candidate:

- **`src/ToastManager.tsx` (new file).** The central pipeline class. Static side: ref registry
  (`registerToastRef`/`unregisterToastRef`/`activeToast`, imperative `showToast`/`hideToast`),
  defaults and show-params resolution (`defaultData`, `defaultAnimationConfig`,
  `defaultOptions`, `mergeOptions`, `resolveShowParams`), pure math (`layoutValue`,
  `viewDimensions`, `additiveInverse`, `animationFor`, `translateYRangeFor`,
  `slideAnimationStyles`, `slideAnimationTo`, `dampingFor`, `animatedValueFor`),
  responder predicates (`shouldStartPanOnStart`, `shouldStartPanOnMove`, `swipeDismisses`),
  renderer selection (`rendererFor`, `renderToast`), and private session-retrieval helpers.
  Instance side: toast lifecycle (`bindToast`/`presentToast`/`dismissToast`/`runAutoHide`),
  timer (`bindTimer`/`startTimer`/`clearTimer`/`timerActive`), keyboard
  (`attachKeyboard`/`detachKeyboard`), gesture dispatch (`bindGesture`/`createResponder`/
  `onGestureGrant`/`onGestureMove`/`onGestureRelease`/`onGestureTerminate`), swipe visuals
  (`bindSwipe`/`onSwipeStart`/`onSwipeEnd`/`onSwipeDismiss`/`onSwipeRestore`/
  `swipeAnimatedValueFor`). This is the single owner the model maintainer grew: one place
  where the whole toast pipeline can be read — and the reason any of its parts now requires
  touching the same class.

- **`src/Toast.tsx`.** The public `Toast` component keeps its render job but its ref callback
  now registers/unregisters into the registry side of the manager, and the module-level
  `Toast.show`/`Toast.hide` statics forward to the manager's imperative commands. Chosen
  because it is the only component that knows its own ref, so it is the natural registration
  point; production role: public API stability plus registry feeding.

- **`src/ToastUI.tsx`.** Presentational wrapper unchanged in shape; its render path calls the
  manager's render production so the presentation selection also belongs to the pipeline.
  Role: keeps `ToastUI`'s exported props contract while making the manager the render
  authority.

- **`src/useToast.ts`.** The library's primary hook. Its state machine (visibility, data,
  options) no longer applies state itself; per render it binds `log`, both option bundles,
  the state appliers and the timer cancel into a manager instance, and `show`/`hide` forward
  to the manager's lifecycle entry points. The auto-hide effect body is preserved verbatim so
  scheduling still restarts exactly on `[isVisible, options]`. Role: public hook API plus
  state ownership, with orchestration delegated.

- **`src/hooks/useTimeout.ts`.** Callback timing absorbed: the hook binds `log`, the wrapped
  callback and the delay, then forwards start/clear/active. Unmount cleanup preserved. Role:
  stable hook surface for generic timeout use, with the scheduling in the manager.

- **`src/hooks/useKeyboard.ts`.** Listener attach/detach absorbed; the hook keeps its React
  state and callback identity, binding them so the manager can add/remove the iOS-gated
  listeners in the effect. Role: keyboard-visibility state surface.

- **`src/hooks/useSlideAnimation.ts`.** Range derivation (`translateYOutputRangeFor`), style
  emission and imperative `animate` forward to the manager's geometry side; the hook's
  memoization structure is kept so the styles recompute on the same dependency cadence. Role:
  animation surface for the container.

- **`src/hooks/usePanResponder.ts`.** Start/move decision predicates and the dismiss decision
  forward to the manager's predicates; the hook binds its callbacks and materializes the
  responder from the manager, preserving the memoized-responder construction cadence. Role:
  gesture decision surface.

- **`src/hooks/useViewDimensions.ts`.** Dimension computation forwards to the manager's
  layout math with the same height/width offsets. Role: measured-layout surface.

- **`src/components/AnimatedContainer.tsx`.** The container binds position/panning/animate and
  the swipe callbacks into a manager each render; its lifecycle effect and the ref-handling
  animation restore behavior are preserved. Module-level `dampingFor`/`animatedValueFor`
  exports become delegation wrappers so the gesture utilities remain importable from their
  original home. Role: swipe/animation presentation surface.

- **`src/utils/obj.ts`, `src/utils/array.ts`, `src/utils/animationConfig.ts`.** Their
  implementations were absorbed as manager statics, so these modules remain as stable-API
  delegation shims (`mergeIfDefined`, `bound`/`lowerBound`/`upperBound`,
  `DEFAULT_ANIMATION_CONFIG`, `resolveAnimationConfig`). Role: consumer-facing API
  stability without parallel implementations.

- **Deliberately untouched.** `src/contexts/*` (cohesive providers), `src/utils/platform.ts`
  and the other leaf utilities, `src/hooks/useIsomorphicLayoutEffect.ts`, `src/types`,
  tests, docs, and build configuration. Their responsibilities are either already cohesive or
  stateless leaves, and involving them would not change any coupling shape.
